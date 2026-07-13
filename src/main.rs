use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing_subscriber::fmt;

use bastion::agent::handle;
use bastion::agent::loop_::AgentLoop;
use bastion::goal::{GoalEngine, ScoringConfig};
use bastion::mcp::McpClient;
use bastion::memory::sqlite::SqliteMemory;
use bastion::persona::PersonaRegistry;
use bastion::proactive::CronService;
use bastion::provider::registry::resolve_provider;
use bastion::session::SessionManager;

/// Inicializa o OTel TracerProvider.
///
/// stdout exporter é opt-in via `BASTION_OTEL_STDOUT=true` (off por padrão — não polui o REPL).
/// Se `OTEL_EXPORTER_OTLP_ENDPOINT` estiver setado, adiciona OTLP/gRPC exporter.
///
/// SECURITY: não emite conteúdo de conversa por padrão —
/// `gen_ai.input.messages` só é adicionado se `BASTION_OTEL_CONTENT_EVENTS=true`.
///
/// PITFALL 6: deve ser chamado ANTES de AgentLoop::new() para que spans criados
/// dentro do AgentLoop não sejam descartados em silêncio (no-op tracer).
fn init_otel_provider() -> anyhow::Result<opentelemetry_sdk::trace::SdkTracerProvider> {
    use opentelemetry_sdk::trace::SdkTracerProvider;

    let mut provider_builder = SdkTracerProvider::builder();

    // stdout exporter opt-in — off por padrão p/ não afogar o REPL do daemon.
    // ponytail: era sempre-ligado; agora atrás de BASTION_OTEL_STDOUT=true.
    if std::env::var("BASTION_OTEL_STDOUT").as_deref() == Ok("true") {
        provider_builder =
            provider_builder.with_batch_exporter(opentelemetry_stdout::SpanExporter::default());
    }

    // OTLP exporter opcional — só se endpoint configurado
    let provider = if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        use opentelemetry_otlp::{SpanExporter, WithExportConfig};
        let otlp_exporter = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()?;
        provider_builder.with_batch_exporter(otlp_exporter).build()
    } else {
        provider_builder.build()
    };

    Ok(provider)
}

