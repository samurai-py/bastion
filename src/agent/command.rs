use crate::provider::SharedProvider;
use crate::provider::registry::resolve_provider;
use crate::persona::PersonaRegistry;
use crate::memory::SharedMemory;

pub enum CommandResult {
    Handled,
    Stop,
    Unknown(String),
}

/// Route slash commands from stdin.
/// Acquires write lock on provider for /model (safe — called only between turns).
///
/// Widened signature (plan 08): also accepts registry + memory for /as, /cabinet, /contest.
pub async fn handle_command(
    input: &str,
    provider: &SharedProvider,
    registry: &PersonaRegistry,
    memory: &SharedMemory,
    forced_persona: &mut Option<String>,
) -> anyhow::Result<CommandResult> {
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();

    match parts[0] {
        "/model" => {
            let model = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("/model requires a model name (e.g. /model claude-sonnet-4-5)"))?;
            let new_provider = resolve_provider(model)?;
            // Acquire WRITE lock between turns — blocks until any active stream releases READ lock
            *provider.write().await = new_provider;
            println!("Switched to model: {}", model);
            tracing::info!(event = "provider_swapped", model = %model);
            Ok(CommandResult::Handled)
        }

        "/stop" => {
            println!("Stopping daemon.");
            Ok(CommandResult::Stop)
        }

        "/as" => {
            // PERS-05: force a persona for the next turn
            let persona_name = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("/as requires a persona name (e.g. /as Aria)"))?;

            if registry.get(persona_name).is_none() {
                println!("Unknown persona '{}'. Available: {}", persona_name, registry.names().join(", "));
                return Ok(CommandResult::Handled);
            }

            *forced_persona = Some(persona_name.to_string());
            println!("Next turn will use persona: {}", persona_name);
            tracing::info!(event = "persona_forced", persona = %persona_name);
            Ok(CommandResult::Handled)
        }

        "/cabinet" => {
            // CAB-04: convene Cabinet with named personas on the next turn.
            // For now: print the personas that would be convened (deliberation on next turn
            // is triggered by the router returning Cabinet mode, which the user can force
            // by listing the intent in their message; full /cabinet override is Phase 3+).
            let personas_arg = parts.get(1).map(|s| s.trim()).unwrap_or("").trim();
            if personas_arg.is_empty() {
                println!("Usage: /cabinet <persona1> [persona2 ...]");
                println!("Available personas: {}", registry.names().join(", "));
            } else {
                let names: Vec<&str> = personas_arg.split_whitespace().collect();
                let unknown: Vec<&str> = names.iter()
                    .filter(|&&n| registry.get(n).is_none())
                    .copied()
                    .collect();
                if !unknown.is_empty() {
                    println!("Unknown personas: {}. Available: {}",
                        unknown.join(", "), registry.names().join(", "));
                } else {
                    println!("Cabinet convened with: {}", names.join(", "));
                    println!("(Cabinet deliberation will run on next message that triggers Cabinet mode)");
                    tracing::info!(event = "cabinet_convene_request", personas = %names.join(","));
                }
            }
            Ok(CommandResult::Handled)
        }

        "/contest" => {
            // D-14: explicit belief contestation escape hatch
            // /contest <id> revokes the belief with that id (owner-scoped)
            let id_str = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("/contest requires a belief ID (e.g. /contest 5)"))?;

            let id: i64 = id_str.parse()
                .map_err(|_| anyhow::anyhow!("/contest: invalid belief ID '{}' — must be an integer", id_str))?;

            // Owner-scoped revoke (IDOR guard): uses DEFAULT_OWNER for the stdin/daemon path
            let owner = crate::agent::loop_::DEFAULT_OWNER;
            {
                let mem = memory.write().await;
                mem.revoke_belief(owner, id).await
                    .map_err(|e| anyhow::anyhow!("/contest: could not revoke belief {}: {}", id, e))?;
            }
            println!("Belief {} revoked (soft-revoke — audit trail preserved).", id);
            tracing::info!(event = "belief_revoked", belief_id = id, owner = owner);
            Ok(CommandResult::Handled)
        }

        "/help" => {
            println!("Available commands:");
            println!("  /model <name>         Switch LLM provider+model (e.g. /model claude-opus-4-7)");
            println!("  /stop                 Shut down daemon");
            println!("  /as <persona>         Force persona for next turn (PERS-05)");
            println!("  /cabinet [personas..] Convene Cabinet with named personas (CAB-04)");
            println!("  /contest <id>         Revoke a belief by ID (D-14 explicit escape hatch)");
            println!("  /help                 Show this help");
            Ok(CommandResult::Handled)
        }

        _ => Ok(CommandResult::Unknown(trimmed.to_owned())),
    }
}

