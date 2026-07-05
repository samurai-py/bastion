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
    pub owner_id: String,
    pub peer_url: String,
    pub age_pubkey: String,
    /// Tags this peer is allowed to receive (filter_for_mesh allowlist).
    /// Default: empty (no beliefs shared). Example: ["mercado", "calendario"].
    #[serde(default)]
    pub allowed_tags: Vec<String>,
}

/// Config section for mesh settings.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MeshConfig {
    #[serde(default)]
    pub peer: Vec<MeshPeerConfig>,
    /// Interval in minutes between automatic mesh syncs (0 = disable periodic sync, manual /mesh-sync only).
    /// Default: 15.
    #[serde(default = "default_sync_interval")]
    pub sync_interval: u64,
}

fn default_sync_interval() -> u64 {
    15
}

/// Config section for the offline Reflector (LEARN-02/LEARN-05).
#[derive(Debug, Clone, Deserialize)]
pub struct ReflectorConfig {
    /// Hard cost cap per Reflector tick (ADR D-4 "budget duro"). Default: $0.10.
    #[serde(default = "default_reflector_budget_usd")]
    pub budget_usd: f64,
    /// Hours between offline Reflector runs. 0 = disabled (no periodic run). Default: 24.
    #[serde(default = "default_reflector_interval_hours")]
    pub interval_hours: u64,
    /// Cheap/local model id for reflection. None = fall back to `[agent].default_model`
    /// (never silently default to a fixed paid tier — RESEARCH Assumption A5).
    pub model: Option<String>,
    /// Run semantic dedup every N accepted deltas. Default: 10.
    #[serde(default = "default_dedup_every_n")]
    pub dedup_every_n: u32,
    /// Opt-in: allow the Reflector's LLM candidate generation to send the raw daemon
    /// log tail to a NON-local (cloud) provider. Default: false (deny-on-ambiguity —
    /// the log tail is treated as LocalOnly, so a cloud Reflector provider is refused
    /// by the egress chokepoint). Set true ONLY after accepting that log content
    /// (which may contain LocalOnly context) leaves the node to the configured cloud model.
    #[serde(default)]
    pub allow_cloud: bool,
}

impl Default for ReflectorConfig {
    fn default() -> Self {
        Self {
            budget_usd: default_reflector_budget_usd(),
            interval_hours: default_reflector_interval_hours(),
            model: None,
            dedup_every_n: default_dedup_every_n(),
            allow_cloud: false,
        }
    }
}

fn default_reflector_budget_usd() -> f64 {
    0.10
}
fn default_reflector_interval_hours() -> u64 {
    24
}
fn default_dedup_every_n() -> u32 {
    10
}

