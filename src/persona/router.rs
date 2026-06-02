// Router — classifies an inbound message into a typed RouterDecision.
// DECIDES only; does not execute personas.
// 3-attempt serde-parse-retry on complete_structured output (AI-SPEC §4b).
// Safe fallback to single persona + review flag on parse exhaustion (CF-2, T-02-09).

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use crate::persona::PersonaRegistry;
use crate::provider::Provider;

// ---------------------------------------------------------------------------
// RouterDecision types — VERBATIM from spec §2 / AI-SPEC §4b
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    Single,
    Parallel,
    Cabinet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConveneReason {
    HighWeight,
    MultiDomainConflict,
    GoalImpact,
    ManualOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RouterDecision {
    /// PersonaId values to invoke.
    pub personas: Vec<String>,
    /// OwnerId (MESH-ready, multi-owner-aware).
    pub owner: String,
    pub mode: ResponseMode,
    /// Some(..) only when mode == Cabinet.
    pub convene_reason: Option<ConveneReason>,
}

// ---------------------------------------------------------------------------
// route() — the main entry point
// ---------------------------------------------------------------------------

/// Classify `msg` into a `RouterDecision` using the provider as a cheap 0.0-temp
/// classification call.  Retries serde-parse up to 3 attempts; on exhaustion returns
/// a safe single-persona fallback + logs `router_safe_fallback` (CF-2, AI-SPEC §6).
pub async fn route(
    provider: &dyn Provider,
    registry: &PersonaRegistry,
    msg: &str,
    owner: &str,
) -> anyhow::Result<RouterDecision> {
    let schema = schemars::schema_for!(RouterDecision);
    let response_schema = serde_json::to_value(&schema)
        .map_err(|e| anyhow::anyhow!("failed to serialize RouterDecision schema: {e}"))?;

    let system_prompt = build_router_system_prompt(registry);

    const MAX_ATTEMPTS: u32 = 3;
    for attempt in 1..=MAX_ATTEMPTS {
        let raw = provider
            .complete_structured(&system_prompt, msg, response_schema.clone(), 256, 0.0)
            .await
            .map_err(|e| anyhow::anyhow!("router provider call failed (attempt {attempt}): {e}"))?;

        match serde_json::from_str::<RouterDecision>(&raw) {
            Ok(decision) => return Ok(decision),
            Err(parse_err) => {
                // Pitfall 7: do NOT log raw for local-only context.
                // Log metadata only (attempt count + error shape), never raw content.
                tracing::warn!(
                    attempt,
                    error = %parse_err,
                    "router output was not valid RouterDecision JSON — retrying"
                );
            }
        }
    }

    // All 3 attempts failed to yield a parseable RouterDecision.
    // Safe fallback: single persona (first in registry or empty sentinel), no convene (CF-2).
    tracing::warn!(event = "router_safe_fallback", owner, "router failed to parse after 3 attempts; falling back to safe single persona");

    let safe_persona = registry
        .names()
        .into_iter()
        .next()
        .unwrap_or("default")
        .to_string();

    Ok(RouterDecision {
        personas: vec![safe_persona],
        owner: owner.to_string(),
        mode: ResponseMode::Single,
        convene_reason: None,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_router_system_prompt(registry: &PersonaRegistry) -> String {
    let persona_list = registry
        .names()
        .iter()
        .map(|n| {
            let desc = registry
                .get(n)
                .and_then(|p| p.description.as_deref())
                .unwrap_or("general assistant");
            format!("  - {n}: {desc}")
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"You are the Bastion persona router. Given a user message, decide which personas should respond and in what mode.

Available personas:
{persona_list}

Response modes:
  - single: one persona handles the message (single domain, routine)
  - parallel: multiple personas handle the message concurrently (cross-domain, factual, no conflict)
  - cabinet: multiple personas deliberate together (high-stakes, conflicting priorities, or goal-impact)

Cabinet convene reasons (use when mode=cabinet):
  - high_weight: high-stakes message (risk to health, finance, relationships) → maps to ConveneReason::HighWeight (D-04/D-05)
  - multi_domain_conflict: multiple domains with conflicting advice
  - goal_impact: message may affect a tracked user goal
  - manual_override: user explicitly requested cabinet

Rules:
1. high-stakes messages MUST set mode=cabinet and convene_reason=high_weight (D-04/D-05).
2. convene_reason is ONLY set when mode=cabinet; otherwise it must be null/absent.
3. personas must contain at least one valid persona name from the list above.
4. owner is passed through from the caller.

Respond ONLY with valid JSON matching the RouterDecision schema. No prose, no markdown fences."#
    )
}

// ---------------------------------------------------------------------------
// Tests (offline — MockProvider only, no live LLM)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use crate::types::{CallConfig, LlmResponse, Message};
    use crate::memory::PrivacyTier;
    use crate::persona::{Persona, PersonaRegistry};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // --- MockProvider ---

    struct MockProvider {
        /// Scripted responses returned in order. After exhaustion returns the last one.
        responses: Mutex<Vec<String>>,
    }

    impl MockProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }

        fn always(response: &str) -> Self {
            Self::new(vec![response.to_string()])
        }

        fn sequence(responses: &[&str]) -> Self {
            Self::new(responses.iter().map(|s| s.to_string()).collect())
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
            unimplemented!("MockProvider only implements complete_structured for router tests")
        }
        async fn complete_simple(&self, _: &str) -> anyhow::Result<String> {
            unimplemented!()
        }
        fn context_limit(&self) -> usize { 8192 }
        fn model_name(&self) -> &str { "mock" }
        fn name(&self) -> &'static str { "mock" }

        async fn complete_structured(
            &self,
            _system: &str,
            _user: &str,
            _schema: serde_json::Value,
            _max_tokens: u32,
            _temperature: f32,
        ) -> anyhow::Result<String> {
            let mut responses = self.responses.lock().unwrap();
            if responses.len() > 1 {
                Ok(responses.remove(0))
            } else {
                Ok(responses[0].clone())
            }
        }
    }

    // --- Registry builder ---

    fn make_registry() -> PersonaRegistry {
        let mut personas = HashMap::new();
        personas.insert(
            "Saúde".to_string(),
            Persona {
                name: "Saúde".to_string(),
                description: Some("Health persona".to_string()),
                system_prompt: "You are Saúde.".to_string(),
                tier: PrivacyTier::LocalOnly,
                weight: 0.9,
                skills: vec!["health".to_string()],
            },
        );
        personas.insert(
            "Aria".to_string(),
            Persona {
                name: "Aria".to_string(),
                description: Some("General assistant".to_string()),
                system_prompt: "You are Aria.".to_string(),
                tier: PrivacyTier::CloudOk,
                weight: 0.7,
                skills: vec![],
            },
        );
        PersonaRegistry::new_from_map(personas)
    }

    // --- Tests ---

    #[tokio::test]
    async fn valid_single_decision_is_parsed() {
        let json = serde_json::json!({
            "personas": ["Aria"],
            "owner": "user1",
            "mode": "single",
            "convene_reason": null
        })
        .to_string();

        let provider = MockProvider::always(&json);
        let registry = make_registry();
        let decision = route(&provider, &registry, "hello", "user1")
            .await
            .expect("route failed");

        assert_eq!(decision.mode, ResponseMode::Single);
        assert_eq!(decision.personas, vec!["Aria"]);
        assert!(decision.convene_reason.is_none());
    }

    #[tokio::test]
    async fn valid_cabinet_decision_with_high_weight_is_parsed() {
        // D-04: high-stakes → Cabinet; D-05: convene_reason = HighWeight
        let json = serde_json::json!({
            "personas": ["Saúde", "Aria"],
            "owner": "user1",
            "mode": "cabinet",
            "convene_reason": "high_weight"
        })
        .to_string();

        let provider = MockProvider::always(&json);
        let registry = make_registry();
        let decision = route(&provider, &registry, "I have chest pains", "user1")
            .await
            .expect("route failed");

        assert_eq!(decision.mode, ResponseMode::Cabinet);
        assert_eq!(decision.convene_reason, Some(ConveneReason::HighWeight));
    }

    #[tokio::test]
    async fn garbage_3x_falls_back_to_safe_single_persona() {
        // CF-2: 3 consecutive unparseable outputs → safe single-persona fallback
        let provider = MockProvider::sequence(&["not json", "also garbage", "{{ invalid"]);
        let registry = make_registry();
        let decision = route(&provider, &registry, "test", "user1")
            .await
            .expect("route must not error — safe fallback");

        assert_eq!(decision.mode, ResponseMode::Single, "fallback must be Single");
        assert_eq!(decision.personas.len(), 1, "fallback must have exactly 1 persona");
        assert!(decision.convene_reason.is_none(), "fallback must not convene Cabinet");
    }

    #[tokio::test]
    async fn two_garbage_then_valid_succeeds() {
        // Retry succeeds on the 3rd attempt
        let valid = serde_json::json!({
            "personas": ["Aria"],
            "owner": "u",
            "mode": "parallel",
            "convene_reason": null
        })
        .to_string();

        let provider = MockProvider::sequence(&["garbage", "also bad", &valid]);
        let registry = make_registry();
        let decision = route(&provider, &registry, "cross-domain query", "u")
            .await
            .expect("route failed");

        assert_eq!(decision.mode, ResponseMode::Parallel);
    }
}
