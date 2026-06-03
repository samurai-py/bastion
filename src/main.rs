use std::sync::Arc;
use tokio::sync::RwLock;
use clap::{Parser, Subcommand};
use tracing_subscriber::fmt;

use bastion::agent::loop_::AgentLoop;
use bastion::mcp::McpClient;
use bastion::memory::sqlite::SqliteMemory;
use bastion::provider::registry::resolve_provider;
use bastion::session::SessionManager;
use bastion::persona::PersonaRegistry;
use bastion::goal::{GoalEngine, ScoringConfig};
use bastion::proactive::CronService;

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

    // Init structured JSON logging to .bastion/bastion.log
    std::fs::create_dir_all(".bastion")?;
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var("BASTION_LOG").unwrap_or_else(|_| ".bastion/bastion.log".into()))?;

    fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(log_file)
        .init();

    let cli = Cli::parse();

    // Init SessionManager
    let db_path = std::env::var("BASTION_DB").unwrap_or_else(|_| ".bastion/sessions.db".into());
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
    let mcp_client = McpClient::connect_all(".bastion/mcp-servers.json").await?;

    // Init provider from env var or default model
    let default_model = std::env::var("BASTION_DEFAULT_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-5".to_owned());
    let provider: bastion::provider::SharedProvider =
        Arc::new(RwLock::new(resolve_provider(&default_model)?));

    let daily_budget = std::env::var("DAILY_BUDGET_USD")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(5.0);

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
/// Fourth select arm drains pending_rx — proactive messages delivered ONLY between turns (PROACT-05).
async fn daemon_loop(agent: &mut AgentLoop) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::signal::unix::{signal, SignalKind};
    use bastion::agent::command::CommandResult;

    // PROACT-05: take pending_rx out of the agent so we own it in the select! loop.
    // Because select! processes ONE branch per iteration and run_turn fully awaits,
    // a pending message is only picked up BETWEEN turns — this IS the structural guarantee.
    let mut pending_rx = agent.pending_rx.take().expect("pending_rx must be available at daemon start");

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
            // PROACT-05: 4th select arm — proactive messages delivered ONLY between turns.
            // select! processes ONE branch per iteration; run_turn is fully awaited in stdin arm,
            // so a pending message is never picked up mid-turn. This is the structural guarantee.
            Some(msg) = pending_rx.recv() => {
                tracing::info!(event = "proactive_turn", msg_len = msg.len());
                match agent.run_turn(&msg).await {
                    Ok(r) => println!("{r}"),
                    Err(e) => tracing::error!(event = "proactive_turn_error", error = %e),
                }
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
