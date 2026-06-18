// Memory trait, PrivacyTier, Belief, SharedMemory alias.
// SqliteMemory backend is in sqlite.rs.
// Tests (offline, temp DB) are in sqlite.rs #[cfg(test)].

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Privacy tier consumed by persona/soul.rs (plan 03) and hooks/egress.rs (plan 04).
/// Defined here once; exported at crate root via `pub mod memory`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrivacyTier {
    LocalOnly,
    CloudOk,
}

/// A retrieved belief (read-only view of the beliefs table row).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Belief {
    pub id: i64,
    pub owner_id: String,
    pub persona_tag: Option<String>,
    pub content: String,
    pub weight: f64,
    pub is_core: bool,
    /// Privacy tier — None if column absent or unset in DB (treated as LocalOnly by egress gate).
    pub tier: Option<PrivacyTier>,
}

/// Core memory abstraction. Every subsystem reads/writes beliefs through this trait.
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a belief and one provenance row; returns the new belief id.
    async fn store_belief(
        &self,
        owner_id: &str,
        persona_tag: Option<&str>,
        content: &str,
        session_id: &str,
        source: &str,
        is_core: bool,
        tier: Option<PrivacyTier>,
    ) -> anyhow::Result<i64>;

    /// Retrieve non-revoked beliefs for (owner, persona_tag).
    /// WHERE owner_id=? AND (persona_tag=? OR persona_tag IS NULL) AND revoked=0 AND weight>0
    async fn retrieve_tagged(
        &self,
        owner_id: &str,
        persona_tag: Option<&str>,
    ) -> anyhow::Result<Vec<Belief>>;

    /// Soft-revoke: set weight=0, revoked=1, revoked_at=now. Row is NEVER deleted (D-15).
    /// Owner-scoped (IDOR guard): only the owning user's belief may be revoked.
    /// Errors when no row matches (id, owner_id) so a wrong owner cannot silently no-op.
    async fn revoke_belief(&self, owner_id: &str, id: i64) -> anyhow::Result<()>;

    /// Load frozen-core beliefs (is_core=1, revoked=0) once at session start.
    async fn load_core(&self, owner_id: &str) -> anyhow::Result<Vec<Belief>>;

    /// Return (session_id, source) provenance rows for a belief.
    /// Owner-scoped (IDOR guard): provenance is only returned when the belief is
    /// owned by `owner_id`; cross-owner probes get an empty vec (indistinguishable
    /// from a missing id).
    async fn provenance_for(
        &self,
        owner_id: &str,
        belief_id: i64,
    ) -> anyhow::Result<Vec<(String, String)>>;
}

/// Clonable shared-handle alias — mirrors SharedProvider from provider/mod.rs.
pub type SharedMemory = Arc<RwLock<Box<dyn Memory>>>;

pub mod sqlite;
