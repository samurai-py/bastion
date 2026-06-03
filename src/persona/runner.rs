// Runner — executes a RouterDecision by dispatching to persona(s).
// Single: one provider call for the selected persona.
// Parallel: JoinSet fan-out, each task returns (PersonaId, Result) — tagged by RETURNED id,
//           never by spawn order (Pitfall 4 / CF-3 / T-02-07).
// Cabinet: returns ConveneCabinet(decision) — orchestrator (plan 05) takes over.

use tokio::task::JoinSet;

use crate::hooks::egress::check_egress;
use crate::persona::router::RouterDecision;
use crate::persona::{Persona, PersonaRegistry};
use crate::provider::SharedProvider;

/// Stable identifier for a persona within a run.
pub type PersonaId = String;

/// Output of a single runner invocation.
#[derive(Debug)]
pub enum RunnerOutput {
    /// Single persona responded; carries (persona_id, response_text).
    Single(PersonaId, String),
    /// Multiple personas responded in parallel; each entry is (persona_id, response_text).
    /// Entries are in JoinSet completion order — callers must NOT assume any fixed ordering.
    Parallel(Vec<(PersonaId, String)>),
    /// Cabinet mode — hand this decision to the Cabinet orchestrator (plan 05).
    ConveneCabinet(RouterDecision),
}

/// Execute the `RouterDecision` against the registry + provider.
///
/// - `msg` is the original user message (threaded through so each persona completion
///   receives the user turn directly).
/// - The `SharedProvider` (`Arc<RwLock<Box<dyn Provider>>>`) is cloned into each
///   JoinSet task; the read-lock is acquired **inside** each task (loop_.rs:118-125 pattern).
pub async fn run(
    decision: RouterDecision,
    registry: &PersonaRegistry,
    provider: SharedProvider,
    msg: &str,
) -> anyhow::Result<RunnerOutput> {
    match decision.mode {
        crate::persona::router::ResponseMode::Single => {
            run_single(decision, registry, provider, msg).await
        }
        crate::persona::router::ResponseMode::Parallel => {
            run_parallel(decision, registry, provider, msg).await
        }
        crate::persona::router::ResponseMode::Cabinet => {
            Ok(RunnerOutput::ConveneCabinet(decision))
        }
    }
}

// ---------------------------------------------------------------------------
// Single execution
// ---------------------------------------------------------------------------

async fn run_single(
    decision: RouterDecision,
    registry: &PersonaRegistry,
    provider: SharedProvider,
    msg: &str,
) -> anyhow::Result<RunnerOutput> {
    let persona_id = decision
        .personas
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("RouterDecision.personas is empty for Single mode"))?;

    // Fail-closed egress gate (CF-1, PRIV-03): check before EVERY provider call.
    // Resolve name first (no payload held across lock), then tier, then gate.
    let provider_name = provider.read().await.name().to_owned();
    let tier = registry.get(&persona_id).map(|p| p.tier);
    check_egress(tier, &provider_name)?;

    let system_prompt = get_system_prompt(registry, &persona_id);
    let full_prompt = build_prompt(&system_prompt, msg);

    let text = {
        let guard = provider.read().await;
        guard.complete_simple(&full_prompt).await?
    };

    Ok(RunnerOutput::Single(persona_id, text))
}

// ---------------------------------------------------------------------------
// Parallel execution (JoinSet — tagged by RETURNED PersonaId, not spawn order)
// ---------------------------------------------------------------------------

async fn run_parallel(
    decision: RouterDecision,
    registry: &PersonaRegistry,
    provider: SharedProvider,
    msg: &str,
) -> anyhow::Result<RunnerOutput> {
    let mut set: JoinSet<(PersonaId, anyhow::Result<String>)> = JoinSet::new();

    for persona_id in decision.personas {
        let system_prompt = get_system_prompt(registry, &persona_id);
        let full_prompt = build_prompt(&system_prompt, msg);
        let provider_clone = provider.clone(); // clone Arc, not the inner value
        // Capture tier before spawning — registry not Send so resolve here.
        let tier = registry.get(&persona_id).map(|p| p.tier);

        set.spawn(async move {
            // Fail-closed egress gate (CF-1, PRIV-03): resolve provider name INSIDE task,
            // check before ANY provider call. Never bypass on block.
            let provider_name = provider_clone.read().await.name().to_owned();
            if let Err(e) = check_egress(tier, &provider_name) {
                return (persona_id, Err(e));
            }

            // Acquire read lock INSIDE the task — loop_.rs:118-125 pattern.
            let text = {
                let guard = provider_clone.read().await;
                guard.complete_simple(&full_prompt).await
            };
            // Return (persona_id, Result) — tag by THIS task's persona_id,
            // never by spawn order (T-02-07 / Pitfall 4 / CF-3).
            (persona_id, text)
        });
    }

    let mut results: Vec<(PersonaId, String)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok((pid, Ok(text))) => results.push((pid, text)),
            Ok((pid, Err(e))) => {
                tracing::warn!(persona_id = %pid, error = %e, "parallel persona call failed");
                errors.push(format!("{pid}: {e}"));
            }
            Err(join_err) => {
                tracing::warn!(error = %join_err, "JoinSet task panicked");
            }
        }
    }

    if results.is_empty() {
        anyhow::bail!("all parallel persona calls failed: {}", errors.join("; "));
    }

    Ok(RunnerOutput::Parallel(results))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_system_prompt(registry: &PersonaRegistry, persona_id: &str) -> String {
    registry
        .get(persona_id)
        .map(|p: &Persona| p.system_prompt.clone())
        .unwrap_or_default()
}

