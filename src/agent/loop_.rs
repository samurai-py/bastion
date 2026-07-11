use crate::agent::command::{handle_command, CommandResult};
use crate::agent::compactor::AutoCompact;
use crate::agent::context::TurnContextProvider;
use crate::agent::identity::IdentityProvider;
use crate::goal::GoalEngine;
use crate::hooks::egress::EgressHook;
use crate::hooks::guardrails::InputGuardrail;
use crate::hooks::output_validator::OutputValidator;
use crate::mcp::McpClient;
use crate::memory::SharedMemory;
use crate::persona::PersonaRegistry;
use crate::provider::{call_with_retry, SharedProvider};
use crate::session::SessionManager;
use crate::types::{
    BastionError, CallConfig, ContentPart, Message, MessageContent, Role, TokenUsage,
};
use opentelemetry::trace::{Span as _, SpanKind, Tracer as _};
use opentelemetry::{global as otel_global, KeyValue};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

const MAX_TOOL_ROUNDS: u32 = 10;
const DEFAULT_SYSTEM_PROMPT: &str = "You are Bastion, a proactive personal AI assistant.";
pub const DEFAULT_OWNER: &str = "_local";

pub struct AgentLoop {
    pub provider: SharedProvider,
    pub session: SessionManager,
    pub mcp: Arc<McpClient>,
    pub compactor: AutoCompact,
    pub session_id: String,
    pub daily_budget_usd: f64,
    /// Registry of loaded personas.
    pub registry: PersonaRegistry,
    /// Shared memory backend (beliefs + provenance).
    pub memory: SharedMemory,
    /// Goal engine for drift nudges.
    pub goals: GoalEngine,
    /// Input guardrail — screens malformed/oversized input (HOOK-02).
    pub input_guard: InputGuardrail,
    /// Output-validator — NL contestation detection → belief revocation (HOOK-03).
    pub output_validator: OutputValidator,
    /// Egress hook — fail-closed privacy egress check (PRIV-03, WR-04, T-04-02-04).
    /// Wired here so EgressHook is a live component in the AgentLoop; inline check_egress
    /// calls in run_provider_fallback and the cabinet path are the primary enforcement.
    pub egress_hook: EgressHook,
    /// Unified capability registry (D-13) — single policy enforcement point.
    /// Starts empty; McpTool adapters are registered after McpClient connects.
    /// When non-empty, tool calls route through registry.invoke instead of run_provider_fallback.
    pub capability_registry: crate::capability::CapabilityRegistry,
    /// SEAM #2 — Provedores de contexto opaco para injeção no system prompt.
    /// Cada provider contribui com zero ou mais blocos por turn.
    /// O core inclui o conteúdo sem interpretar.
    pub context_providers: Vec<Box<dyn TurnContextProvider>>,
    /// Pending queue for proactive messages.
    /// Phase 2: consumed by daemon_loop select arm (PROACT-05).
    pub pending_tx: mpsc::Sender<String>,
    pub pending_rx: Option<mpsc::Receiver<String>>,
    /// Forced persona for the next turn (set by /as command).
    pub forced_persona: Option<String>,
    /// CR-02: shared OTC store handle, injected by main.rs when the webhook channel
    /// starts. `/connect-app` writes one-time codes here for the mobile pairing flow
    /// (`/auth/exchange`). `None` when the webhook channel is not running.
    pub otc_store: Option<crate::channel::webhook::OtcStore>,
    /// SEC-03: Composio OAuth client, injected by main.rs when `COMPOSIO_API_KEY` is
    /// configured. `/connect-app-composio` uses this to initiate real OAuth
    /// connections. `None` (the opt-in default) degrades the command to an
    /// "unavailable" message rather than panicking the daemon.
    pub composio_oauth: Option<std::sync::Arc<crate::mcp::oauth::ComposioOAuth>>,
    /// D-11 (Plan 08-01) / SO-03 (Plan 08-08): ordered list of model-name strings tried,
    /// in order, when the primary provider suffers a hard/persistent failure
    /// (`complete_with_fallback_ladder`'s rung 3). Sourced from `AgentConfig.fallback_models`
    /// via main.rs. Empty = zero behavior change (today's exact fail-on-exhaustion behavior).
    pub fallback_models: Vec<String>,
    /// Test-only seam (mirrors `#[cfg(test)] pub async fn drain_handle` below): lets unit
    /// tests inject a scripted `Provider` for the fallback ladder's provider-switch rung
    /// instead of a real, network/credential-backed one from `registry::resolve_provider`.
    /// Always `None` outside test builds — production always resolves through the real
    /// registry; this field does not exist in non-test compilations.
    #[cfg(test)]
    fallback_resolver_override: Option<FallbackResolverOverride>,
}

/// Test-only alias for the scripted-provider-resolution closure type (keeps the
/// `fallback_resolver_override` field declaration under clippy's type-complexity limit).
#[cfg(test)]
type FallbackResolverOverride =
    Box<dyn Fn(&str) -> anyhow::Result<Box<dyn crate::provider::Provider>> + Send + Sync>;

