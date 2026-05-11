use crate::provider::SharedProvider;
use crate::provider::registry::resolve_provider;

pub enum CommandResult {
    Handled,
    Stop,
    Unknown(String),
}

/// Route slash commands from stdin. Acquires write lock on provider for /model.
/// WRITE lock is safe here — called only between turns (never mid-stream).
pub async fn handle_command(
    input: &str,
    provider: &SharedProvider,
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
        "/help" => {
            println!("Available commands:");
            println!("  /model <name>     Switch LLM provider+model (e.g. /model claude-opus-4-7)");
            println!("  /stop             Shut down daemon");
            println!("  /as <persona>     Force persona (Phase 2+, not yet implemented)");
            println!("  /help             Show this help");
            Ok(CommandResult::Handled)
        }
        "/as" => {
            println!("Persona routing not yet implemented (Phase 2+).");
            Ok(CommandResult::Handled)
        }
        _ => Ok(CommandResult::Unknown(trimmed.to_owned())),
    }
}