// ---------------------------------------------------------------------------
// Tests (offline — MockProvider + temp-DB memory)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::sqlite::SqliteMemory;
    use crate::memory::Memory;
    use crate::persona::{Persona, PersonaRegistry};
    use crate::provider::{Provider, SharedProvider};
    use crate::types::{CallConfig, LlmResponse, Message};
    use crate::memory::PrivacyTier;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;

    struct StubProvider;

    #[async_trait]
    impl Provider for StubProvider {
        async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
            unimplemented!()
        }
        async fn complete_simple(&self, _: &str) -> anyhow::Result<String> { unimplemented!() }
        async fn complete_structured(&self, _: &str, _: &str, _: serde_json::Value, _: u32, _: f32) -> anyhow::Result<String> { unimplemented!() }
        fn context_limit(&self) -> usize { 8192 }
        fn model_name(&self) -> &str { "stub" }
        fn name(&self) -> &'static str { "stub" }
    }

    fn make_provider() -> SharedProvider {
        Arc::new(RwLock::new(Box::new(StubProvider) as Box<dyn Provider>))
    }

    fn make_registry(names: &[&str]) -> PersonaRegistry {
        let mut personas = HashMap::new();
        for name in names {
            personas.insert(name.to_string(), Persona {
                name: name.to_string(),
                description: None,
                system_prompt: format!("You are {name}."),
                tier: PrivacyTier::CloudOk,
                weight: 0.5,
                skills: vec![],
            });
        }
        PersonaRegistry::new_from_map(personas)
    }

    async fn make_memory(db_path: &str) -> SharedMemory {
        let session = crate::session::SessionManager::new(db_path);
        session.init_schema().await.expect("init_schema");
        Arc::new(RwLock::new(Box::new(SqliteMemory::new(db_path)) as Box<dyn Memory>))
    }

    #[tokio::test]
    async fn contest_revokes_existing_belief() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;
        let registry = make_registry(&["Aria"]);
        let provider = make_provider();

        // Store a belief
        let id = {
            let m = mem.read().await;
            m.store_belief("_local", None, "Mario drinks coffee", "sess1", "user", false)
                .await.expect("store")
        };

        // /contest <id> should revoke it
        let mut forced = None;
        let result = handle_command(
            &format!("/contest {}", id),
            &provider,
            &registry,
            &mem,
            &mut forced,
        ).await.expect("handle_command");

        assert!(matches!(result, CommandResult::Handled));

        // Belief should be gone from retrieve_tagged
        let beliefs = {
            let m = mem.read().await;
            m.retrieve_tagged("_local", None).await.expect("retrieve")
        };
        assert!(beliefs.is_empty(), "belief must be revoked");
    }

    #[tokio::test]
    async fn as_unknown_persona_does_not_set_forced() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;
        let registry = make_registry(&["Aria"]);
        let provider = make_provider();
        let mut forced = None;

        let _ = handle_command("/as UnknownPersona", &provider, &registry, &mem, &mut forced).await.expect("cmd");
        // forced_persona must remain None — unknown persona rejected
        assert!(forced.is_none(), "forced must not be set for unknown persona");
    }

    #[tokio::test]
    async fn as_known_persona_sets_forced() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;
        let registry = make_registry(&["Aria"]);
        let provider = make_provider();
        let mut forced = None;

        let result = handle_command("/as Aria", &provider, &registry, &mem, &mut forced).await.expect("cmd");
        assert!(matches!(result, CommandResult::Handled));
        assert_eq!(forced.as_deref(), Some("Aria"), "forced must be set to Aria");
    }
}
