use std::time::Instant;
use tokio::sync::mpsc;
use crate::types::{Message, Role, MessageContent, ContentPart, CallConfig, BastionError, TokenUsage};
use crate::provider::{SharedProvider, call_with_retry};
use crate::session::SessionManager;
use crate::mcp::McpClient;
use crate::agent::compactor::AutoCompact;
use crate::agent::command::{handle_command, CommandResult};
use crate::memory::SharedMemory;
use crate::persona::PersonaRegistry;
use crate::goal::GoalEngine;
use crate::hooks::guardrails::InputGuardrail;
use crate::hooks::output_validator::OutputValidator;
use crate::hooks::egress::EgressHook;

const MAX_TOOL_ROUNDS: u32 = 10;
const DEFAULT_SYSTEM_PROMPT: &str = "You are Bastion, a proactive personal AI assistant.";
pub const DEFAULT_OWNER: &str = "_local";

pub struct AgentLoop {
    pub provider:          SharedProvider,
    pub session:           SessionManager,
    pub mcp:               McpClient,
    pub compactor:         AutoCompact,
    pub session_id:        String,
    pub daily_budget_usd:  f64,
    /// Registry of loaded personas.
    pub registry:          PersonaRegistry,
    /// Shared memory backend (beliefs + provenance).
    pub memory:            SharedMemory,
    /// Goal engine for drift nudges.
    pub goals:             GoalEngine,
    /// Input guardrail — screens malformed/oversized input (HOOK-02).
    pub input_guard:       InputGuardrail,
    /// Output-validator — NL contestation detection → belief revocation (HOOK-03).
    pub output_validator:  OutputValidator,
    /// Egress hook — fail-closed privacy egress check (PRIV-03, WR-04, T-04-02-04).
    /// Wired here so EgressHook is a live component in the AgentLoop; inline check_egress
    /// calls in run_provider_fallback and the cabinet path are the primary enforcement.
    pub egress_hook:       EgressHook,
    /// Pending queue for proactive messages.
    /// Phase 2: consumed by daemon_loop select arm (PROACT-05).
    pub pending_tx:        mpsc::Sender<String>,
    pub pending_rx:        Option<mpsc::Receiver<String>>,
    /// Forced persona for the next turn (set by /as command).
    pub forced_persona:    Option<String>,
}

impl AgentLoop {
    pub fn new(
        provider:         SharedProvider,
        session:          SessionManager,
        mcp:              McpClient,
        session_id:       String,
        daily_budget_usd: f64,
        registry:         PersonaRegistry,
        memory:           SharedMemory,
        goals:            GoalEngine,
    ) -> Self {
        let (pending_tx, pending_rx) = mpsc::channel(32);
        Self {
            provider,
            session,
            mcp,
            compactor: AutoCompact::new(),
            session_id,
            daily_budget_usd,
            registry,
            memory,
            goals,
            input_guard: InputGuardrail::default(),
            output_validator: OutputValidator,
            egress_hook: EgressHook,
            pending_tx,
            pending_rx: Some(pending_rx),
            forced_persona: None,
        }
    }

    /// Execute one full agent turn for the default local owner.
    pub async fn run_turn(&mut self, user_input: &str) -> anyhow::Result<String> {
        self.run_turn_for(user_input, DEFAULT_OWNER).await
    }