#[derive(Debug, Deserialize, Clone)]
pub struct BastionConfig {
    pub agent: AgentConfig,
    pub session: SessionConfig,
    pub logging: LoggingConfig,
    pub mcp: McpConfig,
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub mesh: MeshConfig,
    #[serde(default)]
    pub reflector: ReflectorConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub default_model: String,
    pub daily_budget_usd: f64,
    /// D-11: ordered list of model-name strings, using the same naming convention
    /// `resolve_provider()` (src/provider/registry.rs) already accepts (e.g.
    /// `"groq/llama-3.1-8b-instant"`, `"gemini-2.0-flash"`). Tried in order when the
    /// primary provider suffers a hard/persistent failure (SO-03/D-10 rung 3, wired
    /// in Plan 08-08). Empty = no provider-switching (today's exact behavior).
    #[serde(default)]
    pub fallback_models: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SessionConfig {
    pub db_path: String,
    pub autocompact_threshold: f64,
    pub keep_last_n: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub log_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct McpConfig {
    pub tool_call_timeout_secs: u64,
    #[serde(default)]
    pub servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct McpServerEntry {
    pub url: String,
    pub label: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChannelsConfig {
    pub telegram: ChannelConfig,
    pub webhook: ChannelConfig,
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
        map.register(
            entry.owner_id.clone(),
            crate::mesh::MeshPeer {
                peer_url: entry.peer_url.clone(),
                age_pubkey: entry.age_pubkey.clone(),
                allowed_tags: entry.allowed_tags.clone(),
            },
        );
        tracing::info!(
            event    = "mesh_peer_loaded",
            owner_id = %entry.owner_id,
            peer_url = %entry.peer_url,
        );
    }
    map
}

/// Validate age public key format. Must match ^age1[0-9a-z]+$ (bech32 age key).
///
/// SEC-01: called before any config write to prevent injection via malformed key strings.
fn validate_age_pubkey(key: &str) -> anyhow::Result<()> {
    // Static regex — compile once. age keys are lowercase bech32: age1 + [0-9a-z]+
    let re = regex::Regex::new(r"^age1[0-9a-z]+$").expect("static regex must compile");
    if !re.is_match(key) {
        anyhow::bail!("invalid age_pubkey format — must match ^age1[0-9a-z]+$ (SEC-01)");
    }
    Ok(())
}

/// Append a new [[mesh.peer]] entry to bastion.toml using toml_edit.
///
/// SEC-01: uses toml_edit (programmatic table construction, no string interpolation).
///         age_pubkey validated against ^age1[0-9a-z]+$ before touching the file.
/// WR-02: bails on read error instead of overwriting config with empty + new entry.
///        Existing entries (including allowed_tags) are preserved via toml_edit parse/append.
///        Atomic write via temp-file + rename prevents partial write corruption.
pub async fn append_mesh_peer(
    owner_id: &str,
    peer_url: &str,
    age_pubkey: &str,
    allowed_tags: &[String],
) -> anyhow::Result<()> {
    use toml_edit::{value, DocumentMut};

    // SEC-01: validate age_pubkey format before touching the file
    validate_age_pubkey(age_pubkey)?;

    let path = std::env::var("BASTION_CONFIG").unwrap_or_else(|_| "bastion.toml".to_string());

    // WR-02: bail on read error — do NOT fall back to empty string.
    // Falling back to "" would overwrite the entire config with just the new peer entry.
    let current = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| anyhow::anyhow!("failed to read '{}' before appending peer: {}", path, e))?;

    // Parse as mutable TOML document (toml_edit preserves comments and formatting)
    let mut doc: DocumentMut = current
        .parse()
        .map_err(|e| anyhow::anyhow!("failed to parse '{}' as TOML: {}", path, e))?;

    // Build the new [[mesh.peer]] entry as a toml_edit Table
    let mut peer_entry = toml_edit::Table::new();
    peer_entry["owner_id"] = value(owner_id);
    peer_entry["peer_url"] = value(peer_url);
    peer_entry["age_pubkey"] = value(age_pubkey);
    if !allowed_tags.is_empty() {
        let mut tags_array = toml_edit::Array::new();
        for t in allowed_tags {
            tags_array.push(t.as_str());
        }
        peer_entry["allowed_tags"] = toml_edit::Item::Value(toml_edit::Value::Array(tags_array));
    }

    // Ensure doc["mesh"] exists as a table
    if !doc.contains_key("mesh") {
        doc["mesh"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    // Append to [[mesh.peer]] array-of-tables
    match doc["mesh"]["peer"].as_array_of_tables_mut() {
        Some(arr) => {
            arr.push(peer_entry);
        }
        None => {
            // [[mesh.peer]] key doesn't exist yet — create it
            let mut aot = toml_edit::ArrayOfTables::new();
            aot.push(peer_entry);
            doc["mesh"]["peer"] = toml_edit::Item::ArrayOfTables(aot);
        }
    }

    // Atomic write: write to .tmp then rename to prevent partial write corruption
    let tmp_path = format!("{}.tmp", path);
    tokio::fs::write(&tmp_path, doc.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to write tmp config '{}': {}", tmp_path, e))?;
    tokio::fs::rename(&tmp_path, &path)
        .await
        .map_err(|e| anyhow::anyhow!("failed to rename '{}' → '{}': {}", tmp_path, path, e))?;

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
        .add_source(config::Environment::with_prefix("BASTION").separator("__"))
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
        assert!(
            !cfg.agent.default_model.is_empty(),
            "default_model must be set in bastion.toml"
        );
        assert!(cfg.agent.daily_budget_usd > 0.0);
        assert!(cfg.mcp.servers.contains_key("memupalace"));
        assert_eq!(
            cfg.mcp.servers["memupalace"].url,
            "http://memupalace:8001/mcp"
        );
    }

    // ── SEC-01 age_pubkey validation tests ───────────────────────────────────

    /// SEC-01: append_mesh_peer must reject age_pubkey not matching ^age1[0-9a-z]+$
    #[tokio::test]
    async fn test_append_mesh_peer_rejects_invalid_age_pubkey() {
        let result = append_mesh_peer(
            "owner1",
            "https://peer.example.com",
            "not-an-age-key", // does not match ^age1[0-9a-z]+$
            &[],
        )
        .await;
        assert!(result.is_err(), "must reject invalid age_pubkey");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("age_pubkey") || msg.contains("SEC-01"),
            "error must reference age_pubkey: {msg}",
        );
    }

    /// SEC-01: TOML-breaking characters in age_pubkey must be caught by regex before touching file.
    #[tokio::test]
    async fn test_append_mesh_peer_rejects_toml_injection_in_age_pubkey() {
        // injection attempt via TOML-breaking characters (quotes, newlines)
        let result = append_mesh_peer(
            "owner1",
            "https://peer.example.com",
            "age1abcdef\"\nmalicious_key = true\nage1", // injection payload
            &[],
        )
        .await;
        assert!(
            result.is_err(),
            "must reject age_pubkey with TOML-breaking characters"
        );
    }

    /// SEC-01: valid age_pubkey passes regex (does not write to file — bails on missing config).
    /// This confirms the regex itself is not overly restrictive.
    #[tokio::test]
    async fn test_validate_age_pubkey_accepts_valid_key() {
        // validate_age_pubkey only — no filesystem I/O
        let result =
            validate_age_pubkey("age1ql3z7hjy54pw3yywmz2fxnftqqhrlrr2e9xsmrwckkl2u5dc3kzqsrcq7t");
        assert!(result.is_ok(), "valid age pubkey must pass validation");
    }

    /// SEC-01: age_pubkey with uppercase must be rejected (bech32 is lowercase only).
    #[tokio::test]
    async fn test_validate_age_pubkey_rejects_uppercase() {
        let result = validate_age_pubkey("AGE1UPPERCASE");
        assert!(result.is_err(), "uppercase age_pubkey must be rejected");
    }
}
