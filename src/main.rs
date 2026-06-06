use std::sync::Arc;
use tokio::sync::RwLock;
use clap::{Parser, Subcommand};
use tracing_subscriber::fmt;

use bastion::agent::loop_::AgentLoop;
use bastion::agent::handle;
use bastion::channel::OwnerMap;
use bastion::mcp::McpClient;
use bastion::memory::sqlite::SqliteMemory;
use bastion::provider::registry::resolve_provider;
use bastion::session::SessionManager;
use bastion::persona::PersonaRegistry;
use bastion::goal::{GoalEngine, ScoringConfig};
use bastion::proactive::CronService;
use axum;

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
            .unwrap_or_else(|| std::path::Path::new(".bastion"))
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

    // Init MCP client — connect_all handles missing/failed servers gracefully:
    // logs tracing::warn per failed server and continues; missing config returns Ok(empty registry).
    // bastion.toml [mcp.servers.*] replaces .bastion/mcp-servers.json (D-09).
    let mcp_client = McpClient::connect_all(".bastion/mcp-servers.json").await?;

    // Init provider from config (default_model from bastion.toml, overridable via env BASTION__AGENT__DEFAULT_MODEL)
    let default_model = cfg.agent.default_model.clone();
    let provider: bastion::provider::SharedProvider =
        Arc::new(RwLock::new(resolve_provider(&default_model)?));

    let daily_budget = cfg.agent.daily_budget_usd;

    // Init persona registry (load from "./personas/" directory; empty if missing — PERS-07)
    let registry = PersonaRegistry::load_dir(".").await?;

    // Init shared memory
    let memory: bastion::memory::SharedMemory = Arc::new(RwLock::new(
        Box::new(SqliteMemory::new(&db_path)) as Box<dyn bastion::memory::Memory>
    ));

    // Init goal engine
    let goals = GoalEngine::new(&db_path, ScoringConfig::default());

    let mut agent = AgentLoop::new(
        provider.clone(),
        session,
        mcp_client,
        session_id,
        daily_budget,
        registry,
        memory.clone(),
        goals,
    );

    match cli.command {
        Command::Agent { message } => {
            let response = agent.run_turn(&message).await?;
            println!("{}", response);
        }
        Command::Daemon => {
            daemon_loop(&mut agent).await?;
        }
    }

    Ok(())
}

/// REPL daemon loop: stdin line by line, slash commands, graceful shutdown (D-01).
/// Five select arms: stdin, pending_rx (proactive), inbound_rx (channel), SIGTERM, Ctrl-C.
/// All arms serialize through ONE `&mut agent` — single-turn invariant holds (CR-07).
async fn daemon_loop(agent: &mut AgentLoop) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::signal::unix::{signal, SignalKind};
    use bastion::agent::command::CommandResult;

    // PROACT-05: take pending_rx out of the agent so we own it in the select! loop.
    // Because select! processes ONE branch per iteration and run_turn fully awaits,
    // a pending message is only picked up BETWEEN turns — this IS the structural guarantee.
    let mut pending_rx = agent.pending_rx.take().expect("pending_rx must be available at daemon start");

    // CR-07: create AgentHandle + inbound receiver BEFORE the select! loop.
    // Channels (Telegram, webhook) hold clones of `handle` and send messages into `inbound_rx`.
    // The select! arm below serializes all channel turns through the SAME agent as stdin/proactive.
    let (agent_handle, mut inbound_rx) = handle::channel();

    // Build OwnerMap from environment (mirrors how the provider is built above).
    // Format: BASTION_WEBHOOK_OWNERS=token1:owner1,token2:owner2
    //         BASTION_TELEGRAM_OWNERS=chat_id1:owner1,chat_id2:owner2
    let webhook_owner_map = parse_owner_map_env("BASTION_WEBHOOK_OWNERS");
    let telegram_owner_map = parse_owner_map_env("BASTION_TELEGRAM_OWNERS");

    // Spawn webhook channel if BASTION_WEBHOOK_ADDR is set.
    if let Ok(addr) = std::env::var("BASTION_WEBHOOK_ADDR") {
        let h = agent_handle.clone();
        let owner_map = webhook_owner_map;
        tokio::spawn(async move {
            if let Err(e) = bastion::channel::webhook::serve(h, &addr, owner_map).await {
                tracing::error!(event = "webhook_error", error = %e, "webhook channel terminated");
            }
        });
        tracing::info!(event = "webhook_started", addr = %std::env::var("BASTION_WEBHOOK_ADDR").unwrap_or_default());
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
        let host = infer_addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(&infer_addr);
        let is_loopback = host == "127.0.0.1" || host == "::1" || host == "[::1]" || host == "localhost";

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
            let infer_router =
                bastion::api::infer::router(agent.provider.clone(), infer_token);
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
        let cron = CronService::new(agent.pending_tx.clone(), agent.goals.clone());
        let owner = bastion::agent::loop_::DEFAULT_OWNER.to_string();
        // Only spawn heartbeat if there are goals to nudge (fire-and-forget task)
        tokio::spawn(async move {
            cron.run_heartbeat(std::time::Duration::from_secs(86_400), &owner).await;
        });
    }

    let mut stdin   = BufReader::new(tokio::io::stdin()).lines();
    let mut sigterm = signal(SignalKind::terminate())?;

    println!("Bastion daemon started. Type a message or /help for commands.");

    loop {
        tokio::select! {
            line = stdin.next_line() => {
                match line? {
                    None => {
                        tracing::info!(event = "stdin_eof");
                        break;
                    }
                    Some(s) if s.trim().is_empty() => continue,
                    Some(s) if s.trim().starts_with('/') => {
                        match agent.handle_command(s.trim()).await? {
                            CommandResult::Stop    => break,
                            CommandResult::Handled => {}
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
            Some(req) = inbound_rx.recv() => {
                let res = agent.run_turn_for(&req.text, &req.owner).await;
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

/// Parse a `KEY=val1:owner1,val2:owner2` env var into an [`OwnerMap`].
/// Returns an empty map if the variable is absent or empty.
/// Mirrors the CSV-pair format used by other Bastion env config (e.g. MCP servers).
fn parse_owner_map_env(var: &str) -> OwnerMap {
    let raw = match std::env::var(var) {
        Ok(v) if !v.is_empty() => v,
        _ => return OwnerMap::default(),
    };
    let pairs: Vec<(&str, &str)> = raw
        .split(',')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, ':');
            let key = parts.next()?.trim();
            let val = parts.next()?.trim();
            if key.is_empty() || val.is_empty() {
                // IN-08: log malformed pairs instead of silently dropping them.
                tracing::warn!(event = "owner_map_malformed_pair", pair = pair.trim());
                return None;
            }
            Some((key, val))
        })
        .collect();
    OwnerMap::from_pairs(&pairs)
}