    /// Execute a turn for a specific owner (multi-owner / channel path).
    ///
    /// Flow: input_guard (HOOK-02) → router → runner/cabinet → output_validator (HOOK-03) → text
    pub async fn run_turn_for(&mut self, user_input: &str, owner: &str) -> anyhow::Result<String> {
        let t_start = Instant::now();

        // HOOK-02: input guardrail before routing (screens empty/oversized/spam input)
        self.input_guard.screen(user_input)?;

        // CR-04: resolve or create a session PER OWNER so two owners never share history.
        // WR-08: for DEFAULT_OWNER (CLI path) reuse self.session_id chosen at startup to
        // avoid load_most_recent_id_for resurrecting an older _local session.
        let session_id: String = if owner == DEFAULT_OWNER {
            self.session_id.clone()
        } else {
            match self.session.load_most_recent_id_for(owner).await? {
                Some(id) => id,
                None => self.session.create_session_for(owner).await?,
            }
        };

        // 1. Persist user message.
        // WR-13: user message is appended here, before the egress gate in step 5.
        // Risk: if egress blocks later, the user message is already stored in session history.
        // Acceptable for this phase: the user's own input is not the sensitive data — the egress
        // gate protects outbound LLM calls (sending local-only context to cloud providers), not
        // inbound user messages. A full transactional rollback requires a session.remove_last()
        // API that does not exist yet; deferred to Phase 4 (plan 08 session hardening).
        self.session.append(
            &session_id,
            Message { role: Role::User, content: MessageContent::Text(user_input.to_owned()) },
            None,
        ).await?;

        // 2. Load history and build token estimate
        let mut history = self.session.load_recent(&session_id).await?;

        // 3. Token ratio check and compaction BEFORE LLM call (D-08, AI-SPEC §4b.4).
        //    MEM-09: memory_flush runs before compaction.
        let used_tokens: u32 = AutoCompact::estimate_tokens(&history);
        let context_limit = self.provider.read().await.context_limit();
        if self.compactor.needs_compaction(used_tokens, context_limit) {
            // MEM-09: flush distilled beliefs to memory before compacting
            crate::agent::dream::memory_flush(&history, &self.memory, owner).await;

            let provider_ref = self.provider.read().await;
            history = self.compactor.compact(
                &session_id,
                &history,
                &**provider_ref,
                &self.session,
            ).await?;
            drop(provider_ref);
        }

        // 4. Router — classify the message into a RouterDecision.
        //    If /as forced a persona, override the router's choice.
        let mut decision = {
            let provider_ref = self.provider.read().await;
            crate::persona::router::route(&**provider_ref, &self.registry, user_input, owner).await?
        };

        if let Some(ref forced) = self.forced_persona.take() {
            decision.personas = vec![forced.clone()];
            decision.mode = crate::persona::router::ResponseMode::Single;
            decision.convene_reason = None;
        }

        // 5. Dispatch on decision.mode → build response text.
        //    Empty registry → route_text will be empty → fall back to provider.
        let route_text = match decision.mode {
            crate::persona::router::ResponseMode::Cabinet => {
                // Cabinet path: build_table → deliberate → synthesize (D-07 unified voice + dissent)
                let table = crate::cabinet::build_table(&self.registry, &decision, None)?;
                let transcript = crate::cabinet::orchestrator::deliberate(
                    &table,
                    self.provider.clone(),
                    crate::cabinet::orchestrator::DEFAULT_ROUNDS,
                ).await?;
                // CR-02: fail-closed egress on synthesis — the transcript may contain LocalOnly
                // content. Gate synthesis on the table tier before touching the cloud provider.
                let synth_provider_name = self.provider.read().await.name().to_owned();
                crate::hooks::egress::check_egress(Some(table.tier), &synth_provider_name)?;
                let provider_ref = self.provider.read().await;
                let verdict = crate::cabinet::synth::synthesize(&**provider_ref, &transcript).await?;
                drop(provider_ref);
                render_verdict(&verdict)
            }
            _ => {
                // Single / Parallel path via runner (egress is on the runner/cabinet path — plan 03/06)
                let output = crate::persona::runner::run(
                    decision,
                    &self.registry,
                    self.provider.clone(),
                    user_input,
                ).await?;
                render_runner_output(output)
            }
        };

        // 6. Graceful degradation: if registry is empty, fall back to plain tool-loop provider.
        let final_text = if route_text.is_empty() {
            self.run_provider_fallback(&mut history, &session_id).await?
        } else {
            // Persist the assistant response
            self.session.append(
                &session_id,
                Message { role: Role::Assistant, content: MessageContent::Text(route_text.clone()) },
                None,
            ).await?;
            route_text
        };

        // HOOK-03: output-validator — NL contestation detection → belief revocation (D-13).
        // Runs after the response is produced (before return).
        self.output_validator.validate(user_input, &self.memory, owner).await?;

        let latency_ms = t_start.elapsed().as_millis() as u64;
        tracing::info!(
            event = "turn_complete",
            latency_ms,
            session_id = %session_id,
            owner,
        );

        Ok(final_text)
    }

