use crate::memory::SharedMemory;
use crate::persona::PersonaRegistry;
use crate::provider::registry::resolve_provider;
use crate::provider::SharedProvider;

/// Every real Bastion slash command — single source of truth so callers (e.g.
/// main.rs's inbound_rx arm, WEB-CMD-01) can tell "known command that's
/// console-only" apart from "not a Bastion command at all" (a Claude-Code-style
/// `/usage` typed out of habit should fall through to the normal Unknown-command
/// message, not be mislabeled "console-only" as if it would work at the console).
pub const KNOWN_COMMANDS: &[&str] = &[
    "/connect-app",
    "/model",
    "/stop",
    "/as",
    "/cabinet",
    "/contest",
    "/logs",
    "/help",
];

pub enum CommandResult {
    /// Carries the user-facing text — the stdin console prints it, the webhook
    /// channel (WEB-CMD-01) puts it straight in the JSON reply. Neither path
    /// duplicates formatting logic; this function is the only place that builds it.
    Handled(String),
    Stop,
    Unknown(String),
}

/// CR-02: generate an unguessable one-time pairing code, e.g. `BAST-7K2M-9QXR`.
/// Uses the OS CSPRNG (rand::thread_rng) — the code grants a 90-day JWT on exchange,
/// so it must not be predictable. Charset excludes ambiguous chars (0/O/1/I).
fn generate_otc() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    let pick = |rng: &mut rand::rngs::ThreadRng| -> char {
        CHARSET[rng.gen_range(0..CHARSET.len())] as char
    };
    let g1: String = (0..4).map(|_| pick(&mut rng)).collect();
    let g2: String = (0..4).map(|_| pick(&mut rng)).collect();
    format!("BAST-{}-{}", g1, g2)
}

