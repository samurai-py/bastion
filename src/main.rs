use std::sync::Arc;
use tokio::sync::RwLock;
use clap::{Parser, Subcommand};
use tracing_subscriber::fmt;

use bastion::agent::loop_::AgentLoop;
use bastion::mcp::McpClient;
use bastion::provider::registry::resolve_provider;
use bastion::session::SessionManager;

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

    let mut agent = AgentLoop::new(provider, session, mcp_client, session_id, daily_budget);

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
async fn daemon_loop(agent: &mut AgentLoop) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::signal::unix::{signal, SignalKind};
    use bastion::agent::command::CommandResult;

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