    /// Classic tool-loop provider call — used as fallback when registry is empty.
    /// `session_id` is the per-owner session resolved by the caller (run_turn_for).
    async fn run_provider_fallback(
        &mut self,
        history: &mut Vec<Message>,
        session_id: &str,
    ) -> anyhow::Result<String> {
        // Build tool definitions from ToolRegistry.
        let tools: Vec<serde_json::Value> = self.mcp.registry().list_tool_names()
            .iter()
            .map(|name| {
                let schema = self.mcp.registry().get_tool_schema(name)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
                serde_json::json!({
                    "name": name,
                    "description": format!("External tool: {}", name),
                    "input_schema": schema
                })
            })
            .collect();

        let config = CallConfig {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_owned(),
            max_tokens: 4096,
            tools,
        };

        // WR-04: resolve PrivacyTier for this turn before any provider call.
        // Strategy: use the active persona's tier from registry; default to None (fail-closed).
        // Tier is resolved from the PersonaRegistry (trusted), NOT from MCP tool results
        // (untrusted) — T-04-02-03 mitigation.
        let resolved_tier: Option<crate::memory::PrivacyTier> = {
            if let Some(ref persona_name) = self.forced_persona {
                self.registry.get(persona_name).map(|p| p.tier)
            } else {
                // No forced persona — None tier is always fail-closed per check_egress contract.
                None
            }
        };

        // WR-04: fail-closed egress gate — mirrors cabinet path (loop_.rs line 159-161, CR-02).
        // CRITICAL: Do NOT log system/user payload on block (egress.rs invariant).
        let provider_name_for_egress = self.provider.read().await.name().to_owned();
        tracing::debug!(
            event = "fallback_egress_check",
            tier = ?resolved_tier,
            provider = %provider_name_for_egress,
        );
        crate::hooks::egress::check_egress(resolved_tier, &provider_name_for_egress)?;

        // Agentic tool loop with hard round cap (Pitfall 4)
        let mut rounds = 0u32;
        let final_text = loop {
            if rounds >= MAX_TOOL_ROUNDS {
                tracing::error!(
                    event = "tool_loop_cap",
                    rounds = rounds,
                    session_id = %session_id
                );
                anyhow::bail!(BastionError::ToolLoopCap);
            }

            // Budget check BEFORE cloud call (PROV-06)
            let provider_name = self.provider.read().await.name().to_owned();
            if provider_name != "ollama" {
                if !self.session.check_budget(self.daily_budget_usd).await? {
                    anyhow::bail!(BastionError::BudgetExceeded);
                }
            }

            // LLM call — hold READ lock for full stream duration (Pitfall 5)
            let response = {
                let provider = self.provider.read().await;
                let prov_ref: &dyn crate::provider::Provider = &**provider;
                // SAFETY: call_with_retry closure borrows prov_ref for the duration of this block.
                // The READ lock is held for the entire duration of complete(), released after this block.
                call_with_retry(|| prov_ref.complete(history, &config), 3).await?
            };  // READ lock released here

            // Update budget with actual cost
            let cost_usd = estimate_cost_usd(provider_name.as_str(), &response.usage);
            if let Err(e) = self.session.update_budget(cost_usd).await {
                tracing::warn!(error = %e, "failed to update budget");
            }

            // Write assistant message to SQLite + history BEFORE dispatching tools (Pitfall 1).
            // History MUST carry tool_calls (ToolUse parts) — without them, tool-using models
            // never see that they already called the tool and loop until the round cap.
            let assistant_content = if let Some(ref tc) = response.tool_calls {
                MessageContent::Parts(
                    std::iter::once(ContentPart::Text { text: response.text.clone() })
                        .chain(tc.iter().map(|t| ContentPart::ToolUse {
                            id: t.id.clone(),
                            name: t.name.clone(),
                            input: t.arguments.clone(),
                        }))
                        .collect()
                )
            } else {
                MessageContent::Text(response.text.clone())
            };
            self.session.append(
                session_id,
                Message { role: Role::Assistant, content: assistant_content.clone() },
                Some(response.usage.output_tokens),
            ).await?;
            history.push(Message { role: Role::Assistant, content: assistant_content });

            // Tool dispatch
            match response.tool_calls {
                None => break response.text,  // final answer — no more tool calls
                Some(tool_calls) => {
                    for tc in &tool_calls {
                        tracing::debug!(event = "tool_dispatch", tool = %tc.name);
                        let result = self.mcp.call_tool_with_timeout(&tc.name, tc.arguments.clone()).await
                            .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }));

                        // D-06: handle skill_reloaded signal from skill-writer container.
                        // CR-02 fix: rebase skill_path to core's own SKILLS_DIR —
                        // skill-writer returns /skills/<name>/SKILL.md (its container path).
                        if result.get("skill_reloaded").and_then(|v| v.as_bool()) == Some(true) {
                            if let Some(raw_path) = result.get("skill_path").and_then(|v| v.as_str()) {
                                let skills_dir = std::env::var("SKILLS_DIR")
                                    .unwrap_or_else(|_| "/skills".to_string());
                                // SEC: skill_path crosses the skill-writer→core container trust
                                // boundary. Keep ONLY Normal components — discarding RootDir,
                                // Prefix, CurDir and ParentDir ("..") — so a malicious segment
                                // cannot escape SKILLS_DIR. Then take the last two
                                // (<skill_name>/SKILL.md) and resolve under SKILLS_DIR.
                                let normals: Vec<std::path::PathBuf> =
                                    std::path::Path::new(raw_path)
                                        .components()
                                        .filter_map(|c| match c {
                                            std::path::Component::Normal(s) => {
                                                Some(std::path::PathBuf::from(s))
                                            }
                                            _ => None,
                                        })
                                        .collect();
                                let skills_base = std::path::Path::new(&skills_dir);
                                // Strip the shared skills-base prefix and keep the FULL relative
                                // remainder (e.g. "personas/<slug>/<name>/SKILL.md" for private
                                // skills). Taking only the last two components would drop the
                                // personas/<slug>/ segment and rescan the wrong slot (WR-01).
                                let base_norm_count = skills_base
                                    .components()
                                    .filter(|c| matches!(c, std::path::Component::Normal(_)))
                                    .count();
                                let tail_components: Vec<std::path::PathBuf> =
                                    if normals.len() > base_norm_count {
                                        normals[base_norm_count..].to_vec()
                                    } else {
                                        normals.clone()
                                    };
                                // Require the reload target to be <name>/SKILL.md (at least two
                                // components, ending in SKILL.md) — guards the format coupling.
                                let last_is_skill_md = tail_components
                                    .last()
                                    .and_then(|p| p.to_str())
                                    == Some("SKILL.md");
                                if tail_components.len() < 2 || !last_is_skill_md {
                                    tracing::warn!(
                                        event = "skill_reload_rejected",
                                        raw_path = %raw_path,
                                        reason = "path does not resolve to <name>/SKILL.md under SKILLS_DIR"
                                    );
                                } else {
                                    let tail: std::path::PathBuf = tail_components.iter().collect();
                                    let local_path = skills_base.join(&tail);
                                    // Defense in depth: Normal-only components cannot escape
                                    // skills_base lexically, but a symlink planted inside
                                    // SKILLS_DIR could still redirect rescan outside it. Resolve
                                    // symlinks before the containment check (mirrors the Python
                                    // guard's resolve()). A not-yet-existing path can't be
                                    // canonicalized — fall back to the lexical check; rescan then
                                    // fails closed on the missing file.
                                    let canon_base = std::fs::canonicalize(skills_base)
                                        .unwrap_or_else(|_| skills_base.to_path_buf());
                                    let contained = match std::fs::canonicalize(&local_path) {
                                        Ok(canon) => canon.starts_with(&canon_base),
                                        Err(_) => local_path.starts_with(skills_base),
                                    };
                                    if !contained {
                                        tracing::warn!(
                                            event = "skill_reload_rejected",
                                            path = %local_path.to_string_lossy(),
                                            reason = "resolved path escapes SKILLS_DIR"
                                        );
                                    } else {
                                        let path_str = local_path.to_string_lossy();
                                        tracing::info!(event = "skill_reload_signal", path = %path_str);
                                        match crate::agent::skills::SkillsLoader::rescan(&path_str) {
                                            Ok(meta) => tracing::info!(
                                                event = "skill_loaded",
                                                name = %meta.name,
                                                path = %path_str
                                            ),
                                            Err(e) => tracing::warn!(
                                                event = "skill_reload_failed",
                                                path = %path_str,
                                                err = %e
                                            ),
                                        }
                                    }
                                }
                            }
                        }

                        let result_str = result.to_string();
                        let tool_msg = Message {
                            role: Role::Tool,
                            content: MessageContent::Parts(vec![ContentPart::ToolResult {
                                tool_use_id: tc.id.clone(),
                                content: result_str,
                            }]),
                        };
                        self.session.append(session_id, tool_msg.clone(), None).await?;
                        history.push(tool_msg);
                    }
                    rounds += 1;
                }
            }
        };

        Ok(final_text)
    }

    pub async fn handle_command(&mut self, input: &str) -> anyhow::Result<CommandResult> {
        handle_command(
            input,
            &self.provider,
            &self.registry,
            &self.memory,
            &mut self.forced_persona,
        ).await
    }

    /// Drain an `AgentHandle` receiver, serializing channel messages through `run_turn_for`.
    ///
    /// This is the single consumer that connects every channel (webhook, Telegram) to the
    /// AgentLoop. Call this from the daemon spawn task to wire channel turns with per-owner
    /// sessions and egress checks (CR-03/CR-04).
    ///
    /// Each request carries a trusted `owner` resolved by the channel layer.
    /// Replies are sent back through the oneshot in `AgentRequest`.
    /// Drain an `AgentHandle` receiver, serializing channel messages through `run_turn_for`.
    ///
    /// Errors are propagated as typed `Err` through the reply oneshot so the channel layer
    /// (e.g. `webhook::error_status`) can map them to the correct HTTP status (WR-10).
    /// Internal error detail is never echoed to the channel caller — only logged here.
    #[cfg(test)]
    pub async fn drain_handle(&mut self, mut rx: mpsc::Receiver<crate::agent::handle::AgentRequest>) {
        while let Some(req) = rx.recv().await {
            let result = self.run_turn_for(&req.text, &req.owner).await;
            if let Err(ref e) = result {
                tracing::warn!(
                    event = "handle_turn_error",
                    owner = %req.owner,
                    error = %e,
                    "channel turn failed"
                );
            }
            // Send Ok(text) or Err(e) — caller receives the typed error (WR-10).
            let _ = req.reply.send(result);
        }
    }
}