/// Route slash commands from stdin OR a channel (WEB-CMD-01 — webhook/Telegram
/// reuse this exact router via main.rs's inbound_rx arm; `/stop` is refused there
/// for channel-sourced requests before it ever reaches here).
/// Acquires write lock on provider for /model (safe — called only between turns).
///
/// Widened signature (plan 08): also accepts registry + memory for /as, /cabinet, /contest.
/// CR-02 (plan 06-08): also accepts the shared OTC store for /connect-app.
/// `owner` scopes owner-sensitive commands (e.g. /contest) — IDOR guard now that
/// this router is reachable from multi-owner channels, not just the local console.
pub async fn handle_command(
    input: &str,
    provider: &SharedProvider,
    registry: &PersonaRegistry,
    memory: &SharedMemory,
    forced_persona: &mut Option<String>,
    otc_store: Option<&crate::channel::webhook::OtcStore>,
    owner: &str,
) -> anyhow::Result<CommandResult> {
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();

    match parts[0] {
        "/connect-app" => {
            // CR-02: mint a one-time pairing code for the mobile companion app.
            // The code is consumed by POST /auth/exchange (webhook server) within 5 min.
            let device = parts
                .get(1)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("mobile");
            match otc_store {
                Some(store) => {
                    let code = generate_otc();
                    store.write().await.insert(
                        code.clone(),
                        (device.to_string(), std::time::Instant::now()),
                    );
                    tracing::info!(event = "connect_app_otc_issued", device = %device);
                    Ok(CommandResult::Handled(format!(
                        "One-time pairing code for '{device}': {code}\n\
                         Enter it in the app within 5 minutes (POST /auth/exchange)."
                    )))
                }
                None => Ok(CommandResult::Handled(
                    "/connect-app unavailable — the webhook channel is not running.\n\
                     Start the daemon with BASTION_WEBHOOK_ADDR set, then retry."
                        .to_string(),
                )),
            }
        }

        "/model" => {
            let model = parts
                .get(1)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("/model requires a model name (e.g. /model claude-sonnet-4-5)")
                })?;
            let new_provider = resolve_provider(model)?;
            // Acquire WRITE lock between turns — blocks until any active stream releases READ lock
            *provider.write().await = new_provider;
            tracing::info!(event = "provider_swapped", model = %model);
            Ok(CommandResult::Handled(format!(
                "Switched to model: {model}"
            )))
        }

        "/stop" => {
            println!("Stopping daemon.");
            Ok(CommandResult::Stop)
        }

        "/as" => {
            // PERS-05: force a persona for the next turn
            let persona_name = parts
                .get(1)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("/as requires a persona name (e.g. /as Aria)"))?;

            if registry.get(persona_name).is_none() {
                return Ok(CommandResult::Handled(format!(
                    "Unknown persona '{}'. Available: {}",
                    persona_name,
                    registry.names().join(", ")
                )));
            }

            *forced_persona = Some(persona_name.to_string());
            tracing::info!(event = "persona_forced", persona = %persona_name);
            Ok(CommandResult::Handled(format!(
                "Next turn will use persona: {persona_name}"
            )))
        }

        "/cabinet" => {
            // CAB-04: convene Cabinet with named personas on the next turn.
            // For now: report the personas that would be convened (deliberation on next turn
            // is triggered by the router returning Cabinet mode, which the user can force
            // by listing the intent in their message; full /cabinet override is Phase 3+).
            let personas_arg = parts.get(1).map(|s| s.trim()).unwrap_or("").trim();
            let msg = if personas_arg.is_empty() {
                format!(
                    "Usage: /cabinet <persona1> [persona2 ...]\nAvailable personas: {}",
                    registry.names().join(", ")
                )
            } else {
                let names: Vec<&str> = personas_arg.split_whitespace().collect();
                let unknown: Vec<&str> = names
                    .iter()
                    .filter(|&&n| registry.get(n).is_none())
                    .copied()
                    .collect();
                if !unknown.is_empty() {
                    format!(
                        "Unknown personas: {}. Available: {}",
                        unknown.join(", "),
                        registry.names().join(", ")
                    )
                } else {
                    tracing::info!(event = "cabinet_convene_request", personas = %names.join(","));
                    format!(
                        "Cabinet convened with: {}\n\
                         (Cabinet deliberation will run on next message that triggers Cabinet mode)",
                        names.join(", ")
                    )
                }
            };
            Ok(CommandResult::Handled(msg))
        }

        "/contest" => {
            // D-14: explicit belief contestation escape hatch
            // /contest <id> revokes the belief with that id (owner-scoped)
            let id_str = parts
                .get(1)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("/contest requires a belief ID (e.g. /contest 5)")
                })?;

            let id: i64 = id_str.parse().map_err(|_| {
                anyhow::anyhow!(
                    "/contest: invalid belief ID '{}' — must be an integer",
                    id_str
                )
            })?;

            // Owner-scoped revoke (IDOR guard): the caller's real owner, not a hardcoded
            // constant — this router is reachable from multi-owner channels now (WEB-CMD-01).
            {
                let mem = memory.write().await;
                mem.revoke_belief(owner, id).await.map_err(|e| {
                    anyhow::anyhow!("/contest: could not revoke belief {}: {}", id, e)
                })?;
            }
            tracing::info!(event = "belief_revoked", belief_id = id, owner = owner);
            Ok(CommandResult::Handled(format!(
                "Belief {id} revoked (soft-revoke — audit trail preserved)."
            )))
        }

        "/logs" => {
            // M3: return only recent ERROR/WARN log entries — never conversation content.
            // Source of log_path (explicit and verifiable):
            //   1. RUST_LOG_PATH env var (user-set override)
            //   2. BASTION__LOGGING__LOG_PATH env var (config-rs env override for cfg.logging.log_path)
            //   3. fallback "bastion.log" (same default as bastion.toml)
            let log_path = std::env::var("RUST_LOG_PATH")
                .or_else(|_| std::env::var("BASTION__LOGGING__LOG_PATH"))
                .unwrap_or_else(|_| "bastion.log".to_string());
            let entries = read_recent_log_errors(&log_path, 10);
            let msg = if entries.is_empty() {
                "Nenhum erro recente nos logs.".to_string()
            } else {
                entries.join("\n")
            };
            Ok(CommandResult::Handled(msg))
        }

        "/help" => Ok(CommandResult::Handled(
            "Available commands:\n\
             \x20 /model <name>         Switch LLM provider+model (console only — daemon-wide state)\n\
             \x20 /stop                 Shut down daemon (console only)\n\
             \x20 /as <persona>         Force persona for next turn (console only — daemon-wide state)\n\
             \x20 /cabinet [personas..] Convene Cabinet with named personas (console only)\n\
             \x20 /contest <id>         Revoke a belief by ID (D-14 — also over webhook/Telegram)\n\
             \x20 /logs                 Show recent ERROR/WARN log entries (console only)\n\
             \x20 /help                 Show this help (also over webhook/Telegram)"
                .to_string(),
        )),

        _ => Ok(CommandResult::Unknown(trimmed.to_owned())),
    }
}