fn build_prompt(system_prompt: &str, user_msg: &str) -> String {
    if system_prompt.is_empty() {
        user_msg.to_string()
    } else {
        format!("{system_prompt}\n\n{user_msg}")
    }
}

// ---------------------------------------------------------------------------
// Tests (offline — MockProvider only, no live LLM)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::router::{ConveneReason, ResponseMode, RouterDecision};
    use crate::persona::{Persona, PersonaRegistry};
    use crate::provider::{Provider, SharedProvider};
    use crate::types::{CallConfig, LlmResponse, Message};
    use crate::memory::PrivacyTier;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // --- MockProvider ---
    // Echoes the persona name embedded in the prompt (system prompt = "You are <name>.")
    // so attribution tests can verify each (id, text) pair is self-consistent.

    struct EchoProvider;

    #[async_trait]
    impl Provider for EchoProvider {
        async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
            unimplemented!()
        }
        async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String> {
            // Return "echo:<persona_name>" extracted from "You are <name>." in the system prompt.
            // The prompt is "<system_prompt>\n\n<user_msg>".
            let name = prompt
                .lines()
                .find(|l| l.starts_with("You are "))
                .and_then(|l| l.strip_prefix("You are "))
                .and_then(|s| s.strip_suffix('.'))
                .unwrap_or("unknown");
            Ok(format!("echo:{name}"))
        }
        fn context_limit(&self) -> usize { 8192 }
        fn model_name(&self) -> &str { "echo" }
        fn name(&self) -> &'static str { "echo" }
    }

    fn make_provider() -> SharedProvider {
        Arc::new(RwLock::new(Box::new(EchoProvider) as Box<dyn Provider>))
    }

    fn make_registry_with(names: &[&str]) -> PersonaRegistry {
        let mut personas = HashMap::new();
        for name in names {
            personas.insert(
                name.to_string(),
                Persona {
                    name: name.to_string(),
                    description: None,
                    system_prompt: format!("You are {name}."),
                    tier: PrivacyTier::CloudOk,
                    weight: 0.5,
                    skills: vec![],
                },
            );
        }
        PersonaRegistry::new_from_map(personas)
    }

    fn single_decision(persona: &str) -> RouterDecision {
        RouterDecision {
            personas: vec![persona.to_string()],
            owner: "user1".to_string(),
            mode: ResponseMode::Single,
            convene_reason: None,
        }
    }

    fn parallel_decision(personas: &[&str]) -> RouterDecision {
        RouterDecision {
            personas: personas.iter().map(|s| s.to_string()).collect(),
            owner: "user1".to_string(),
            mode: ResponseMode::Parallel,
            convene_reason: None,
        }
    }

    fn cabinet_decision(personas: &[&str]) -> RouterDecision {
        RouterDecision {
            personas: personas.iter().map(|s| s.to_string()).collect(),
            owner: "user1".to_string(),
            mode: ResponseMode::Cabinet,
            convene_reason: Some(ConveneReason::HighWeight),
        }
    }

    // --- Tests ---

    #[tokio::test]
    async fn single_mode_returns_one_result() {
        let registry = make_registry_with(&["Aria"]);
        let provider = make_provider();
        let output = run(single_decision("Aria"), &registry, provider, "hello")
            .await
            .expect("run failed");

        match output {
            RunnerOutput::Single(id, text) => {
                assert_eq!(id, "Aria");
                assert_eq!(text, "echo:Aria");
            }
            other => panic!("expected Single, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parallel_mode_returns_all_tagged_correctly() {
        // Attribution test (T-02-07 / CF-3): 3 personas in parallel; each returned
        // (id, text) pair must be self-consistent regardless of completion order.
        let names = ["Alpha", "Beta", "Gamma"];
        let registry = make_registry_with(&names);
        let provider = make_provider();
        let output = run(parallel_decision(&names), &registry, provider, "query")
            .await
            .expect("run failed");

        match output {
            RunnerOutput::Parallel(results) => {
                assert_eq!(results.len(), 3, "expected 3 results");
                for (id, text) in &results {
                    // Each text must echo the correct persona name, proving attribution
                    // is by returned PersonaId — not by spawn order.
                    let expected = format!("echo:{id}");
                    assert_eq!(
                        text, &expected,
                        "attribution mismatch: persona={id} but text={text}"
                    );
                }
            }
            other => panic!("expected Parallel, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cabinet_mode_returns_convene_sentinel() {
        let registry = make_registry_with(&["Saúde", "Aria"]);
        let provider = make_provider();
        let decision = cabinet_decision(&["Saúde", "Aria"]);
        let output = run(decision, &registry, provider, "chest pains")
            .await
            .expect("run failed");

        match output {
            RunnerOutput::ConveneCabinet(d) => {
                assert_eq!(d.mode, ResponseMode::Cabinet);
                assert_eq!(d.convene_reason, Some(ConveneReason::HighWeight));
            }
            other => panic!("expected ConveneCabinet, got {other:?}"),
        }
    }
}