// ---------------------------------------------------------------------------
// Render helpers
// ---------------------------------------------------------------------------

fn render_runner_output(output: crate::persona::runner::RunnerOutput) -> String {
    use crate::persona::runner::RunnerOutput;
    match output {
        RunnerOutput::Single(_id, text) => text,
        RunnerOutput::Parallel(results) => {
            results.into_iter()
                .map(|(id, text)| format!("[{id}]: {text}"))
                .collect::<Vec<_>>()
                .join("\n\n")
        }
        RunnerOutput::ConveneCabinet(_) => String::new(), // handled in caller
    }
}

fn render_verdict(verdict: &crate::cabinet::synth::CabinetVerdict) -> String {
    let mut out = verdict.recommendation.clone();
    if !verdict.dissents.is_empty() {
        out.push_str("\n\n**Dissenting views:**");
        for d in &verdict.dissents {
            out.push_str(&format!("\n- {}: {}", d.persona, d.position));
        }
    }
    out
}

/// Simple cost estimation for budget tracking.
/// Per AI-SPEC §4b.5: claude-sonnet-4-5 ≈ $3/1M input, $15/1M output
fn estimate_cost_usd(provider: &str, usage: &TokenUsage) -> f64 {
    match provider {
        "anthropic" => {
            let input_cost  = usage.input_tokens  as f64 * 3.0  / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 15.0 / 1_000_000.0;
            input_cost + output_cost
        }
        "openai" => {
            let input_cost  = usage.input_tokens  as f64 * 2.5  / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 10.0 / 1_000_000.0;
            input_cost + output_cost
        }
        "ollama" => 0.0,  // local — no cost
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Tests (offline — MockProvider + temp-DB memory + single-persona registry)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::sqlite::SqliteMemory;
    use crate::persona::{Persona, PersonaRegistry};
    use crate::provider::{Provider, SharedProvider};
    use crate::types::{CallConfig, LlmResponse, Message};
    use crate::memory::PrivacyTier;
    use crate::goal::{GoalEngine, ScoringConfig};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;

    // MockProvider: complete_simple echoes a persona response;
    // complete_structured returns a valid Single RouterDecision.
    struct MockProvider {
        persona_name: String,
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
            Ok(LlmResponse {
                text: format!("response from {}", self.persona_name),
                tool_calls: None,
                usage: crate::types::TokenUsage { input_tokens: 10, output_tokens: 10, cache_read: 0, cache_write: 0 },
            })
        }
        async fn complete_simple(&self, _prompt: &str) -> anyhow::Result<String> {
            Ok(format!("simple:{}", self.persona_name))
        }
        async fn complete_structured(
            &self,
            _system: &str,
            _user: &str,
            _schema: serde_json::Value,
            _max_tokens: u32,
            _temperature: f32,
        ) -> anyhow::Result<String> {
            // Return a valid Single RouterDecision
            Ok(serde_json::json!({
                "personas": [self.persona_name],
                "owner": DEFAULT_OWNER,
                "mode": "single",
                "convene_reason": null
            }).to_string())
        }
        fn context_limit(&self) -> usize { 8192 }
        fn model_name(&self) -> &str { "mock" }
        fn name(&self) -> &'static str { "mock" }
    }

    fn make_provider(name: &str) -> SharedProvider {
        Arc::new(RwLock::new(
            Box::new(MockProvider { persona_name: name.to_string() }) as Box<dyn Provider>
        ))
    }

    fn make_registry(name: &str) -> PersonaRegistry {
        let mut personas = HashMap::new();
        personas.insert(name.to_string(), Persona {
            name: name.to_string(),
            description: Some("Test persona".to_string()),
            system_prompt: format!("You are {name}."),
            tier: PrivacyTier::CloudOk,
            weight: 0.8,
            skills: vec![],
        });
        PersonaRegistry::new_from_map(personas)
    }

    async fn make_loop(db_path: &str) -> AgentLoop {
        let session = crate::session::SessionManager::new(db_path);
        session.init_schema().await.expect("init_schema");
        let session_id = session.create_session().await.expect("create_session");

        let memory: SharedMemory = Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(db_path)) as Box<dyn crate::memory::Memory>
        ));

        // connect_all with non-existent path returns empty client (load_mcp_config returns {})
        let mcp = McpClient::connect_all("nonexistent_mcp.json").await
            .expect("connect_all empty");

        AgentLoop::new(
            make_provider("TestPersona"),
            session,
            mcp,
            session_id,
            10.0,
            make_registry("TestPersona"),
            memory,
            GoalEngine::new(db_path, ScoringConfig::default()),
        )
    }

    #[tokio::test]
    async fn run_turn_benign_message_returns_persona_response() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        let resp = agent.run_turn("hello world").await.expect("run_turn failed");
        assert!(!resp.is_empty(), "response must not be empty; got: {resp:?}");
    }

    #[tokio::test]
    async fn run_turn_contestation_phrase_revokes_belief() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        // Pre-store a belief
        {
            let mem = agent.memory.read().await;
            mem.store_belief(DEFAULT_OWNER, None, "Mario exercises every morning", "sess1", "user", false)
                .await
                .expect("store_belief");
        }

        // Verify belief is stored
        let before = {
            let mem = agent.memory.read().await;
            mem.retrieve_tagged(DEFAULT_OWNER, None).await.expect("retrieve")
        };
        assert_eq!(before.len(), 1, "belief must exist before contestation");

        // Run a turn with a contestation phrase that overlaps with the belief
        let _ = agent.run_turn("isso não é mais verdade sobre exercises morning").await;

        // After the turn, the output-validator should have revoked the belief
        let after = {
            let mem = agent.memory.read().await;
            mem.retrieve_tagged(DEFAULT_OWNER, None).await.expect("retrieve")
        };
        assert!(after.is_empty(), "belief must be revoked after contestation turn");
    }
}