/// Read the most recent ERROR and WARN entries from the JSON-lines log file.
///
/// Safety contract (M3 / T-05-04-02):
///   - Extracts ONLY: timestamp, level, message.
///   - NEVER includes fields: user_input, assistant_response, text, content, or any
///     conversation payload. The caller can grep this function to verify.
///   - Returns at most `max` entries in chronological order.
///   - If the file does not exist or cannot be read, returns an empty vec (silent fail).
fn read_recent_log_errors(path: &str, max: usize) -> Vec<String> {
    use std::io::{BufRead, BufReader};

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

    // Scan the last 200 lines for efficiency — O(200) constant cost (T-05-04-04).
    let tail: Vec<&String> = lines.iter().rev().take(200).collect();

    let mut entries: Vec<String> = tail
        .iter()
        .filter_map(|line| {
            // Minimal JSON-line parsing — no extra deps beyond serde_json (already in Cargo.toml).
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            let level = v.get("level").and_then(|l| l.as_str())?;
            if level != "ERROR" && level != "WARN" {
                return None;
            }
            // Extract ONLY timestamp + level + message — NEVER user_input/assistant_response/content.
            let ts = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("?");
            let msg = v
                .get("fields")
                .and_then(|f| f.get("message"))
                .and_then(|m| m.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .unwrap_or("(sem mensagem)");
            Some(format!("[{ts}] [{level}] {msg}"))
        })
        .collect();

    // tail iterated in reverse order — restore chronological order.
    entries.reverse();

    // Return only the last `max` entries.
    let skip = entries.len().saturating_sub(max);
    entries.into_iter().skip(skip).collect()
}

// ---------------------------------------------------------------------------
// Tests (offline — MockProvider + temp-DB memory)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::sqlite::SqliteMemory;
    use crate::memory::Memory;
    use crate::memory::PrivacyTier;
    use crate::persona::{Persona, PersonaRegistry};
    use crate::provider::{Provider, SharedProvider};
    use crate::types::{CallConfig, LlmResponse, Message};
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
        async fn complete_simple(&self, _: &str) -> anyhow::Result<String> {
            unimplemented!()
        }
        fn context_limit(&self) -> usize {
            8192
        }
        fn model_name(&self) -> &str {
            "stub"
        }
        fn name(&self) -> &'static str {
            "stub"
        }
    }

    fn make_provider() -> SharedProvider {
        Arc::new(RwLock::new(Box::new(StubProvider) as Box<dyn Provider>))
    }

    fn make_registry(names: &[&str]) -> PersonaRegistry {
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

    async fn make_memory(db_path: &str) -> SharedMemory {
        let session = crate::session::SessionManager::new(db_path);
        session.init_schema().await.expect("init_schema");
        Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(db_path)) as Box<dyn Memory>
        ))
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
            m.store_belief(
                "_local",
                None,
                "Mario drinks coffee",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store")
        };

        // /contest <id> should revoke it
        let mut forced = None;
        let result = handle_command(
            &format!("/contest {}", id),
            &provider,
            &registry,
            &mem,
            &mut forced,
            None,
            "_local",
        )
        .await
        .expect("handle_command");

        assert!(matches!(result, CommandResult::Handled(_)));

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

        let _ = handle_command(
            "/as UnknownPersona",
            &provider,
            &registry,
            &mem,
            &mut forced,
            None,
            "_local",
        )
        .await
        .expect("cmd");
        // forced_persona must remain None — unknown persona rejected
        assert!(
            forced.is_none(),
            "forced must not be set for unknown persona"
        );
    }

    #[tokio::test]
    async fn as_known_persona_sets_forced() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;
        let registry = make_registry(&["Aria"]);
        let provider = make_provider();
        let mut forced = None;

        let result = handle_command(
            "/as Aria",
            &provider,
            &registry,
            &mem,
            &mut forced,
            None,
            "_local",
        )
        .await
        .expect("cmd");
        assert!(matches!(result, CommandResult::Handled(_)));
        assert_eq!(
            forced.as_deref(),
            Some("Aria"),
            "forced must be set to Aria"
        );
    }

    // ── /logs unit tests ──────────────────────────────────────────────────────

    #[test]
    fn read_recent_log_errors_empty_when_file_missing() {
        let entries = super::read_recent_log_errors("/tmp/bastion_nonexistent_log_12345.log", 10);
        assert!(entries.is_empty(), "missing file must return empty vec");
    }

    #[test]
    fn read_recent_log_errors_filters_only_error_warn() {
        use std::io::Write;
        let mut f = NamedTempFile::new().unwrap();
        // Write three JSON-lines log entries: INFO (must be excluded), WARN (must be included), ERROR (must be included).
        writeln!(f, r#"{{"timestamp":"2026-06-14T10:00:00Z","level":"INFO","fields":{{"message":"startup ok"}}}}"#).unwrap();
        writeln!(f, r#"{{"timestamp":"2026-06-14T10:01:00Z","level":"WARN","fields":{{"message":"retry triggered"}}}}"#).unwrap();
        writeln!(f, r#"{{"timestamp":"2026-06-14T10:02:00Z","level":"ERROR","fields":{{"message":"turn failed","user_input":"secret","assistant_response":"secret2"}}}}"#).unwrap();
        f.flush().unwrap();

        let entries = super::read_recent_log_errors(f.path().to_str().unwrap(), 10);

        assert_eq!(entries.len(), 2, "must return exactly WARN + ERROR entries");
        assert!(
            entries[0].contains("WARN"),
            "first entry must be WARN: {:?}",
            entries[0]
        );
        assert!(
            entries[1].contains("ERROR"),
            "second entry must be ERROR: {:?}",
            entries[1]
        );

        // CRITICAL: no conversation content must appear in formatted output.
        for entry in &entries {
            assert!(
                !entry.contains("secret"),
                "entry must NOT contain user_input/assistant_response content: {:?}",
                entry
            );
        }

        // Messages must be present.
        assert!(
            entries[0].contains("retry triggered"),
            "WARN message must appear"
        );
        assert!(
            entries[1].contains("turn failed"),
            "ERROR message must appear"
        );
    }

    #[test]
    fn read_recent_log_errors_respects_max_limit() {
        use std::io::Write;
        let mut f = NamedTempFile::new().unwrap();
        for i in 0..20_u32 {
            writeln!(f, r#"{{"timestamp":"2026-06-14T10:{:02}:00Z","level":"ERROR","fields":{{"message":"err {i}"}}}}"#, i).unwrap();
        }
        f.flush().unwrap();

        let entries = super::read_recent_log_errors(f.path().to_str().unwrap(), 5);
        assert_eq!(entries.len(), 5, "must not exceed max limit");
        // Must be the LAST 5 (most recent).
        assert!(
            entries[4].contains("err 19"),
            "last entry must be most recent: {:?}",
            entries[4]
        );
    }

    #[tokio::test]
    async fn logs_command_returns_handled() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;
        let registry = make_registry(&["Aria"]);
        let provider = make_provider();
        let mut forced = None;

        // Point RUST_LOG_PATH to a non-existent file — /logs should still return Handled.
        std::env::set_var("RUST_LOG_PATH", "/tmp/bastion_no_log_for_test.log");
        let result = handle_command(
            "/logs",
            &provider,
            &registry,
            &mem,
            &mut forced,
            None,
            "_local",
        )
        .await
        .expect("cmd");
        assert!(matches!(result, CommandResult::Handled(_)));
    }

    #[test]
    fn generate_otc_is_well_formed_and_unique() {
        let a = super::generate_otc();
        let b = super::generate_otc();
        // Format: BAST-XXXX-XXXX with the no-ambiguous charset.
        assert!(a.starts_with("BAST-"), "must be prefixed BAST-: {a}");
        assert_eq!(a.len(), 14, "BAST- + 4 + - + 4 = 14 chars: {a}");
        assert!(
            !a.contains('0') && !a.contains('O') && !a.contains('1') && !a.contains('I'),
            "must exclude ambiguous chars: {a}"
        );
        assert_ne!(a, b, "two codes must not collide");
    }

    #[tokio::test]
    async fn connect_app_inserts_live_otc_into_store() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;
        let registry = make_registry(&["Aria"]);
        let provider = make_provider();
        let mut forced = None;
        let store = crate::channel::webhook::new_otc_store();

        let result = handle_command(
            "/connect-app my-phone",
            &provider,
            &registry,
            &mem,
            &mut forced,
            Some(&store),
            "_local",
        )
        .await
        .expect("cmd");
        assert!(matches!(result, CommandResult::Handled(_)));

        // Exactly one code, mapped to the supplied device name, freshly issued.
        let guard = store.read().await;
        assert_eq!(guard.len(), 1, "one OTC must be inserted");
        let (code, (device, issued_at)) = guard.iter().next().unwrap();
        assert!(
            code.starts_with("BAST-"),
            "stored key is the BAST- code: {code}"
        );
        assert_eq!(
            device, "my-phone",
            "device name must be the /connect-app arg"
        );
        assert!(
            issued_at.elapsed().as_secs() < 5,
            "issued just now (well within 5-min TTL)"
        );
    }

    #[tokio::test]
    async fn connect_app_without_store_is_graceful() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;
        let registry = make_registry(&["Aria"]);
        let provider = make_provider();
        let mut forced = None;

        // No webhook channel running → otc_store is None → command still Handled, no panic.
        let result = handle_command(
            "/connect-app",
            &provider,
            &registry,
            &mem,
            &mut forced,
            None,
            "_local",
        )
        .await
        .expect("cmd");
        assert!(matches!(result, CommandResult::Handled(_)));
    }
}