impl AgentLoop {
    // Wires 8 independent subsystems (provider, session, mcp, registry, memory, goals…).
    // A params struct would just be a one-call-site bag — no shared shape to extract.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: SharedProvider,
        session: SessionManager,
        mcp: McpClient,
        session_id: String,
        daily_budget_usd: f64,
        registry: PersonaRegistry,
        memory: SharedMemory,
        goals: GoalEngine,
        fallback_models: Vec<String>,
        db_path: &str,
    ) -> Self {
        let (pending_tx, pending_rx) = mpsc::channel(32);
        // BIG-1 (Gap 2): McpClient is shared by-Arc so each McpToolAdapter can hold a
        // reference and route tool calls through capability_registry.invoke.
        let mcp = Arc::new(mcp);
        let mut agent = Self {
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
            // SEC-01: the real ApprovalQueue is wired against the SAME db_path as
            // session/memory — a needs_approval()==true capability is never
            // unusable-but-should-work; it always has a queue behind it.
            capability_registry: crate::capability::CapabilityRegistry::new().with_approval_queue(
                Arc::new(crate::capability::approval::ApprovalQueue::new(db_path)),
            ),
            context_providers: vec![],
            pending_tx,
            pending_rx: Some(pending_rx),
            forced_persona: None,
            otc_store: None,
            composio_oauth: None,
            fallback_models,
            #[cfg(test)]
            fallback_resolver_override: None,
        };
        // M1: registrar IdentityProvider para injeção do bloco de identidade via SEAM #2.
        // No primeiro uso retorna o ONBOARDING_PROMPT; nos subsequentes retorna o bloco gravado.
        agent
            .context_providers
            .push(Box::new(IdentityProvider::new(agent.memory.clone())));

        // SEAM #2 — MemoryRagProvider: recall de beliefs por injeção (perna "RAG" do
        // BIG-1, decisão de híbrido ainda pendente → opt-in). Funciona com qualquer
        // provider — incluindo terminal-agents (PROV-09) que nunca emitem tool_calls —
        // e é egress-safe: blocos separados por tier, build_system_prompt derruba
        // por bloco. Default-off porque providers com function-calling já recebem as
        // tools de memória (injetar também duplicaria exposição e cresce o prompt).
        let memory_rag_on = std::env::var("BASTION_MEMORY_RAG")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        if memory_rag_on {
            agent.context_providers.push(Box::new(
                crate::agent::memory_rag::MemoryRagProvider::new(agent.memory.clone()),
            ));
            tracing::info!(event = "memory_rag_enabled");
        }

        // LEARN-03 — ProceduralBeliefProvider: recall de beliefs PROCEDURAIS (kind=
        // 'procedural') por injeção de contexto, mesma mecânica de MemoryRagProvider
        // (tier-split, egress-safe por bloco). Always-on (não gated por env, ao
        // contrário do BASTION_MEMORY_RAG acima): procedural é entregável de primeira
        // classe da Fase 7, não uma perna experimental do RAG híbrido do BIG-1.
        agent.context_providers.push(Box::new(
            crate::agent::procedural::ProceduralBeliefProvider::new(agent.memory.clone()),
        ));
        tracing::info!(event = "procedural_belief_provider_enabled");

        // BIG-1 (Gap 2): populate the capability_registry from every connected MCP tool.
        // Without this the registry stays empty, list_tool_defs() returns [] (so the normal
        // persona path offers ZERO tools to the LLM), and the is_empty() fast-path in
        // dispatch_tool_loop bypasses the egress/approval gate. Registering one McpToolAdapter
        // per tool makes ALL tool calls flow through capability_registry.invoke (D-13).
        // Snapshot tool metadata first (owned) so the agent.mcp borrow is released before we
        // mutably borrow agent.capability_registry.
        let mcp_tools: Vec<(String, String, serde_json::Value, String, bool, bool, bool)> = agent
            .mcp
            .registry()
            .list_tool_names()
            .iter()
            .map(|name| {
                let server_label = agent
                    .mcp
                    .registry()
                    .server_for(name)
                    .unwrap_or("")
                    .to_string();
                let schema = agent
                    .mcp
                    .registry()
                    .get_tool_schema(name)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
                let description = agent
                    .mcp
                    .registry()
                    .get_tool_description(name)
                    .unwrap_or("")
                    .to_string();
                // Plan 10-08: the load-bearing lookup — any tool whose owning MCP
                // server is config-flagged `is_local = true` (e.g. the voice sidecar,
                // Plan 10-03/10-09's `[mcp.servers.voice]`) is automatically registered
                // as a local capability below, with zero tool-name string matching.
                let is_local = agent.mcp.registry().is_local(name);
                // Plan 11-04: the load-bearing lookup — a tool whose own MCP server
                // self-reported `ToolAnnotations.destructive_hint` (or omitted it,
                // fail-cautious) is automatically registered as needing owner
                // approval below, with zero tool-name string matching. `trusted`
                // mirrors `is_local`'s threading, sourced from the owning server's
                // `McpServerEntry.trusted` config.
                let needs_approval = agent.mcp.registry().needs_approval(name);
                let trusted = agent.mcp.registry().is_trusted(name);
                (
                    name.to_string(),
                    server_label,
                    schema,
                    description,
                    is_local,
                    needs_approval,
                    trusted,
                )
            })
            .collect();
        for (tool_name, server_label, schema, description, is_local, needs_approval, trusted) in
            mcp_tools
        {
            let adapter = crate::capability::McpToolAdapter {
                tool_name: tool_name.clone(),
                server_label,
                description,
                schema,
                mcp: agent.mcp.clone(),
                is_local_override: is_local,
                needs_approval_override: needs_approval,
                trusted_override: trusted,
            };
            if let Err(e) = agent.capability_registry.register(Arc::new(adapter)) {
                tracing::warn!(event = "mcp_capability_register_failed", tool = %tool_name, err = %e);
            }
        }
        let registered = agent.capability_registry.list_tool_defs().len();
        tracing::info!(
            event = "capability_registry_populated",
            mcp_tools = registered
        );

        agent
    }

    /// Register MeshSliceProvider (SEAM #2 for mesh slices from remote owners).
    ///
    /// Called after AgentLoop::new() when mesh is configured (MESH_IDENTITY_KEY set).
    /// The slice_store is shared with the ingest_handler via AppState so that received
    /// slices become visible in the system prompt on the very next agent turn.
    ///
    /// WR-06: uses the real owner_id (BASTION_OWNER_ID env var) rather than session_id
    /// as the local_owner passed to MeshSliceProvider. session_id is a per-session UUID
    /// that changes across restarts — it is NOT a stable owner identifier.
    pub fn add_mesh_slice_provider(
        &mut self,
        store: crate::mesh::context_provider::MeshSliceStore,
    ) {
        // WR-06: read real owner_id from env; fall back to DEFAULT_OWNER (not session_id).
        // BASTION_OWNER_ID is the stable identity used by P2PTransport and the mesh config.
        let local_owner = std::env::var("BASTION_OWNER_ID")
            .or_else(|_| std::env::var("MESH_OWNER_ID"))
            .unwrap_or_else(|_| DEFAULT_OWNER.to_string());
        let mesh_provider =
            crate::mesh::context_provider::MeshSliceProvider::from_store(local_owner, store);
        self.context_providers.push(Box::new(mesh_provider));
        tracing::info!(
            event = "mesh_slice_provider_registered",
            "MeshSliceProvider registered in context_providers (SEAM #2)"
        );
    }

    /// SEAM #2 — Constrói o system prompt para o turn atual.
    ///
    /// Começa com DEFAULT_SYSTEM_PROMPT como base.
    /// Itera context_providers e concatena blocos cujo max_tier seja compatível
    /// com o provider ativo (egress check por bloco).
    ///
    /// SECURITY (Pitfall 5): usa o max_tier do BLOCO, não o tier da persona —
    /// impede que beliefs LocalOnly vazem para providers cloud quando a persona é CloudOk.
    ///
    /// D-12/D-14b — STABLE vs VOLATILE prefix split (byte-stable prompt caching):
    /// `context_providers` is intentionally ordered so the FIRST `k` entries are
    /// turn-invariant and the remainder are turn-scoped:
    ///   - index 0: `DEFAULT_SYSTEM_PROMPT` (compile-time constant).
    ///   - index 1: `IdentityProvider`'s block — ignores `turn_msg`/`persona`, reads only
    ///     `owner`'s core memory (onboarding prompt or the stored identity belief), so it
    ///     is byte-identical across turns for the same owner as long as identity isn't
    ///     rewritten mid-session.
    ///   - index 2+ (when `BASTION_MEMORY_RAG=1`), and the always-on
    ///     `ProceduralBeliefProvider` / post-construction `MeshSliceProvider`: turn-scoped
    ///     recall/active_object blocks that legitimately vary per turn — these come AFTER
    ///     the stable prefix, never before it.
    ///
    /// This ordering is what lets a caching-aware provider (e.g. Anthropic
    /// `cache_control`) cache the stable prefix once and reuse it across turns.
    /// `build_system_prompt_parts` (below) is the pub seam `tests/prompt_cache_prefix.rs`
    /// uses to assert `parts[0..2]` stays byte-identical across turns with different
    /// volatile content (D-14b regression guard) — do NOT reorder `context_providers` in
    /// `AgentLoop::new`/`add_mesh_slice_provider` without updating that test's `k`.
    async fn build_system_prompt(
        &self,
        owner: &str,
        turn_msg: &str,
        persona: Option<&str>,
    ) -> String {
        self.build_system_prompt_parts(owner, turn_msg, persona)
            .await
            .join("\n\n")
    }

    /// Test seam for D-14b: identical logic to `build_system_prompt`, but returns the
    /// pre-join `Vec<String>` parts instead of the final joined `String`.
    ///
    /// This is deliberately `pub` and NOT `#[cfg(test)]`-gated: integration test binaries
    /// under `tests/` are compiled against the crate's normal (non-`cfg(test)`) build, so
    /// `#[cfg(test)]` items are invisible to them (same limitation already documented for
    /// `fallback_resolver_override` in Plan 08-08's STATE.md entry). Exposing this ordered
    /// view lets `tests/prompt_cache_prefix.rs` assert the STABLE prefix (`parts[0..k]`,
    /// see `build_system_prompt`'s rustdoc) is byte-identical across turns without
    /// duplicating the egress-check logic below — DO NOT let the two functions diverge.
    pub async fn build_system_prompt_parts(
        &self,
        owner: &str,
        turn_msg: &str,
        persona: Option<&str>,
    ) -> Vec<String> {
        let provider_name = self.provider.read().await.name().to_owned();
        let mut parts: Vec<String> = vec![DEFAULT_SYSTEM_PROMPT.to_owned()];

        for provider in &self.context_providers {
            let blocks = provider.context_for_turn(owner, turn_msg, persona).await;
            for block in blocks {
                // SECURITY: verificar egress pelo tier do BLOCO, não da persona.
                // check_egress(Some(LocalOnly), "openrouter") → Err → não injeta.
                // check_egress(Some(CloudOk), "openrouter") → Ok → injeta.
                if crate::hooks::egress::check_egress(Some(block.max_tier), &provider_name).is_ok()
                {
                    parts.push(block.content);
                } else {
                    tracing::debug!(
                        event = "context_block_skipped_egress",
                        provider = %provider_name,
                        tier = ?block.max_tier,
                    );
                }
            }
        }

        parts
    }

    /// Execute one full agent turn for the default local owner.
    pub async fn run_turn(&mut self, user_input: &str) -> anyhow::Result<String> {
        self.run_turn_for(user_input, DEFAULT_OWNER).await
    }

    /// Execute a turn for a specific owner (multi-owner / channel path).
    ///
    /// Flow: input_guard (HOOK-02) → router → runner/cabinet → output_validator (HOOK-03) → text
    /// Cockpit commands (used by the mobile cockpit via /webhook): return real
    /// data from memory + the goal engine. Returns `None` for normal turns.
    async fn cockpit_command(&self, input: &str, owner: &str) -> Option<anyhow::Result<String>> {
        let t = input.trim();
        if t == "/memories" {
            let mem = self.memory.read().await;
            return Some(mem.retrieve_tagged(owner, None).await.map(|bs| {
                if bs.is_empty() {
                    "Nenhuma memória registrada.".to_string()
                } else {
                    bs.iter()
                        .map(|b| format!("{}: {}", b.id, b.content))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }));
        }
        if let Some(id_str) = t.strip_prefix("/contest ") {
            let id: i64 = match id_str.trim().parse() {
                Ok(v) => v,
                Err(_) => return Some(Ok("Uso: /contest <id>".to_string())),
            };
            let mem = self.memory.read().await;
            return Some(
                mem.revoke_belief(owner, id)
                    .await
                    .map(|_| format!("Memória {} contestada e revogada.", id)),
            );
        }
        if t == "/goals" {
            return Some(self.goals.list_goals(owner).await.map(|gs| {
                if gs.is_empty() {
                    "Nenhuma meta ativa.".to_string()
                } else {
                    let lines: Vec<String> = gs
                        .iter()
                        .map(|g| match &g.metric {
                            Some(m) => format!("- {} ({})", g.description, m),
                            None => format!("- {}", g.description),
                        })
                        .collect();
                    format!("{} metas ativas\n{}", gs.len(), lines.join("\n"))
                }
            }));
        }
        if t == "/drift" {
            return Some(self.goals.list_goals(owner).await.map(|gs| {
                if gs.is_empty() {
                    return "Nenhuma meta ativa — sem drift a monitorar.".to_string();
                }
                let n = gs.len();
                let healthy = gs.iter().filter(|g| g.last_confirmed.is_some()).count();
                let pct = healthy * 100 / n;
                let status = if pct >= 60 {
                    "estável"
                } else if pct >= 30 {
                    "atenção"
                } else {
                    "em risco"
                };
                format!(
                    "drift {} ({}%) — {}/{} metas com progresso confirmado.",
                    status, pct, healthy, n
                )
            }));
        }
        None
    }

    pub async fn run_turn_for(&mut self, user_input: &str, owner: &str) -> anyhow::Result<String> {
        let t_start = Instant::now();

        // HOOK-02: input guardrail before routing (screens empty/oversized/spam input)
        self.input_guard.screen(user_input)?;

        // Cockpit commands resolve to real memory/goal data, bypassing the LLM turn.
        if let Some(result) = self.cockpit_command(user_input, owner).await {
            return result;
        }

        // SEAM #4: span raiz invoke_agent por turn.
        // DESIGN: nome genérico "invoke_agent" — span names são imutáveis após start().
        // gen_ai.agent.name é setado via set_attribute APÓS o routing (quando persona é conhecida).
        let tracer = otel_global::tracer("bastion");
        let mut turn_span = tracer
            .span_builder("invoke_agent")
            .with_kind(SpanKind::Internal)
            .with_attributes(vec![
                KeyValue::new("gen_ai.operation.name", "invoke_agent"),
                KeyValue::new("gen_ai.conversation.id", self.session_id.clone()),
            ])
            .start(&tracer);

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
        self.session
            .append(
                &session_id,
                Message {
                    role: Role::User,
                    content: MessageContent::Text(user_input.to_owned()),
                },
                None,
            )
            .await?;

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
            history = self
                .compactor
                .compact(&session_id, &history, &**provider_ref, &self.session)
                .await?;
            drop(provider_ref);
        }

        // 4. Router — classify the message into a RouterDecision.
        //    If /as forced a persona, override the router's choice.
        let mut decision = {
            let provider_ref = self.provider.read().await;
            crate::persona::router::route(
                &**provider_ref,
                &self.registry,
                user_input,
                owner,
                &mut self.capability_registry,
            )
            .await?
        };

        if let Some(ref forced) = self.forced_persona.take() {
            decision.personas = vec![forced.clone()];
            decision.mode = crate::persona::router::ResponseMode::Single;
            decision.convene_reason = None;
        }

        // SEAM #4: registrar persona no span raiz via atributo (span name é imutável).
        // Após routing — persona é conhecida agora.
        let agent_name = decision
            .personas
            .first()
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        turn_span.set_attribute(KeyValue::new("gen_ai.agent.name", agent_name));

        // WR-01 (review #2): capture the turn's privacy tier from the handling persona
        // ONCE, before `decision` is moved into the dispatch match below. Threaded into
        // run_provider_fallback so the fallback path no longer re-reads the already-taken
        // `self.forced_persona` (collapsed to None — over-blocked a forced CloudOk persona
        // and relied on accidental fail-closed for LocalOnly). None stays fail-closed.
        let turn_tier: Option<crate::memory::PrivacyTier> = decision
            .personas
            .first()
            .and_then(|name| self.registry.get(name).map(|p| p.tier));

        // SEAM #2: the active persona name scopes belief recall (persona-tagged + global).
        // Resolved ONCE here (like turn_tier) and threaded into build_system_prompt on BOTH
        // the single/parallel path and the fallback path, so recall never crosses persona
        // boundaries. `None` (no persona matched) keeps global-only recall — the fail-safe.
        let turn_persona: Option<String> = decision.personas.first().cloned();

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
                    &self.capability_registry,
                )
                .await?;
                // CR-02: fail-closed egress on synthesis — the transcript may contain LocalOnly
                // content. Gate synthesis on the table tier before touching the cloud provider.
                let synth_provider_name = self.provider.read().await.name().to_owned();
                crate::hooks::egress::check_egress(Some(table.tier), &synth_provider_name)?;
                let provider_ref = self.provider.read().await;
                let verdict = crate::cabinet::synth::synthesize(
                    &**provider_ref,
                    &transcript,
                    &mut self.capability_registry,
                )
                .await?;
                drop(provider_ref);
                render_verdict(&verdict)
            }
            _ => {
                // Single / Parallel path via runner.
                // Build CallConfig with tools from capability_registry (BIG-1).
                // SEAM #2: system_prompt built dynamically — context_providers inject opaque blocks.
                let system_prompt = self
                    .build_system_prompt(owner, user_input, turn_persona.as_deref())
                    .await;
                let tools = self.capability_registry.list_tool_defs();
                let config = CallConfig {
                    system_prompt, // ← dinâmico via SEAM #2
                    max_tokens: 4096,
                    tools,
                    ..Default::default()
                };

                let output = crate::persona::runner::run(
                    decision,
                    &self.registry,
                    self.provider.clone(),
                    &history,
                    &config,
                )
                .await?;

                // Process tool_calls if present via dispatch_tool_loop (BIG-1).
                match output {
                    crate::persona::runner::RunnerOutput::Single(pid, response) => {
                        // WR-04 / CR-01: resolve PrivacyTier from the persona actually
                        // handling this turn (router-chosen or /as-forced). Re-reading
                        // self.forced_persona here was a privacy bug: it was already
                        // consumed by .take() above (line ~211), so a forced LocalOnly
                        // persona resolved to None and got stamped CloudOk in
                        // dispatch_tool_loop — a LocalOnly→cloud downgrade.
                        let resolved_tier: Option<crate::memory::PrivacyTier> =
                            self.registry.get(&pid).map(|p| p.tier);
                        let text = self
                            .dispatch_tool_loop(
                                &mut history,
                                &session_id,
                                &config,
                                response,
                                owner,
                                resolved_tier,
                            )
                            .await?;
                        // Persist the assistant response (dispatch_tool_loop handles intermediate turns)
                        self.session
                            .append(
                                &session_id,
                                Message {
                                    role: Role::Assistant,
                                    content: crate::types::MessageContent::Text(text.clone()),
                                },
                                None,
                            )
                            .await?;
                        text
                    }
                    crate::persona::runner::RunnerOutput::Parallel(results) => {
                        // Parallel: run tool-loop for each persona result and collect texts.
                        let mut texts: Vec<String> = Vec::new();
                        for (pid, response) in results {
                            // CR-01: resolve tier per-persona — each parallel persona may
                            // carry a different tier. fail-closed via check_egress inside
                            // dispatch_tool_loop (None → blocked, not defaulted to cloud).
                            let resolved_tier: Option<crate::memory::PrivacyTier> =
                                self.registry.get(&pid).map(|p| p.tier);
                            let text = self
                                .dispatch_tool_loop(
                                    &mut history,
                                    &session_id,
                                    &config,
                                    response,
                                    owner,
                                    resolved_tier,
                                )
                                .await?;
                            texts.push(text);
                        }
                        let combined = texts.join("\n\n");
                        self.session
                            .append(
                                &session_id,
                                Message {
                                    role: Role::Assistant,
                                    content: crate::types::MessageContent::Text(combined.clone()),
                                },
                                None,
                            )
                            .await?;
                        combined
                    }
                    crate::persona::runner::RunnerOutput::ConveneCabinet(_) => String::new(),
                }
            }
        };

        // 6. Graceful degradation: if route_text is empty (no persona matched, or Cabinet
        //    produced no output), fall back to plain tool-loop provider.
        //    The Single/Parallel path now persists assistant response inline in step 5.
        //    The Cabinet path also produces its own text.
        //    Only the truly empty case (no persona matched) reaches run_provider_fallback.
        let final_text = if route_text.is_empty() {
            match self
                .run_provider_fallback(
                    &mut history,
                    &session_id,
                    owner,
                    user_input,
                    turn_tier,
                    turn_persona.as_deref(),
                )
                .await
            {
                Ok(text) => text,
                Err(e) => {
                    // EVAL-01: grow the regression set from a concrete production
                    // failure signal (egress rejection) — tier-gated, structural-only.
                    if matches!(
                        e.downcast_ref::<BastionError>(),
                        Some(BastionError::PrivacyEgressBlocked)
                    ) {
                        crate::eval::capture::record_failure(
                            crate::eval::capture::FailureKind::EgressReject,
                            turn_tier,
                            "localonly_belief_blocked_from_cloud_provider",
                        );
                    }
                    return Err(e);
                }
            }
        } else {
            route_text
        };

        // HOOK-03: output-validator — NL contestation detection → belief revocation (D-13).
        // Runs after the response is produced (before return).
        self.output_validator
            .validate(user_input, &self.memory, owner)
            .await?;

        let latency_ms = t_start.elapsed().as_millis() as u64;
        tracing::info!(
            event = "turn_complete",
            latency_ms,
            session_id = %session_id,
            owner,
        );

        // SEAM #4: fechar span raiz do turn
        turn_span.end();

        Ok(final_text)
    }

    /// D-06: handle the `skill_reloaded` signal emitted by the skill-writer
    /// container after a skill is created/updated by natural language.
    ///
    /// Gap 1 fix: this was previously inline in `run_provider_fallback` only,
    /// which is unreachable on normal persona turns — so skill-writer-by-NL never
    /// reloaded in normal conversation. Extracted into a shared helper called by
    /// BOTH `run_provider_fallback` and `dispatch_tool_loop`, so the skill becomes
    /// available on the very next turn regardless of which path produced it.
    ///
    /// Synchronous (no awaits): `SkillsLoader::rescan` and the path checks are sync.
    fn handle_skill_reload(&self, result: &serde_json::Value) {
        // CR-02 path-safety: rebase skill_path to core's own SKILLS_DIR —
        // skill-writer returns /skills/<name>/SKILL.md (its container path).
        if result.get("skill_reloaded").and_then(|v| v.as_bool()) == Some(true) {
            if let Some(raw_path) = result.get("skill_path").and_then(|v| v.as_str()) {
                let skills_dir =
                    std::env::var("SKILLS_DIR").unwrap_or_else(|_| "/skills".to_string());
                // SEC: skill_path crosses the skill-writer→core container trust
                // boundary. Keep ONLY Normal components — discarding RootDir,
                // Prefix, CurDir and ParentDir ("..") — so a malicious segment
                // cannot escape SKILLS_DIR.
                let normals: Vec<std::path::PathBuf> = std::path::Path::new(raw_path)
                    .components()
                    .filter_map(|c| match c {
                        std::path::Component::Normal(s) => Some(std::path::PathBuf::from(s)),
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
                let tail_components: Vec<std::path::PathBuf> = if normals.len() > base_norm_count {
                    normals[base_norm_count..].to_vec()
                } else {
                    normals.clone()
                };
                // Require the reload target to be <name>/SKILL.md (at least two
                // components, ending in SKILL.md) — guards the format coupling.
                let last_is_skill_md =
                    tail_components.last().and_then(|p| p.to_str()) == Some("SKILL.md");
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
                    // symlinks before the containment check. A not-yet-existing
                    // path can't be canonicalized — fall back to the lexical
                    // check; rescan then fails closed on the missing file.
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
    }

    /// Dispatch tool-loop for a single LLM response (BIG-1).
    ///
    /// Processes `response.tool_calls` by routing each call through `capability_registry.invoke`
    /// (D-13 single policy enforcement point). Loops until no more tool_calls or MAX_TOOL_ROUNDS.
    ///
    /// Returns the final text answer from the LLM (after all tool rounds complete).
    ///
    /// # Arguments
    /// - `history`: mutable session history — updated with assistant+tool messages
    /// - `session_id`: for persistence
    /// - `config`: CallConfig with tools (reused for subsequent complete() calls)
    /// - `response`: initial LlmResponse from the runner
    /// - `owner`: resolved owner for InvokeCtx
    /// - `resolved_tier`: privacy tier for egress gate in InvokeCtx
    async fn dispatch_tool_loop(
        &mut self,
        history: &mut Vec<Message>,
        session_id: &str,
        config: &CallConfig,
        initial_response: crate::types::LlmResponse,
        owner: &str,
        resolved_tier: Option<crate::memory::PrivacyTier>,
    ) -> anyhow::Result<String> {
        // SEAM #4: tracer handle for child spans (chat, execute_tool)
        let tracer = otel_global::tracer("bastion");
        let mut response = initial_response;
        let mut rounds = 0u32;

        loop {
            // Write assistant message to history BEFORE dispatching tools (Pitfall 1).
            let assistant_content = if let Some(ref tc) = response.tool_calls {
                MessageContent::Parts(
                    std::iter::once(ContentPart::Text {
                        text: response.text.clone(),
                    })
                    .chain(tc.iter().map(|t| ContentPart::ToolUse {
                        id: t.id.clone(),
                        name: t.name.clone(),
                        input: t.arguments.clone(),
                        extra: t.extra.clone(),
                    }))
                    .collect(),
                )
            } else {
                MessageContent::Text(response.text.clone())
            };
            self.session
                .append(
                    session_id,
                    Message {
                        role: Role::Assistant,
                        content: assistant_content.clone(),
                    },
                    Some(response.usage.output_tokens),
                )
                .await?;
            history.push(Message {
                role: Role::Assistant,
                content: assistant_content,
            });

            match response.tool_calls {
                None => break Ok(response.text),
                Some(tool_calls) => {
                    if rounds >= MAX_TOOL_ROUNDS {
                        tracing::error!(
                            event = "tool_loop_cap",
                            rounds = rounds,
                            session_id = %session_id
                        );
                        anyhow::bail!(BastionError::ToolLoopCap);
                    }

                    for tc in &tool_calls {
                        tracing::debug!(event = "tool_dispatch", tool = %tc.name);
                        // D-13: route ALL tool calls through capability_registry.invoke.
                        // SEC-01: the approval gate is real now — whether this call queues
                        // is decided entirely by the capability's own needs_approval().
                        let ctx = crate::capability::InvokeCtx {
                            owner: owner.to_owned(),
                            // CR-01/CR-02: fail-closed — an unresolved tier is treated as the
                            // MOST restrictive (LocalOnly), never the most permissive. A None
                            // here previously defaulted to CloudOk, opening an egress path.
                            privacy_tier: Some(
                                resolved_tier.unwrap_or(crate::memory::PrivacyTier::LocalOnly),
                            ),
                        };
                        // SEAM #4: span filho execute_tool por tool call
                        let mut tool_span = tracer
                            .span_builder(format!("execute_tool {}", tc.name))
                            .with_kind(SpanKind::Internal)
                            .with_attributes(vec![
                                KeyValue::new("gen_ai.operation.name", "execute_tool"),
                                KeyValue::new("gen_ai.tool.name", tc.name.clone()),
                                KeyValue::new("gen_ai.tool.call.id", tc.id.clone()),
                            ])
                            .start(&tracer);
                        let result = if self.capability_registry.is_empty() {
                            // Fallback: if no capabilities registered, try MCP directly.
                            // WR-02 (review #2): even this registry-bypass path must honor egress
                            // (D-13). Mirror the policy registry.invoke applies to a non-local MCP
                            // capability — gate the turn tier against "external" before dispatch,
                            // so a hallucinated/injected tool call can't execute ungated.
                            match crate::hooks::egress::check_egress(resolved_tier, "external") {
                                Err(e) => {
                                    tool_span
                                        .set_attribute(KeyValue::new("error.type", e.to_string()));
                                    serde_json::json!({"error": e.to_string()})
                                }
                                Ok(()) => self
                                    .mcp
                                    .call_tool_with_timeout(&tc.name, tc.arguments.clone())
                                    .await
                                    .unwrap_or_else(|e| {
                                        // SEAM #4: record error type (CRITICAL: no content/payload — T-05-05-02)
                                        tool_span.set_attribute(KeyValue::new(
                                            "error.type",
                                            e.to_string(),
                                        ));
                                        serde_json::json!({"error": e.to_string()})
                                    }),
                            }
                        } else {
                            self.capability_registry
                                .invoke(&tc.name, tc.arguments.clone(), &ctx)
                                .await
                                .unwrap_or_else(|e| {
                                    // SEAM #4: record error type (CRITICAL: no content/payload — T-05-05-02)
                                    tool_span
                                        .set_attribute(KeyValue::new("error.type", e.to_string()));
                                    serde_json::json!({"error": e.to_string()})
                                })
                        };
                        tool_span.end();

                        // Gap 1 (SC#2): skill-writer-by-NL must reload on the normal
                        // persona path too, not only in run_provider_fallback. Shared
                        // helper handles the skill_reloaded signal.
                        self.handle_skill_reload(&result);

                        let result_str = result.to_string();
                        let tool_msg = Message {
                            role: Role::Tool,
                            content: MessageContent::Parts(vec![ContentPart::ToolResult {
                                tool_use_id: tc.id.clone(),
                                content: result_str,
                            }]),
                        };
                        self.session
                            .append(session_id, tool_msg.clone(), None)
                            .await?;
                        history.push(tool_msg);
                    }
                    rounds += 1;

                    // Budget check BEFORE next cloud call (PROV-06)
                    let provider_name = self.provider.read().await.name().to_owned();
                    if provider_name != "ollama"
                        && !self.session.check_budget(self.daily_budget_usd).await?
                    {
                        anyhow::bail!(BastionError::BudgetExceeded);
                    }

                    // CR-02: fail-closed egress gate before the next cloud round. `history`
                    // now carries tool results (and prior turns) that may include LocalOnly
                    // content; block before any of it reaches a non-local provider. Mirrors
                    // the Cabinet synthesis gate. check_egress fails closed on None/LocalOnly.
                    crate::hooks::egress::check_egress(resolved_tier, &provider_name)?;

                    // Next LLM call in the loop
                    // SEAM #4: span filho chat {model} por provider call
                    let (model_name, provider_system) = {
                        let p = self.provider.read().await;
                        (p.model_name().to_owned(), p.name().to_owned())
                    };
                    let chat_span_name = format!("chat {}", model_name);
                    let mut chat_span = tracer
                        .span_builder(chat_span_name)
                        .with_kind(SpanKind::Client)
                        .with_attributes(vec![
                            KeyValue::new("gen_ai.operation.name", "chat"),
                            KeyValue::new("gen_ai.system", provider_system),
                            KeyValue::new("gen_ai.request.model", model_name),
                        ])
                        .start(&tracer);
                    let next_response = self
                        .complete_with_fallback_ladder(history, config, resolved_tier)
                        .await?;
                    // Record token usage and finish reason
                    chat_span.set_attribute(KeyValue::new(
                        "gen_ai.usage.input_tokens",
                        next_response.usage.input_tokens as i64,
                    ));
                    chat_span.set_attribute(KeyValue::new(
                        "gen_ai.usage.output_tokens",
                        next_response.usage.output_tokens as i64,
                    ));
                    // D-14a: surface cache_read/cache_write (Plans 08-02/08-04) so the
                    // cache-hit effect is observable, not just theoretically possible.
                    chat_span.set_attributes(cache_usage_attributes(&next_response.usage));
                    let finish_reason = if next_response.tool_calls.is_some() {
                        "tool_calls"
                    } else {
                        "stop"
                    };
                    chat_span.set_attribute(KeyValue::new(
                        "gen_ai.response.finish_reasons",
                        finish_reason,
                    ));
                    // SECURITY: NÃO emitir gen_ai.input/output.messages por padrão (PII — T-05-05-01)
                    // Opt-in via BASTION_OTEL_CONTENT_EVENTS=true
                    if std::env::var("BASTION_OTEL_CONTENT_EVENTS").as_deref() == Ok("true") {
                        chat_span.set_attribute(KeyValue::new(
                            "gen_ai.output.messages",
                            next_response.text.clone(),
                        ));
                    }
                    chat_span.end();

                    // Update budget with actual cost
                    let cost_usd = estimate_cost_usd(&provider_name, &next_response.usage);
                    if let Err(e) = self.session.update_budget(cost_usd).await {
                        tracing::warn!(error = %e, "failed to update budget");
                    }

                    response = next_response;
                }
            }
        }
    }

    /// Resolve the fallback candidate's `Provider` instance (D-10 rung 3).
    ///
    /// Production always delegates to the real `registry::resolve_provider` (which
    /// constructs a live, credential/network-backed provider). Test builds check
    /// `fallback_resolver_override` first so unit tests can inject a scripted `Provider`
    /// without real network or provider credentials (mirrors the `#[cfg(test)]
    /// drain_handle` seam elsewhere in this file).
    #[cfg(not(test))]
    fn resolve_fallback_provider(
        &self,
        candidate: &str,
    ) -> anyhow::Result<Box<dyn crate::provider::Provider>> {
        crate::provider::registry::resolve_provider(candidate)
    }

    #[cfg(test)]
    fn resolve_fallback_provider(
        &self,
        candidate: &str,
    ) -> anyhow::Result<Box<dyn crate::provider::Provider>> {
        match &self.fallback_resolver_override {
            Some(f) => f(candidate),
            None => crate::provider::registry::resolve_provider(candidate),
        }
    }

    /// D-10 fallback ladder — rung 1 (transient retry) + rung 3 (provider-switch on
    /// hard/persistent failure). Rung 2 (schema/parse forced-tool-call) is Plan 08-07's
    /// concern, scoped to structured-output callers (`router::route`, `cabinet::synth`,
    /// `learn::Reflector`) — it does not apply here, since the main agent tool loop never
    /// sets `CallConfig.response_format`.
    ///
    /// Shared by both provider-call sites (`dispatch_tool_loop`, `run_provider_fallback`)
    /// so the ladder logic exists exactly once (core = mechanism, not orchestrator — no
    /// duplicated retry/switch logic per call site).
    ///
    /// Bounded to ONE switch per call: if the switched-to provider also fails, that error
    /// propagates unchanged (no cascading through the rest of `fallback_models`). An empty
    /// `fallback_models` — or one where every configured entry equals the CURRENT
    /// provider's `model_name()` — preserves today's exact behavior: the original
    /// retry-exhaustion error propagates, byte-identical to before this plan.
    async fn complete_with_fallback_ladder(
        &mut self,
        history: &[Message],
        config: &CallConfig,
        resolved_tier: Option<crate::memory::PrivacyTier>,
    ) -> anyhow::Result<crate::types::LlmResponse> {
        // Rung 1 — transient retry, exactly as today.
        let rung1 = {
            let provider = self.provider.read().await;
            let prov_ref: &dyn crate::provider::Provider = &**provider;
            call_with_retry(|| prov_ref.complete(history, config), 3).await
        };
        let original_err = match rung1 {
            Ok(resp) => return Ok(resp),
            Err(e) => e,
        };

        // Rung 3 — switch to the first configured fallback model that isn't the current
        // provider. Empty list / all-entries-are-current-provider => zero behavior change.
        let current_model = self.provider.read().await.model_name().to_owned();
        let candidate = self
            .fallback_models
            .iter()
            .find(|m| m.as_str() != current_model.as_str())
            .cloned();
        let Some(candidate) = candidate else {
            return Err(original_err);
        };

        // resolve_provider() itself never fails in practice (every registry.rs branch
        // returns Ok; the underlying `::new()` may panic on a missing API key — a
        // pre-existing, accepted pattern, T-08-08-01). Handled defensively regardless:
        // an unresolvable candidate falls back to the ORIGINAL error, not a new one.
        let new_provider = match self.resolve_fallback_provider(&candidate) {
            Ok(p) => p,
            Err(_) => return Err(original_err),
        };

        let from_provider_name = self.provider.read().await.name().to_owned();
        tracing::warn!(
            event = "provider_fallback_switch",
            from = %from_provider_name,
            to_model = %candidate,
            error = %original_err,
        );

        // T-08-08-02 (mitigate): re-check egress against the NEW provider BEFORE the
        // swap and BEFORE the retry call — a fallback that would violate the turn's
        // privacy tier never gets swapped in.
        crate::hooks::egress::check_egress(resolved_tier, new_provider.name())?;

        *self.provider.write().await = new_provider;

        let provider = self.provider.read().await;
        let prov_ref: &dyn crate::provider::Provider = &**provider;
        call_with_retry(|| prov_ref.complete(history, config), 3).await
    }

    /// Classic tool-loop provider call — used as fallback when registry is empty.
    /// `session_id` is the per-owner session resolved by the caller (run_turn_for).
    /// `owner` and `user_input` are passed so build_system_prompt can apply the per-block
    /// egress check (SEAM #2 / T-05-03-03: prevents LocalOnly beliefs leaking on fallback path).
    async fn run_provider_fallback(
        &mut self,
        history: &mut Vec<Message>,
        session_id: &str,
        owner: &str,
        user_input: &str,
        turn_tier: Option<crate::memory::PrivacyTier>,
        turn_persona: Option<&str>,
    ) -> anyhow::Result<String> {
        // Build tool definitions from ToolRegistry.
        // D-12/D-14b: list_tool_names() returns sorted-by-name output since Plan 08-02's
        // mcp/registry.rs fix (was iteration-order-dependent HashMap output before) — this
        // tools array is part of CallConfig and therefore part of the byte-stable-prefix
        // contract build_system_prompt documents; no code change needed here, confirming only.
        let tools: Vec<serde_json::Value> = self
            .mcp
            .registry()
            .list_tool_names()
            .iter()
            .map(|name| {
                let schema = self
                    .mcp
                    .registry()
                    .get_tool_schema(name)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
                serde_json::json!({
                    "name": name,
                    "description": format!("External tool: {}", name),
                    "input_schema": schema
                })
            })
            .collect();

        // SEAM #2: build_system_prompt applies per-block egress check so LocalOnly blocks
        // are not injected when the active provider is cloud. This covers the fallback path
        // (T-05-03-03 mitigation — egress leak in fallback path).
        let system_prompt = self
            .build_system_prompt(owner, user_input, turn_persona)
            .await;
        let config = CallConfig {
            system_prompt,
            max_tokens: 4096,
            tools,
            ..Default::default()
        };

        // WR-04 / WR-01 (review #2): the turn's PrivacyTier is resolved ONCE in run_turn_for
        // (from the handling persona, before `decision` is consumed) and threaded in here.
        // Previously this re-read the already-taken `self.forced_persona` (always None at this
        // point), so a forced CloudOk persona was over-blocked and LocalOnly safety relied on
        // an accidental None collapse. Tier comes from the trusted PersonaRegistry, never from
        // MCP tool results (T-04-02-03). None stays fail-closed per check_egress contract.
        let resolved_tier: Option<crate::memory::PrivacyTier> = turn_tier;

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
            if provider_name != "ollama"
                && !self.session.check_budget(self.daily_budget_usd).await?
            {
                anyhow::bail!(BastionError::BudgetExceeded);
            }

            // WR-01 (review #2): fail-closed egress gate on EVERY round, not just pre-loop.
            // Subsequent rounds re-send `history` (which may carry LocalOnly tool results) to
            // the provider; mirror the per-round gate in dispatch_tool_loop. (The pre-loop
            // check above covers round 0; this covers all rounds uniformly.)
            crate::hooks::egress::check_egress(resolved_tier, &provider_name)?;

            // LLM call — delegates rung 1 (retry) + rung 3 (provider-switch, D-10) to the
            // shared ladder. Egress for THIS round was already checked above; a switch
            // inside the ladder re-checks egress again against the NEW provider before
            // swapping (T-08-08-02).
            let response = self
                .complete_with_fallback_ladder(history, &config, resolved_tier)
                .await?;

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
                    std::iter::once(ContentPart::Text {
                        text: response.text.clone(),
                    })
                    .chain(tc.iter().map(|t| ContentPart::ToolUse {
                        id: t.id.clone(),
                        name: t.name.clone(),
                        input: t.arguments.clone(),
                        extra: t.extra.clone(),
                    }))
                    .collect(),
                )
            } else {
                MessageContent::Text(response.text.clone())
            };
            self.session
                .append(
                    session_id,
                    Message {
                        role: Role::Assistant,
                        content: assistant_content.clone(),
                    },
                    Some(response.usage.output_tokens),
                )
                .await?;
            history.push(Message {
                role: Role::Assistant,
                content: assistant_content,
            });

            // Tool dispatch
            match response.tool_calls {
                None => break response.text, // final answer — no more tool calls
                Some(tool_calls) => {
                    for tc in &tool_calls {
                        tracing::debug!(event = "tool_dispatch", tool = %tc.name);
                        // WR-02 (review #2): the fallback dispatches MCP tools directly (registry
                        // bypass), so it must apply the same egress policy registry.invoke applies
                        // to a non-local (MCP) capability — gate the turn tier against "external"
                        // before dispatch (D-13). On block, return an error result and keep the
                        // loop going (parity with registry.invoke's caught-error behavior), rather
                        // than executing the tool ungated.
                        let result =
                            match crate::hooks::egress::check_egress(resolved_tier, "external") {
                                Err(e) => serde_json::json!({ "error": e.to_string() }),
                                Ok(()) => self
                                    .mcp
                                    .call_tool_with_timeout(&tc.name, tc.arguments.clone())
                                    .await
                                    .unwrap_or_else(
                                        |e| serde_json::json!({ "error": e.to_string() }),
                                    ),
                            };

                        // D-06: handle skill_reloaded signal from skill-writer container
                        // (shared helper — also used by dispatch_tool_loop, Gap 1 fix).
                        self.handle_skill_reload(&result);

                        let result_str = result.to_string();
                        let tool_msg = Message {
                            role: Role::Tool,
                            content: MessageContent::Parts(vec![ContentPart::ToolResult {
                                tool_use_id: tc.id.clone(),
                                content: result_str,
                            }]),
                        };
                        self.session
                            .append(session_id, tool_msg.clone(), None)
                            .await?;
                        history.push(tool_msg);
                    }
                    rounds += 1;
                }
            }
        };

        Ok(final_text)
    }

    /// CR-02: inject the shared OTC store handle so `/connect-app` can mint pairing
    /// codes that `/auth/exchange` (in the webhook server) consumes. Called by main.rs
    /// after `new_otc_store()`; the same Arc is also moved into `serve_with_mesh`.
    pub fn set_otc_store(&mut self, store: crate::channel::webhook::OtcStore) {
        self.otc_store = Some(store);
    }

    /// SEC-03: inject the Composio OAuth client so `/connect-app-composio` can
    /// initiate real OAuth connections. Called by main.rs when `COMPOSIO_API_KEY`
    /// is configured (opt-in — mirrors `set_otc_store`'s shape exactly).
    pub fn set_composio_oauth(&mut self, oauth: std::sync::Arc<crate::mcp::oauth::ComposioOAuth>) {
        self.composio_oauth = Some(oauth);
    }

    pub async fn handle_command(
        &mut self,
        input: &str,
        owner: &str,
    ) -> anyhow::Result<CommandResult> {
        handle_command(
            input,
            &self.provider,
            &self.registry,
            &self.memory,
            &mut self.forced_persona,
            self.otc_store.as_ref(),
            self.composio_oauth.as_deref(),
            owner,
        )
        .await
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
    pub async fn drain_handle(
        &mut self,
        mut rx: mpsc::Receiver<crate::agent::handle::AgentRequest>,
    ) {
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
///
/// SEC-02 (D-04/D-05): a provider's own reported per-request cost always wins when
/// present (`TokenUsage.actual_cost_usd`, e.g. OpenRouter's `usage.cost`) — the
/// hardcoded tables below are a fallback ONLY, used when the provider never reports a
/// cost field at all (Gemini, always — confirmed no cost field exists in
/// `usageMetadata`, RESEARCH Pitfall 3) or reports one that's momentarily absent.
///
/// Per AI-SPEC §4b.5: claude-sonnet-4-5 ≈ $3/1M input, $15/1M output
fn estimate_cost_usd(provider: &str, usage: &TokenUsage) -> f64 {
    if let Some(real) = usage.actual_cost_usd {
        return real;
    }

    match provider {
        "anthropic" => {
            let input_cost = usage.input_tokens as f64 * 3.0 / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 15.0 / 1_000_000.0;
            input_cost + output_cost
        }
        "openai" => {
            let input_cost = usage.input_tokens as f64 * 2.5 / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 10.0 / 1_000_000.0;
            input_cost + output_cost
        }
        // OpenRouter aggregates many models at different price points; `usage.cost`
        // (real, per-request) is the normal path and always wins above. This is a
        // conservative blended-average estimate for the rare case that field is
        // momentarily missing — never 0.0 for a paid provider (SEC-02, the original
        // defect being fixed here). Source: openrouter.ai/models blended free+paid
        // average as of 2026-07.
        "openrouter" => {
            let input_cost = usage.input_tokens as f64 * 0.5 / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 1.5 / 1_000_000.0;
            input_cost + output_cost
        }
        // Gemini never reports a cost field (RESEARCH Pitfall 3) — this arm is always
        // consulted for Gemini, not just a fallback. Rates match Gemini 2.5 Flash
        // published pricing as of 2026-07 (ai.google.dev/pricing).
        "gemini" => {
            let input_cost = usage.input_tokens as f64 * 0.3 / 1_000_000.0;
            let output_cost = usage.output_tokens as f64 * 2.5 / 1_000_000.0;
            input_cost + output_cost
        }
        "ollama" => 0.0, // local — no cost
        _ => 0.0,
    }
}

/// D-14a: `gen_ai.usage.cache_read_tokens`/`gen_ai.usage.cache_write_tokens` OTel span
/// attributes, mirroring the existing `gen_ai.usage.input_tokens`/`output_tokens` naming
/// convention. `TokenUsage.cache_read`/`cache_write` are populated by Plans 08-02
/// (Anthropic `cache_control`) and 08-04 (OpenAI/Groq/OpenRouter `prompt_tokens_details.
/// cached_tokens`) — this is the missing telemetry step that surfaces them.
///
/// Always emits BOTH attributes, including the `0` case — Groq's expected-zero
/// `cache_read` (Pitfall 6) must be an observable measured `0`, not an absent field, so a
/// dashboard can distinguish "measured zero" from "not wired".
fn cache_usage_attributes(usage: &TokenUsage) -> Vec<KeyValue> {
    vec![
        KeyValue::new("gen_ai.usage.cache_read_tokens", usage.cache_read as i64),
        KeyValue::new("gen_ai.usage.cache_write_tokens", usage.cache_write as i64),
    ]
}

#[cfg(test)]
mod cache_usage_attributes_tests {
    use super::{cache_usage_attributes, TokenUsage};

    #[test]
    fn emits_both_attributes_including_zero() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 20,
            cache_read: 0,
            cache_write: 0,
            ..Default::default()
        };
        let attrs = cache_usage_attributes(&usage);
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].key.as_str(), "gen_ai.usage.cache_read_tokens");
        assert_eq!(attrs[0].value.to_string(), "0");
        assert_eq!(attrs[1].key.as_str(), "gen_ai.usage.cache_write_tokens");
        assert_eq!(attrs[1].value.to_string(), "0");
    }

    #[test]
    fn emits_nonzero_values() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 20,
            cache_read: 1200,
            cache_write: 340,
            ..Default::default()
        };
        let attrs = cache_usage_attributes(&usage);
        assert_eq!(attrs[0].value.to_string(), "1200");
        assert_eq!(attrs[1].value.to_string(), "340");
    }
}

