//! Bastion configuration — single source of truth for non-secret config.
//!
//! Layering strategy (D-09):
//!   bastion.toml (defaults) → BASTION__* env vars (overrides)
//!
//! Secrets (API keys, tokens) NEVER appear in bastion.toml — they come from .env only.

use serde::Deserialize;
use std::collections::HashMap;

/// Single [[mesh.peer]] entry from bastion.toml.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct MeshPeerConfig {
    pub owner_id:   String,
    pub peer_url:   String,
    pub age_pubkey: String,
}

/// Config section for mesh settings.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MeshConfig {
    #[serde(default)]
    pub peer: Vec<MeshPeerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BastionConfig {
    pub agent:    AgentConfig,
    pub session:  SessionConfig,
    pub logging:  LoggingConfig,
    pub mcp:      McpConfig,
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub mesh:     MeshConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub default_model:    String,
    pub daily_budget_usd: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SessionConfig {
    pub db_path:               String,
    pub autocompact_threshold: f64,
    pub keep_last_n:           u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub log_path: String,
    pub level:    String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct McpConfig {
    pub tool_call_timeout_secs: u64,
    #[serde(default)]
    pub servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct McpServerEntry {
    pub url:   String,
    pub label: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChannelsConfig {
    pub telegram: ChannelConfig,
    pub webhook:  ChannelConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChannelConfig {
    pub enabled: bool,
}

/// Load [[mesh.peer]] entries from bastion.toml into a MeshPeerMap.
/// Called once at daemon startup. Errors are logged but do not abort startup
/// (daemon runs without mesh peers if none configured).
pub fn load_mesh_peers(config: &BastionConfig) -> crate::mesh::MeshPeerMap {
    let mut map = crate::mesh::MeshPeerMap::new();
    for entry in &config.mesh.peer {
        map.register(entry.owner_id.clone(), crate::mesh::MeshPeer {
            peer_url:   entry.peer_url.clone(),
            age_pubkey: entry.age_pubkey.clone(),
        });
        tracing::info!(
            event    = "mesh_peer_loaded",
            owner_id = %entry.owner_id,
            peer_url = %entry.peer_url,
        );
    }
    map
}

/// Append a new [[mesh.peer]] entry to bastion.toml.
/// Called by /mesh/pair handler after successful pairing.
pub async fn append_mesh_peer(owner_id: &str, peer_url: &str, age_pubkey: &str) -> anyhow::Result<()> {
    let path = std::env::var("BASTION_CONFIG").unwrap_or_else(|_| "bastion.toml".to_string());
    let current = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let entry = format!(
        "\n[[mesh.peer]]\nowner_id = \"{}\"\npeer_url = \"{}\"\nage_pubkey = \"{}\"\n",
        owner_id, peer_url, age_pubkey
    );
    tokio::fs::write(&path, format!("{}{}", current, entry)).await?;
    Ok(())
}

/// Load BastionConfig from a TOML file, with env var overrides.
///
/// Env var naming convention (config-rs separator "__"):
///   BASTION__AGENT__DEFAULT_MODEL=claude-opus-4-7
///   BASTION__SESSION__DB_PATH=/data/sessions.db
pub fn load_config(path: &str) -> anyhow::Result<BastionConfig> {
    let cfg = config::Config::builder()
        .add_source(config::File::with_name(path))
        .add_source(
            config::Environment::with_prefix("BASTION")
                .separator("__")
        )
        .build()?;
    Ok(cfg.try_deserialize()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_config_from_bastion_toml() {
        let cfg = load_config("bastion.toml").expect("bastion.toml must exist at repo root");
        // default_model is deployment-specific (Mario runs OpenRouter free); assert it's set,
        // not a specific value — this test verifies config parsing, not the chosen model.
        assert!(!cfg.agent.default_model.is_empty(), "default_model must be set in bastion.toml");
        assert!(cfg.agent.daily_budget_usd > 0.0);
        assert!(cfg.mcp.servers.contains_key("memupalace"));
        assert_eq!(cfg.mcp.servers["memupalace"].url, "http://memupalace:8001/mcp");
    }
}