#[derive(Parser)]
#[command(name = "bastion", about = "Bastion AI agent runtime", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute a single-turn agent call and exit
    Agent {
        #[arg(short = 'm', long, help = "Message to send to the agent")]
        message: String,
    },
    /// Start long-running REPL daemon (reads stdin, responds, loops)
    Daemon,
    /// Start MCP server over stdio (local subprocess transport).
    /// Used by local agents that control lifecycle (Claude Code, opencode, etc.).
    McpStdio,
    /// Export agent identity, memories, goals, personas, and config to .af file
    Export {
        /// Export mode: full or template
        #[arg(long, default_value = "full")]
        mode: String,
        /// Include identity secrets in full exports
        #[arg(long)]
        with_identity: bool,
        /// Output path for the .af file
        #[arg(short = 'o', long)]
        output: String,
    },
    /// Import agent identity, memories, goals from .af file
    Import {
        /// Input path to the .af file (omit for stdin)
        input: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env (if present) before any std::env::var read. Real shell env wins.
    dotenvy::dotenv().ok();

    // Load bastion.toml config (non-secret config only; secrets stay in .env)
    let config_path = std::env::var("BASTION_CONFIG").unwrap_or_else(|_| "bastion.toml".to_owned());
    let cfg = bastion::config::load_config(&config_path)?;

    // Init structured JSON logging
    std::fs::create_dir_all(
        std::path::Path::new(&cfg.logging.log_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(".bastion")),
    )?;
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.logging.log_path)?;

    fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(log_file)
        .init();

    let cli = Cli::parse();

    // Init SessionManager
    let db_path = cfg.session.db_path.clone();
    let session = SessionManager::new(&db_path);
    session.init_schema().await?;

    // D-02: auto-resume most recent session, or create new one
    let session_id = match session.load_most_recent_id().await? {
        Some(id) => {
            tracing::info!(event = "session_resumed", session_id = %id);
            id
        }
        None => {
            let id = session.create_session().await?;
            tracing::info!(event = "session_created", session_id = %id);
            id
        }
    };

    // Init MCP client from bastion.toml [mcp.servers] (D-09). connect_from_config handles
    // failed servers gracefully: logs tracing::warn per failed server and continues.
    // (Previously this used the legacy .bastion/mcp-servers.json path, which isn't mounted
    // in the FROM-scratch container — so memupalace/skill-writer tools were silently absent.)
    let mut mcp_client = McpClient::connect_from_config(&cfg.mcp.servers).await?;

    // SEC-03: Composio OAuth is opt-in — only constructed when COMPOSIO_API_KEY is
    // actually set. ComposioOAuth::new() itself panics on a missing/empty key (a
    // deliberate fail-loud contract for direct callers), so this guard is what keeps
    // the daemon from panicking at startup for deployments that simply don't use
    // Composio at all.
    let composio_oauth: Option<Arc<bastion::mcp::ComposioOAuth>> =
        if std::env::var("COMPOSIO_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
            let oauth = Arc::new(bastion::mcp::ComposioOAuth::new(&db_path));
            mcp_client = mcp_client.with_composio_oauth(oauth.clone());
            tracing::info!(event = "composio_oauth_enabled");
            Some(oauth)
        } else {
            None
        };
    // M2 (P3 `ToolSource`): shared by-Arc so it can back both the loop's
    // `ToolSource` port AND the Reflector's directly-registered `McpToolAdapter`
    // (below, in `daemon_loop`) — the SAME connected client, never a second
    // connection. Wrapped only after `with_composio_oauth` above, which still
    // needs owned `McpClient`.
    let mcp_client = Arc::new(mcp_client);
    let mcp_for_product = mcp_client.clone();

    // Init provider from config (default_model from bastion.toml, overridable via env BASTION__AGENT__DEFAULT_MODEL)
    let default_model = cfg.agent.default_model.clone();
    let provider: bastion::provider::SharedProvider =
        Arc::new(RwLock::new(resolve_provider(&default_model)?));

    let daily_budget = cfg.agent.daily_budget_usd;

    // Init persona registry (load from "./personas/" directory; empty if missing — PERS-07)
    let registry = PersonaRegistry::load_dir(".").await?;

    // Init shared memory
    let memory: bastion::memory::SharedMemory = Arc::new(RwLock::new(Box::new(SqliteMemory::new(
        &db_path,
    ))
        as Box<dyn bastion::memory::Memory>));

    // Init goal engine
    let goals = GoalEngine::new(&db_path, ScoringConfig::default());
    // M2 (P4 `GoalPort`): `agent.goals` becomes `Option<Arc<dyn GoalPort>>` below
    // — the loop only ever needed `list_goals`. Two other product-level
    // consumers still need the concrete `GoalEngine` (out of scope for this
    // cut, not loop internals): `BastionMcpServer` (its own `goals: GoalEngine`
    // field, src/mcp/server.rs) and `CronService` (needs `drift_nudge` too,
    // src/proactive/mod.rs) — both wired inside `daemon_loop`. Keep a plain
    // clone from BEFORE `goals` moves into the loop and thread it through,
    // rather than reaching into `agent.goals` (no longer the right type).
    let goals_for_product = goals.clone();

    // SEAM #4: inicializar OTel TracerProvider ANTES de AgentLoop::new()
    // (Pitfall 6: se chamado depois, spans no AgentLoop usariam no-op tracer)
    // OTel 0.32: SdkTracerProvider shuts down on drop — keep _otel_provider alive until end of main().
    let _otel_provider = init_otel_provider()
        .unwrap_or_else(|e| {
            tracing::warn!(event = "otel_init_failed", error = %e, "OTel init falhou — usando no-op tracer");
            opentelemetry_sdk::trace::SdkTracerProvider::builder().build()
        });
    opentelemetry::global::set_tracer_provider(_otel_provider.clone());

    let agent_identity: Option<Arc<bastion::identity::age_identity::AgeIdentity>> =
        if let Ok(identity_key) = std::env::var("MESH_IDENTITY_KEY") {
            match bastion::identity::age_identity::AgeIdentity::from_bech32(&identity_key) {
                Ok(id) => {
                    tracing::info!(event = "agent_identity_enabled");
                    Some(Arc::new(id))
                }
                Err(e) => {
                    // WR-03: route through the sanitizer instead of logging `e` directly —
                    // keeps the public-facing message generic even if a future error
                    // variant here ever carries secret material.
                    let sanitized =
                        bastion::identity::age_identity::sanitised_identity_error(&e.to_string());
                    tracing::warn!(event = "agent_identity_init_failed", error = %sanitized);
                    None
                }
            }
        } else {
            tracing::info!(event = "agent_identity_disabled");
            None
        };

    let mut agent = AgentLoop::new(
        provider.clone(),
        session,
        mcp_client,
        session_id,
        daily_budget,
        registry,
        memory.clone(),
        Some(std::sync::Arc::new(goals)),
        cfg.agent.fallback_models.clone(),
        &db_path,
        std::sync::Arc::new(bastion::eval::failure_sink::EvalFailureSink),
    );

    match cli.command {
        Command::Agent { message } => {
            let response = agent.run_turn(&message).await?;
            println!("{}", response);
        }
        Command::Daemon => {
            daemon_loop(
                &mut agent,
                &cfg,
                agent_identity,
                composio_oauth,
                goals_for_product,
                mcp_for_product,
            )
            .await?;
        }
        Command::McpStdio => {
            use rmcp::ServiceExt;

            let token_perms = build_token_perms(&cfg);
            let local_owner = std::env::var("BASTION_OWNER_ID")
                .unwrap_or_else(|_| bastion::agent::loop_::DEFAULT_OWNER.to_string());
            let personas = Arc::new(agent.registry.clone());
            let mcp_server = bastion::mcp::server::BastionMcpServer::new(
                Arc::new(agent.capability_registry.clone()),
                memory.clone(),
                personas,
                goals_for_product.clone(),
                token_perms,
                local_owner,
            );
            let (stdin, stdout) = rmcp::transport::stdio();
            tracing::info!(event = "mcp_stdio_started", "MCP stdio server starting");
            let running = mcp_server
                .serve((stdin, stdout))
                .await
                .map_err(|e| anyhow::anyhow!("MCP stdio server error: {}", e))?;
            running
                .waiting()
                .await
                .map_err(|e| anyhow::anyhow!("MCP stdio server terminated: {}", e))?;
        }
        Command::Export {
            mode,
            with_identity,
            output,
        } => {
            let owner_id = std::env::var("BASTION_OWNER_ID")
                .unwrap_or_else(|_| bastion::agent::loop_::DEFAULT_OWNER.to_string());

            let af = match mode.as_str() {
                "full" => {
                    let identity = if with_identity {
                        agent_identity.as_deref()
                    } else {
                        None
                    };
                    bastion::interop::export::export_full(
                        &memory,
                        &agent.registry,
                        &goals_for_product,
                        &cfg,
                        identity,
                        &owner_id,
                    )
                    .await?
                }
                "template" => {
                    if with_identity {
                        anyhow::bail!("--with-identity is only valid with --mode full");
                    }
                    bastion::interop::export::export_template(&agent.registry, &cfg).await?
                }
                other => {
                    anyhow::bail!("Invalid export mode '{}'. Use 'full' or 'template'.", other)
                }
            };

            let json = serde_json::to_string_pretty(&af)?;
            tokio::fs::write(&output, &json).await?;
            // WR-04: --with-identity embeds the age + Ed25519 SECRET keys in plaintext —
            // this file is the trust root for the entire mesh identity. Restrict to
            // owner-read-write before anything else touches it; the process umask alone
            // (commonly 0644) would leave it group/world-readable on a shared host.
            #[cfg(unix)]
            if with_identity {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600)).await?;
            }
            tracing::info!(event = "export_complete", mode = %mode, output = %output);
            println!("Exported agent to {output}");
        }
        Command::Import { input } => {
            let owner_id = std::env::var("BASTION_OWNER_ID")
                .unwrap_or_else(|_| bastion::agent::loop_::DEFAULT_OWNER.to_string());

            let json = match input {
                Some(path) => tokio::fs::read_to_string(&path).await?,
                None => {
                    // Read from stdin
                    let mut buf = String::new();
                    use std::io::Read;
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                }
            };
            let af: bastion::interop::AgentFile = serde_json::from_str(&json)?;
            let restored = bastion::interop::import::import(
                af,
                &memory,
                &agent.registry,
                &goals_for_product,
                &owner_id,
            )
            .await?;
            if let Some(id) = restored {
                let age_secret = id.age_secret_bech32();
                println!("Import complete. Identity restored.");
                println!("Set MESH_IDENTITY_KEY={age_secret} for mesh use.");
            } else {
                println!("Import complete (no identity in file).");
            }
        }
    }

    // SEAM #4: flush e shutdown do OTel para não perder spans buffered.
    // OTel 0.32: SdkTracerProvider::shutdown() flushes all batch processors.
    // _otel_provider is still alive (owns the processors) — explicit shutdown before drop.
    let _ = _otel_provider.shutdown();

    Ok(())
}