// ---------------------------------------------------------------------------
// Tests (offline — MockProvider + temp-DB memory + single-persona registry)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::{GoalEngine, ScoringConfig};
    use crate::memory::sqlite::SqliteMemory;
    use crate::memory::PrivacyTier;
    use crate::persona::{Persona, PersonaRegistry};
    use crate::provider::{Provider, SharedProvider};
    use crate::types::{CallConfig, LlmResponse, Message};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;

    #[test]
    fn estimate_cost_usd_real_cost_always_wins_over_hardcoded_table() {
        let usage = TokenUsage {
            actual_cost_usd: Some(0.0021),
            ..Default::default()
        };
        assert_eq!(estimate_cost_usd("openrouter", &usage), 0.0021);
    }

    #[test]
    fn estimate_cost_usd_openrouter_fallback_is_never_zero() {
        let usage = TokenUsage {
            actual_cost_usd: None,
            input_tokens: 1000,
            output_tokens: 500,
            ..Default::default()
        };
        assert!(estimate_cost_usd("openrouter", &usage) > 0.0);
    }

    #[test]
    fn estimate_cost_usd_gemini_fallback_is_never_zero() {
        let usage = TokenUsage {
            actual_cost_usd: None,
            input_tokens: 1000,
            output_tokens: 500,
            ..Default::default()
        };
        assert!(estimate_cost_usd("gemini", &usage) > 0.0);
    }

    #[test]
    fn estimate_cost_usd_existing_providers_unchanged() {
        let usage = TokenUsage {
            actual_cost_usd: None,
            input_tokens: 1000,
            output_tokens: 500,
            ..Default::default()
        };
        assert_eq!(
            estimate_cost_usd("anthropic", &usage),
            1000.0 * 3.0 / 1_000_000.0 + 500.0 * 15.0 / 1_000_000.0
        );
        assert_eq!(
            estimate_cost_usd("openai", &usage),
            1000.0 * 2.5 / 1_000_000.0 + 500.0 * 10.0 / 1_000_000.0
        );
        assert_eq!(estimate_cost_usd("ollama", &usage), 0.0);
    }

    // MockProvider: complete_simple echoes a persona response.
    struct MockProvider {
        persona_name: String,
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
            Ok(LlmResponse {
                text: format!("response from {}", self.persona_name),
                tool_calls: None,
                usage: crate::types::TokenUsage {
                    input_tokens: 10,
                    output_tokens: 10,
                    cache_read: 0,
                    cache_write: 0,
                    ..Default::default()
                },
            })
        }
        async fn complete_simple(&self, _prompt: &str) -> anyhow::Result<String> {
            Ok(format!("simple:{}", self.persona_name))
        }
        fn context_limit(&self) -> usize {
            8192
        }
        fn model_name(&self) -> &str {
            "mock"
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    fn make_provider(name: &str) -> SharedProvider {
        Arc::new(RwLock::new(Box::new(MockProvider {
            persona_name: name.to_string(),
        }) as Box<dyn Provider>))
    }

    fn make_registry(name: &str) -> PersonaRegistry {
        let mut personas = HashMap::new();
        personas.insert(
            name.to_string(),
            Persona {
                name: name.to_string(),
                description: Some("Test persona".to_string()),
                system_prompt: format!("You are {name}."),
                tier: PrivacyTier::CloudOk,
                weight: 0.8,
                skills: vec![],
            },
        );
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
        let mcp = McpClient::connect_all("nonexistent_mcp.json")
            .await
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
            vec![],
            db_path,
        )
    }

    #[tokio::test]
    async fn run_turn_benign_message_returns_persona_response() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        let resp = agent
            .run_turn("hello world")
            .await
            .expect("run_turn failed");
        assert!(
            !resp.is_empty(),
            "response must not be empty; got: {resp:?}"
        );
    }

    #[tokio::test]
    async fn context_block_local_only_dropped_on_cloud_provider() {
        use crate::agent::context::{ContextBlock, TurnContextProvider};
        use crate::memory::PrivacyTier;

        struct LocalOnlyProvider;

        #[async_trait]
        impl TurnContextProvider for LocalOnlyProvider {
            async fn context_for_turn(
                &self,
                _owner: &str,
                _msg: &str,
                _persona: Option<&str>,
            ) -> Vec<ContextBlock> {
                vec![ContextBlock {
                    content: "secret-belief".to_owned(),
                    max_tier: PrivacyTier::LocalOnly,
                }]
            }
        }

        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        // Register a LocalOnly provider — MockProvider has name() == "mock" (non-ollama cloud).
        agent.context_providers.push(Box::new(LocalOnlyProvider));

        // build_system_prompt with a non-ollama provider must discard the LocalOnly block.
        let system_prompt = agent
            .build_system_prompt(DEFAULT_OWNER, "hello", None)
            .await;
        assert!(
            !system_prompt.contains("secret-belief"),
            "LocalOnly block must not appear in system prompt when provider is cloud; got: {system_prompt:?}"
        );
    }

    #[tokio::test]
    async fn run_turn_contestation_phrase_revokes_belief() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        // Pre-store a belief
        {
            let mem = agent.memory.read().await;
            mem.store_belief(
                DEFAULT_OWNER,
                None,
                "Mario exercises every morning",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store_belief");
        }

        // Verify belief is stored
        let before = {
            let mem = agent.memory.read().await;
            mem.retrieve_tagged(DEFAULT_OWNER, None)
                .await
                .expect("retrieve")
        };
        assert_eq!(before.len(), 1, "belief must exist before contestation");

        // Run a turn with a contestation phrase that overlaps with the belief
        let _ = agent
            .run_turn("isso não é mais verdade sobre exercises morning")
            .await;

        // After the turn, the output-validator should have revoked the belief
        let after = {
            let mem = agent.memory.read().await;
            mem.retrieve_tagged(DEFAULT_OWNER, None)
                .await
                .expect("retrieve")
        };
        assert!(
            after.is_empty(),
            "belief must be revoked after contestation turn"
        );
    }

    // Guards CR-01/CR-02 (privacy egress through the tool loop):
    // 1. resolved_tier must come from the persona actually handling the turn
    //    (the returned pid), not from self.forced_persona — which is already
    //    consumed by .take() in run_turn_for, so re-reading it yielded None and
    //    a LocalOnly persona was stamped CloudOk.
    // 2. the new per-round check_egress in dispatch_tool_loop must NOT over-block
    //    a legitimate CloudOk persona's multi-round tool loop.
    #[tokio::test]
    async fn cloud_ok_persona_tool_loop_passes_egress_gate() {
        use crate::types::{TokenUsage, ToolCall};
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Round 0 returns a tool_call (forces a second provider round through
        // dispatch_tool_loop, where the new egress gate lives); round 1 returns
        // final text to terminate the loop.
        struct ToolThenText {
            calls: AtomicUsize,
        }

        #[async_trait]
        impl Provider for ToolThenText {
            async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Ok(LlmResponse {
                        text: String::new(),
                        tool_calls: Some(vec![ToolCall {
                            id: "t1".to_owned(),
                            name: "noop".to_owned(),
                            arguments: serde_json::json!({}),
                            extra: None,
                        }]),
                        usage: TokenUsage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read: 0,
                            cache_write: 0,
                            ..Default::default()
                        },
                    })
                } else {
                    Ok(LlmResponse {
                        text: "done".to_owned(),
                        tool_calls: None,
                        usage: TokenUsage {
                            input_tokens: 1,
                            output_tokens: 1,
                            cache_read: 0,
                            cache_write: 0,
                            ..Default::default()
                        },
                    })
                }
            }
            async fn complete_simple(&self, _prompt: &str) -> anyhow::Result<String> {
                Ok("s".to_owned())
            }
            fn context_limit(&self) -> usize {
                8192
            }
            fn model_name(&self) -> &str {
                "mock"
            }
            fn name(&self) -> &'static str {
                "mock"
            }
        }

        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();

        let session = crate::session::SessionManager::new(&path);
        session.init_schema().await.expect("init_schema");
        let session_id = session.create_session().await.expect("create_session");
        let memory: SharedMemory = Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(&path)) as Box<dyn crate::memory::Memory>
        ));
        let mcp = McpClient::connect_all("nonexistent_mcp.json")
            .await
            .expect("connect_all empty");

        let mut personas = HashMap::new();
        personas.insert(
            "Cloudy".to_string(),
            Persona {
                name: "Cloudy".to_string(),
                description: Some("Cloud-ok persona".to_string()),
                system_prompt: "You are Cloudy.".to_string(),
                tier: PrivacyTier::CloudOk,
                weight: 0.9,
                skills: vec![],
            },
        );
        let registry = PersonaRegistry::new_from_map(personas);

        let provider: SharedProvider = Arc::new(RwLock::new(Box::new(ToolThenText {
            calls: AtomicUsize::new(0),
        }) as Box<dyn Provider>));

        let mut agent = AgentLoop::new(
            provider,
            session,
            mcp,
            session_id,
            10.0,
            registry,
            memory,
            GoalEngine::new(&path, ScoringConfig::default()),
            vec![],
            &path,
        );

        // CloudOk persona + cloud provider: the multi-round tool loop must complete,
        // proving the per-round egress gate resolves Some(CloudOk) and lets it through.
        let resp = agent
            .run_turn("do a thing")
            .await
            .expect("CloudOk persona tool loop must not be egress-blocked");
        assert_eq!(
            resp, "done",
            "tool loop must run a second round and return final text"
        );
    }

    // --- Plan 08-08 (SO-03): complete_with_fallback_ladder --------------------------
    //
    // `complete_with_fallback_ladder` is a private method — these are unit tests
    // (not the `tests/provider_hotswap.rs` integration test) because the ladder's
    // provider-switch rung is only injectable via the `#[cfg(test)]
    // fallback_resolver_override` seam, which does not exist in the library as seen
    // by integration-test binaries (they link the crate compiled WITHOUT `--cfg
    // test`). Exercising the ladder end-to-end here — directly, via `make_loop` —
    // is the only place these 3 scenarios can assert on the private swap behavior.

    struct AlwaysFailProvider;

    #[async_trait]
    impl Provider for AlwaysFailProvider {
        async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
            // "HTTP 400" short-circuits call_with_retry's backoff (see
            // src/provider/mod.rs) so this test asserts rung-3 behavior without
            // waiting through 3 retries — this also models the class of
            // hard/non-transient failure rung 3 exists to handle.
            anyhow::bail!("HTTP 400: primary provider unavailable")
        }
        async fn complete_simple(&self, _: &str) -> anyhow::Result<String> {
            anyhow::bail!("HTTP 400: primary provider unavailable")
        }
        fn context_limit(&self) -> usize {
            8192
        }
        fn model_name(&self) -> &str {
            "primary-model"
        }
        fn name(&self) -> &'static str {
            "primary"
        }
    }

    #[tokio::test]
    async fn fallback_ladder_switches_provider_on_hard_failure() {
        use std::sync::atomic::{AtomicU32, Ordering};

        struct FallbackOkProvider;
        #[async_trait]
        impl Provider for FallbackOkProvider {
            async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
                Ok(LlmResponse {
                    text: "response from fallback".to_owned(),
                    tool_calls: None,
                    usage: crate::types::TokenUsage {
                        input_tokens: 5,
                        output_tokens: 5,
                        cache_read: 0,
                        cache_write: 0,
                        ..Default::default()
                    },
                })
            }
            async fn complete_simple(&self, _: &str) -> anyhow::Result<String> {
                Ok("ok".to_owned())
            }
            fn context_limit(&self) -> usize {
                8192
            }
            fn model_name(&self) -> &str {
                "mock2"
            }
            fn name(&self) -> &'static str {
                "fallback"
            }
        }

        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        agent.provider = Arc::new(RwLock::new(
            Box::new(AlwaysFailProvider) as Box<dyn Provider>
        ));
        agent.fallback_models = vec!["mock2".to_owned()];

        let resolve_calls = Arc::new(AtomicU32::new(0));
        agent.fallback_resolver_override = Some(Box::new({
            let resolve_calls = resolve_calls.clone();
            move |candidate: &str| {
                assert_eq!(candidate, "mock2");
                resolve_calls.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(FallbackOkProvider) as Box<dyn Provider>)
            }
        }));

        let history: Vec<Message> = vec![];
        let config = CallConfig::default();
        let resp = agent
            .complete_with_fallback_ladder(&history, &config, Some(PrivacyTier::CloudOk))
            .await
            .expect("ladder must succeed via fallback switch");

        assert_eq!(resp.text, "response from fallback");
        assert_eq!(resolve_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            agent.provider.read().await.name(),
            "fallback",
            "active provider must be swapped to the fallback"
        );
    }

    #[tokio::test]
    async fn fallback_ladder_empty_list_propagates_original_error_unchanged() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        agent.provider = Arc::new(RwLock::new(
            Box::new(AlwaysFailProvider) as Box<dyn Provider>
        ));
        assert!(
            agent.fallback_models.is_empty(),
            "make_loop fixture defaults to no fallback list"
        );

        let history: Vec<Message> = vec![];
        let config = CallConfig::default();
        let err = agent
            .complete_with_fallback_ladder(&history, &config, Some(PrivacyTier::CloudOk))
            .await
            .expect_err("empty fallback_models must propagate the original error, not swap");

        assert!(
            err.to_string().contains("HTTP 400"),
            "propagated error must be the ORIGINAL error unchanged, got: {err}"
        );
        assert_eq!(
            agent.provider.read().await.name(),
            "primary",
            "provider must not be swapped when fallback_models is empty"
        );
    }

    #[tokio::test]
    async fn fallback_ladder_rechecks_egress_before_switching_and_before_retry() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // A resolvable fallback whose provider NAME ("anthropic") is a cloud
        // provider — check_egress(LocalOnly, "anthropic") must block it BEFORE
        // this provider's complete() is ever called.
        struct NeverCalledCloudProvider {
            called: Arc<AtomicBool>,
        }
        #[async_trait]
        impl Provider for NeverCalledCloudProvider {
            async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
                self.called.store(true, Ordering::SeqCst);
                Ok(LlmResponse {
                    text: "should never be returned".to_owned(),
                    tool_calls: None,
                    usage: crate::types::TokenUsage::default(),
                })
            }
            async fn complete_simple(&self, _: &str) -> anyhow::Result<String> {
                Ok("ok".to_owned())
            }
            fn context_limit(&self) -> usize {
                8192
            }
            fn model_name(&self) -> &str {
                "gpt-4o"
            }
            fn name(&self) -> &'static str {
                "anthropic"
            }
        }

        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mut agent = make_loop(&path).await;

        agent.provider = Arc::new(RwLock::new(
            Box::new(AlwaysFailProvider) as Box<dyn Provider>
        ));
        agent.fallback_models = vec!["gpt-4o".to_owned()];

        let called = Arc::new(AtomicBool::new(false));
        agent.fallback_resolver_override = Some(Box::new({
            let called = called.clone();
            move |_candidate: &str| {
                Ok(Box::new(NeverCalledCloudProvider {
                    called: called.clone(),
                }) as Box<dyn Provider>)
            }
        }));

        let history: Vec<Message> = vec![];
        let config = CallConfig::default();
        let err = agent
            .complete_with_fallback_ladder(&history, &config, Some(PrivacyTier::LocalOnly))
            .await
            .expect_err("egress-blocked fallback provider must return the egress error");

        assert!(
            !called.load(Ordering::SeqCst),
            "the fallback provider's complete() must never be called — egress must \
             block before the retry"
        );
        assert!(
            err.downcast_ref::<BastionError>()
                .map(|e| matches!(e, BastionError::PrivacyEgressBlocked))
                .unwrap_or(false),
            "expected PrivacyEgressBlocked, got: {err:?}"
        );
        assert_eq!(
            agent.provider.read().await.name(),
            "primary",
            "provider must NOT be swapped when the new provider fails egress"
        );
    }
}