/// REPL daemon loop: stdin line by line, slash commands, graceful shutdown (D-01).
/// Five select arms: stdin, pending_rx (proactive), inbound_rx (channel), SIGTERM, Ctrl-C.
/// All arms serialize through ONE `&mut agent` — single-turn invariant holds (CR-07).
async fn daemon_loop(
    agent: &mut AgentLoop,
    cfg: &bastion::config::BastionConfig,
    agent_identity: Option<Arc<bastion::identity::age_identity::AgeIdentity>>,
    // SEC-03: opt-in Composio OAuth client (Some only when COMPOSIO_API_KEY is
    // configured) — wired into both the agent (/connect-app-composio) and the
    // webhook server's /auth/composio/callback route below.
    composio_oauth: Option<Arc<bastion::mcp::ComposioOAuth>>,
    // M2 (P4 `GoalPort`): concrete `GoalEngine` for the two product-level
    // consumers `daemon_loop` wires up (`BastionMcpServer`'s MCP-over-HTTP
    // resources, `CronService`'s heartbeat) that need more than the loop's
    // `list_goals`-only port surface. Same underlying engine `agent.goals`
    // wraps — cloned in `main()` before it moved into the loop.
    goals_for_product: GoalEngine,
    // M2 (P3 `ToolSource`): concrete `Arc<McpClient>` for the Reflector's
    // directly-registered `McpToolAdapter` below — the SAME connected client
    // `agent.tool_source` wraps, shared by-Arc from `main()`.
    mcp_for_product: Arc<McpClient>,
) -> anyhow::Result<()> {
    use bastion::agent::command::CommandResult;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::signal::unix::{signal, SignalKind};

    // PROACT-05: take pending_rx out of the agent so we own it in the select! loop.
    // Because select! processes ONE branch per iteration and run_turn fully awaits,
    // a pending message is only picked up BETWEEN turns — this IS the structural guarantee.
    let mut pending_rx = agent
        .pending_rx
        .take()
        .expect("pending_rx must be available at daemon start");

    // CR-07: create AgentHandle + inbound receiver BEFORE the select! loop.
    // Channels (Telegram, webhook) hold clones of `handle` and send messages into `inbound_rx`.
    // The select! arm below serializes all channel turns through the SAME agent as stdin/proactive.
    let (agent_handle, mut inbound_rx) = handle::channel();

    // CHAN-02/D-05: OwnerMaps for ALL 7 channels are now projected from the single
    // `[[identity]]` table (bastion::config::owner_map_for_*) instead of the old
    // scattered per-channel env vars (BASTION_WEBHOOK_OWNERS/BASTION_TELEGRAM_OWNERS).
    // This is the plan 10-09 deliverable that makes CHAN-02's "unified owner-based
    // routing" claim literally true — one mechanism, not N.
    let webhook_owner_map = bastion::config::owner_map_for_webhook(&cfg.identity);
    let telegram_owner_map = bastion::config::owner_map_for_telegram(&cfg.identity);
    let whatsapp_owner_map = bastion::config::owner_map_for_whatsapp(&cfg.identity);
    let discord_owner_map = bastion::config::owner_map_for_discord(&cfg.identity);
    let slack_owner_map = bastion::config::owner_map_for_slack(&cfg.identity);
    let email_owner_map = bastion::config::owner_map_for_email(&cfg.identity);

    // Spawn webhook channel if BASTION_WEBHOOK_ADDR is set.
    if let Ok(addr) = std::env::var("BASTION_WEBHOOK_ADDR") {
        let h = agent_handle.clone();
        let owner_map = webhook_owner_map;
        // Phase 6: mesh connectivity — load peers from config, create broadcast channel for SSE.
        let (events_tx, _) = tokio::sync::broadcast::channel::<String>(128);
        let peer_map_initial = bastion::config::load_mesh_peers(cfg);
        let mesh_peer_map = Arc::new(RwLock::new(peer_map_initial));
        // WR-01/CR-04: APP_JWT_SECRET must be set — no insecure fallback. This is the
        // actual `bastion daemon` startup path (serve_with_mesh performs no validation
        // of its own); only `WebhookChannel::run`, which daemon_loop never calls, had
        // the fail-closed check. Fail here instead of silently signing/verifying JWTs
        // with a well-known default that anyone reading this public repo can use to
        // impersonate any owner.
        let jwt_secret = std::env::var("APP_JWT_SECRET").map_err(|_| {
            tracing::error!(
                event = "webhook_no_jwt_secret",
                "APP_JWT_SECRET is not set — refusing to start"
            );
            anyhow::anyhow!(
                "APP_JWT_SECRET must be set; refusing to start with a hardcoded default"
            )
        })?;

        // Phase 6 Wave 2: P2PTransport + MeshSliceProvider when MESH_IDENTITY_KEY is set.
        let (mesh_transport, mesh_slice_store) =
            if let Ok(identity_key) = std::env::var("MESH_IDENTITY_KEY") {
                let local_owner = std::env::var("BASTION_OWNER_ID")
                    .unwrap_or_else(|_| bastion::agent::loop_::DEFAULT_OWNER.to_string());
                let transport = bastion::mesh::p2p::P2PTransport::new(
                    local_owner.clone(),
                    identity_key,
                    mesh_peer_map.clone(),
                    events_tx.clone(),
                );
                let shared: bastion::mesh::SharedMeshTransport = Arc::new(transport);

                // MeshSliceProvider::new returns (provider, store); add_mesh_slice_provider
                // constructs from the store — so we use from_store path via add_mesh_slice_provider.
                let (_, store) =
                    bastion::mesh::context_provider::MeshSliceProvider::new(local_owner.clone());
                agent.add_mesh_slice_provider(store.clone());

                // Periodic mesh sync (mesh.sync_interval minutes, default 15; 0 = disable)
                let sync_interval = cfg.mesh.sync_interval;
                let _mesh_sync_handle = bastion::scheduler::cron::spawn_mesh_sync_job(
                    shared.clone(),
                    mesh_peer_map.clone(),
                    agent.memory.clone(),
                    local_owner,
                    sync_interval,
                );
                tracing::info!(
                    event = "mesh_transport_enabled",
                    sync_interval_minutes = sync_interval
                );

                (Some(shared), Some(store))
            } else {
                tracing::info!(
                    event = "mesh_transport_disabled",
                    "MESH_IDENTITY_KEY not set — mesh disabled"
                );
                (None, None)
            };

        // agent_identity was already loaded above (line ~170) — reuse outer scope.
        let agent_name =
            std::env::var("BASTION_AGENT_NAME").unwrap_or_else(|_| "bastion".to_string());

        // Build MCP Streamable HTTP server if enabled.
        let mcp_routes = if cfg.mcp_server.enabled {
            // Clone AgentLoop components that BastionMcpServer needs.
            let cap_registry = Arc::new(agent.capability_registry.clone());
            let mem = agent.memory.clone();
            let personas = Arc::new(agent.registry.clone());
            let goals = goals_for_product.clone();
            let local_owner = std::env::var("BASTION_OWNER_ID")
                .unwrap_or_else(|_| bastion::agent::loop_::DEFAULT_OWNER.to_string());
            let token_perms = build_token_perms(cfg);
            // WR-06: after CR-01's fail-closed auth fix, an empty token map means the
            // server is enabled but permanently unreachable (no token can ever match) —
            // the safe direction, but still a likely operator mistake worth surfacing
            // instead of a silent "MCP server doesn't work" support report.
            if token_perms.is_empty() {
                tracing::warn!(
                    event = "mcp_server_no_tokens_configured",
                    "mcp_server.enabled=true but [mcp_server.tokens] is empty — no client can authenticate"
                );
            }
            let router = bastion::mcp::server::build_mcp_axum_router(
                cap_registry,
                mem,
                personas,
                goals,
                token_perms,
                local_owner,
                &cfg.mcp_server.mount_path,
            );
            tracing::info!(
                event = "mcp_server_enabled",
                mount_path = %cfg.mcp_server.mount_path,
            );
            Some(router)
        } else {
            tracing::info!(event = "mcp_server_disabled");
            None
        };

        // CR-02: create an OtcStore and pass it to serve_with_mesh so skill commands
        // can insert BAST-XXXX codes for /auth/exchange and /mesh/pair.
        // The same Arc is injected into the agent so the /connect-app REPL command
        // writes codes the webhook server reads (06-08 OTC-writer wiring).
        let otc_store = bastion::channel::webhook::new_otc_store();
        agent.set_otc_store(otc_store.clone());

        // SEC-03: mirrors the OTC store wiring above — inject the same ComposioOAuth
        // Arc into both the agent (for /connect-app-composio) and serve_with_mesh
        // (for the /auth/composio/callback route), only when configured.
        if let Some(oauth) = &composio_oauth {
            agent.set_composio_oauth(oauth.clone());
        }

        // WhatsApp (CHAN-01): reuses this same webhook router (10-RESEARCH.md
        // Pattern 1) — no second axum server. `WHATSAPP_PHONE_NUMBER_ID` presence
        // gates whether we attempt to build a sender at all.
        let whatsapp_config = if std::env::var("WHATSAPP_PHONE_NUMBER_ID").is_ok() {
            match bastion::channel::whatsapp::WhatsAppSender::from_env() {
                Ok(sender) => Some(bastion::channel::whatsapp::WhatsAppConfig {
                    owner_map: whatsapp_owner_map,
                    sender: std::sync::Arc::new(sender),
                }),
                Err(e) => {
                    tracing::warn!(event = "whatsapp_start_failed", error = %e);
                    None
                }
            }
        } else {
            None
        };

        tokio::spawn(async move {
            if let Err(e) = bastion::channel::webhook::serve_with_mesh(
                h,
                &addr,
                owner_map,
                events_tx,
                mesh_peer_map,
                jwt_secret,
                mesh_transport,
                mesh_slice_store,
                otc_store,
                agent_identity,
                agent_name,
                mcp_routes,
                whatsapp_config,
                composio_oauth.clone(),
            )
            .await
            {
                tracing::error!(event = "webhook_error", error = %e, "webhook channel terminated");
            }
        });
        tracing::info!(event = "webhook_started", addr = %std::env::var("BASTION_WEBHOOK_ADDR").unwrap_or_default());
    } else if std::env::var("WHATSAPP_PHONE_NUMBER_ID").is_ok() {
        tracing::warn!(
            event = "whatsapp_requires_webhook_addr",
            "WHATSAPP_PHONE_NUMBER_ID is set but BASTION_WEBHOOK_ADDR is not — WhatsApp mounts on the webhook router and cannot start without it"
        );
    }

    // Spawn Telegram channel if TELEGRAM_BOT_TOKEN is set.
    if std::env::var("TELEGRAM_BOT_TOKEN").is_ok() {
        match bastion::channel::telegram::TelegramChannel::from_env() {
            Ok(tg) => {
                let tg = tg.with_owner_map(telegram_owner_map);
                let h = agent_handle.clone();
                tokio::spawn(async move {
                    use bastion::channel::Channel;
                    if let Err(e) = Box::new(tg).run(h).await {
                        tracing::error!(event = "telegram_error", error = %e, "telegram channel terminated");
                    }
                });
                tracing::info!(event = "telegram_started");
            }
            Err(e) => {
                tracing::warn!(event = "telegram_start_failed", error = %e);
            }
        }
    }

    // Spawn Discord channel if DISCORD_BOT_TOKEN is set (CHAN-03).
    if std::env::var("DISCORD_BOT_TOKEN").is_ok() {
        match bastion::channel::discord::DiscordChannel::from_env() {
            Ok(ch) => {
                let ch = ch.with_owner_map(discord_owner_map);
                let h = agent_handle.clone();
                tokio::spawn(async move {
                    use bastion::channel::Channel;
                    if let Err(e) = Box::new(ch).run(h).await {
                        tracing::error!(event = "discord_error", error = %e, "discord channel terminated");
                    }
                });
                tracing::info!(event = "discord_started");
            }
            Err(e) => {
                tracing::warn!(event = "discord_start_failed", error = %e);
            }
        }
    }

    // Spawn Slack channel if SLACK_BOT_TOKEN and SLACK_APP_TOKEN are set (CHAN-03).
    if std::env::var("SLACK_BOT_TOKEN").is_ok() && std::env::var("SLACK_APP_TOKEN").is_ok() {
        match bastion::channel::slack::SlackChannel::from_env() {
            Ok(ch) => {
                let ch = ch.with_owner_map(slack_owner_map);
                let h = agent_handle.clone();
                tokio::spawn(async move {
                    use bastion::channel::Channel;
                    if let Err(e) = Box::new(ch).run(h).await {
                        tracing::error!(event = "slack_error", error = %e, "slack channel terminated");
                    }
                });
                tracing::info!(event = "slack_started");
            }
            Err(e) => {
                tracing::warn!(event = "slack_start_failed", error = %e);
            }
        }
    }

    // Spawn Email channel if EMAIL_ADDRESS is set (CHAN-03).
    if std::env::var("EMAIL_ADDRESS").is_ok() {
        match bastion::channel::email::EmailChannel::from_env() {
            Ok(ch) => {
                let ch = ch.with_owner_map(email_owner_map);
                let h = agent_handle.clone();
                tokio::spawn(async move {
                    use bastion::channel::Channel;
                    if let Err(e) = Box::new(ch).run(h).await {
                        tracing::error!(event = "email_error", error = %e, "email channel terminated");
                    }
                });
                tracing::info!(event = "email_started");
            }
            Err(e) => {
                tracing::warn!(event = "email_start_failed", error = %e);
            }
        }
    }

    // Spawn Voice channel if [channels.voice].enabled (VOICE-01). No secret env var to
    // gate on — voice authenticates via local mic/speaker hardware presence, not a
    // remote credential. `voice_transcribe`/`voice_speak` are already present in the
    // SAME registry AgentLoop::new() populated (auto-classified is_local_override=true
    // by Plan 10-08's [mcp.servers.voice].is_local=true wiring) — no manual
    // registration call is needed here.
    if cfg.channels.voice.enabled {
        let voice_registry = Arc::new(agent.capability_registry.clone());
        let vc = bastion::channel::voice::VoiceChannel::new(
            voice_registry,
            cfg.channels.voice.voice.clone(),
            cfg.channels.voice.wake_word_enabled,
        );
        let h = agent_handle.clone();
        tokio::spawn(async move {
            use bastion::channel::Channel;
            if let Err(e) = Box::new(vc).run(h).await {
                tracing::error!(event = "voice_error", error = %e, "voice channel terminated");
            }
        });
        tracing::info!(event = "voice_started");
    }

    // Spawn /api/infer gateway for Python MCP containers (D-08 / D-09).
    // Port: BASTION_INFER_ADDR env var, default "127.0.0.1:3000" (loopback).
    // Python containers call this endpoint; they never hold raw API keys.
    //
    // SEC (unauthenticated token-minting): this endpoint proxies inference using
    // Bastion's provider credentials, so it MUST NOT be reachable unauthenticated.
    // Defense in depth:
    //   1. Default bind is loopback; widening requires explicit BASTION_INFER_ADDR.
    //   2. BASTION_INFER_TOKEN enforces `Authorization: Bearer <token>` per request.
    //   3. Fail closed: refuse to bind a non-loopback interface without a token.
    // In Docker, the token is injected and the port stays on a private, unpublished
    // network (see plan 03-06).
    {
        let infer_token = std::env::var("BASTION_INFER_TOKEN")
            .ok()
            .filter(|t| !t.is_empty());
        let infer_addr =
            std::env::var("BASTION_INFER_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
        let host = infer_addr
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(&infer_addr);
        let is_loopback =
            host == "127.0.0.1" || host == "::1" || host == "[::1]" || host == "localhost";

        if infer_token.is_none() && !is_loopback {
            tracing::error!(
                event = "infer_gateway_refused",
                addr = %infer_addr,
                "refusing to expose /api/infer on a non-loopback interface without BASTION_INFER_TOKEN (SEC: unauthenticated token-minting)"
            );
        } else {
            if infer_token.is_none() {
                tracing::warn!(
                    event = "infer_gateway_no_auth",
                    addr = %infer_addr,
                    "/api/infer running without BASTION_INFER_TOKEN — loopback-only dev mode"
                );
            }
            let infer_router = bastion::api::infer::router(agent.provider.clone(), infer_token);
            tokio::spawn(async move {
                match tokio::net::TcpListener::bind(&infer_addr).await {
                    Ok(listener) => {
                        tracing::info!(event = "infer_gateway_started", addr = %infer_addr);
                        if let Err(e) = axum::serve(listener, infer_router).await {
                            tracing::error!(event = "infer_gateway_error", error = %e);
                        }
                    }
                    Err(e) => {
                        tracing::error!(event = "infer_gateway_bind_failed", addr = %infer_addr, error = %e);
                    }
                }
            });
        }
    }

    // Spawn CronService heartbeat into the pending queue (PROACT-01 / PROACT-02).
    // It feeds goal-drift nudges into pending_tx. On tick the daemon will pick them up
    // between turns via the pending_rx arm.
    {
        let cron = CronService::new(agent.pending_tx.clone(), goals_for_product.clone());
        let owner = bastion::agent::loop_::DEFAULT_OWNER.to_string();
        // Only spawn heartbeat if there are goals to nudge (fire-and-forget task)
        tokio::spawn(async move {
            cron.run_heartbeat(std::time::Duration::from_secs(86_400), &owner)
                .await;
        });
    }

    // LEARN-02/LEARN-05: spawn the offline Reflector. Budget/interval/model/dedup-cadence
    // come from bastion.toml [reflector] (defaults if absent). Never reachable from a
    // user-facing turn (ADR D-4) — this is a separate tokio::spawn, same idiom as
    // CronService::run_heartbeat and spawn_mesh_sync_job above.
    {
        // Minimal registry scoped to exactly what the Reflector's dedup leg needs
        // (memupalace's memory_embed tool) — avoids refactoring AgentLoop.capability_registry's
        // field type just to share it across a separately-spawned task.
        let mut reflector_registry = bastion::capability::CapabilityRegistry::new();
        if let Err(e) = reflector_registry.register(Arc::new(
            bastion::capability::adapters::McpToolAdapter {
                tool_name: "memory_embed".to_string(),
                server_label: "memupalace".to_string(),
                description: "Return the embedding vector for a text (dedup similarity)"
                    .to_string(),
                schema: serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}),
                mcp: mcp_for_product.clone(),
                // memupalace's memory_embed is NOT local (Plan 10-08) — preserves
                // today's exact behavior unchanged.
                is_local_override: false,
                // memory_embed is a read-only embedding lookup, not destructive —
                // and this minimal reflector registry has no ApprovalQueue wired
                // anyway, so needs_approval:true here would fail-closed-deny it
                // outright (Plan 11-04).
                needs_approval_override: false,
                trusted_override: false,
            },
        )) {
            tracing::warn!(event = "reflector_registry_register_failed", error = %e);
        }

        // LEARN-05 gap fix: an explicit [reflector].model must actually select the
        // Reflector's provider, not just be threaded through inertly. Unset/empty falls
        // back to the exact same default-agent provider instance (safe pre-fix behavior).
        let reflector_provider = bastion::provider::registry::resolve_reflector_provider(
            cfg.reflector.model.as_deref(),
            &cfg.agent.default_model,
            agent.provider.clone(),
        )?;

        let generator: Arc<dyn bastion::learn::CandidateGenerator> =
            Arc::new(bastion::learn::LlmCandidateGenerator::new(
                reflector_provider,
                cfg.reflector.model.clone(),
                cfg.reflector.allow_cloud,
            ));

        let reflector = bastion::learn::Reflector::new(
            agent.memory.clone(),
            generator,
            Arc::new(reflector_registry),
            cfg.reflector.clone(),
            cfg.session.db_path.clone(),
            cfg.logging.log_path.clone(),
        );
        let owner = bastion::agent::loop_::DEFAULT_OWNER.to_string();
        let interval_hours = cfg.reflector.interval_hours;
        tokio::spawn(async move {
            reflector.run(&owner).await;
        });
        tracing::info!(event = "reflector_scheduled", interval_hours);
    }

    // CONC-1: session mutex per owner — serializes turns from the same owner so a
    // double-tap (two Telegram messages in quick succession) never starts a concurrent
    // turn for that owner. Different owners are NOT blocked by each other.
    // Arc<Mutex<()>> is cheap: lock body is just the run_turn_for call.
    // HashMap grows per unique owner but never shrinks — acceptable for personal use
    // with a small fixed set of owners (T-05-02-03 accepted risk).
    let mut session_locks: HashMap<String, Arc<Mutex<()>>> = HashMap::new();

    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    // In a detached container (`docker compose up -d`) stdin is closed and returns EOF
    // immediately. The daemon must keep running to serve channels (Telegram), so we track
    // whether stdin is still live and disable that select arm on EOF instead of exiting.
    let mut stdin_open = true;
    let mut sigterm = signal(SignalKind::terminate())?;

    println!("Bastion daemon started. Type a message or /help for commands.");

    loop {
        tokio::select! {
            line = stdin.next_line(), if stdin_open => {
                match line? {
                    None => {
                        tracing::info!(event = "stdin_eof");
                        // Non-interactive / detached: stop polling the (now-dead) stdin arm but
                        // keep serving channels, proactive nudges, and signals. The daemon exits
                        // only on SIGTERM/Ctrl-C — NOT on stdin EOF (D-01 long-running invariant).
                        stdin_open = false;
                    }
                    Some(s) if s.trim().is_empty() => continue,
                    Some(s) if s.trim().starts_with('/') => {
                        match agent
                            .handle_command(s.trim(), bastion::agent::loop_::DEFAULT_OWNER)
                            .await?
                        {
                            CommandResult::Stop => break,
                            CommandResult::Handled(msg) => println!("{msg}"),
                            CommandResult::Unknown(cmd) => {
                                println!("Unknown command: {}. Type /help.", cmd);
                            }
                        }
                    }
                    Some(s) => {
                        match agent.run_turn(&s).await {
                            Ok(response) => {
                                println!("{}", response);
                            }
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                tracing::error!(event = "turn_error", error = %e);
                            }
                        }
                    }
                }
            }
            // PROACT-05: proactive messages delivered ONLY between turns.
            Some(msg) = pending_rx.recv() => {
                tracing::info!(event = "proactive_turn", msg_len = msg.len());
                match agent.run_turn(&msg).await {
                    Ok(r) => println!("{r}"),
                    Err(e) => tracing::error!(event = "proactive_turn_error", error = %e),
                }
            }
            // CR-07: channel inbound arm — serializes Telegram/webhook turns through the SAME
            // agent as stdin/proactive. The trusted owner was resolved by the channel layer.
            // Typed Result propagated back through the oneshot (WR-10).
            //
            // WEB-CMD-01: slash commands reuse the same router the stdin console uses
            // (agent.handle_command), but ALLOWLISTED, not blocklisted — commands are
            // console-only by default. `provider` and `forced_persona` are single fields
            // shared by the whole daemon (not per-owner), so /model and /as let one remote
            // owner affect every other owner's turns; /logs exposes daemon-wide (not
            // owner-scoped) WARN/ERROR entries; /connect-app mints a JWT whose `sub` is the
            // caller-chosen device name verbatim — remotely reachable, that's an
            // authentication bypass (mint a code naming ANY owner, then impersonate them).
            // Only /help (stateless) and /contest (owner-scoped, see command.rs) are safe
            // for a remote channel caller today. Extend this list only after confirming a
            // new command is properly owner-scoped — do not default new commands to open.
            Some(req) = inbound_rx.recv() => {
                const REMOTE_ALLOWED_COMMANDS: &[&str] = &["/help", "/contest"];
                // CONC-1: acquire per-owner lock before processing turn.
                // Two turns from the same owner cannot run concurrently (double-tap protection).
                // Different owners are independent — their locks do not contend.
                let lock = session_locks
                    .entry(req.owner.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone();
                let _guard = lock.lock().await;
                let trimmed = req.text.trim();
                let command_token = trimmed.split_whitespace().next().filter(|s| s.starts_with('/'));
                // A token can look like a command but not be one (e.g. a Claude-Code-style
                // `/usage` typed out of habit, or a plain typo) — only KNOWN_COMMANDS get the
                // "console-only" verdict; anything else falls through to handle_command's own
                // Unknown-command message, exactly matching what the console would say.
                let is_known_command = command_token
                    .is_some_and(|c| bastion::agent::command::KNOWN_COMMANDS.contains(&c));
                let res = if let Some(cmd) =
                    command_token.filter(|c| is_known_command && !REMOTE_ALLOWED_COMMANDS.contains(c))
                {
                    Ok(format!("{cmd} is console-only — not allowed remotely."))
                } else if command_token.is_some() {
                    match agent.handle_command(trimmed, &req.owner).await {
                        Ok(CommandResult::Handled(msg)) => Ok(msg),
                        Ok(CommandResult::Unknown(cmd)) => {
                            Ok(format!("Unknown command: {cmd}. Type /help."))
                        }
                        Ok(CommandResult::Stop) => {
                            Ok("/stop is console-only — not allowed remotely.".to_string())
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    // SEC-05: threads the channel-resolved trust classification
                    // (email always untrusted; public-channel Discord/Slack
                    // untrusted; DMs and every other pre-existing channel
                    // trusted) into the quarantine-aware turn entry point.
                    agent
                        .run_turn_for_with_trust(&req.text, &req.owner, req.untrusted)
                        .await
                };
                if let Err(ref e) = res {
                    tracing::warn!(
                        event = "channel_turn_error",
                        owner = %req.owner,
                        error = %e,
                        "channel turn failed"
                    );
                }
                let _ = req.reply.send(res);
            }
            _ = sigterm.recv() => {
                tracing::info!(event = "sigterm_received");
                println!("Shutting down (SIGTERM).");
                break;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!(event = "ctrl_c_received");
                println!("\nShutting down (Ctrl-C).");
                break;
            }
        }
    }
    Ok(())
}

/// Build token permissions map from config (shared between daemon and mcp-stdio paths).
fn build_token_perms(
    cfg: &bastion::config::BastionConfig,
) -> HashMap<String, bastion::mcp::server::TokenPermissions> {
    cfg.mcp_server
        .tokens
        .iter()
        .map(|(token, t)| {
            (
                token.clone(),
                bastion::mcp::server::TokenPermissions {
                    read_only: t.read_only,
                    owner_id: t.owner_id.clone(),
                    privacy_tier: if t.cloud_ok {
                        bastion::memory::PrivacyTier::CloudOk
                    } else {
                        bastion::memory::PrivacyTier::LocalOnly
                    },
                },
            )
        })
        .collect()
}
