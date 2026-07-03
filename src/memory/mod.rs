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

/// Belief kind — factual (default, Phase 1-6 behavior) or procedural (LEARN-01).
/// Defaults to `Factual` so every pre-Phase-7 row (DB default `'factual'`) decodes
/// identically to before this column existed — zero behavior change for existing data.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BeliefKind {
    Factual,
    Procedural,
}

impl Default for BeliefKind {
    fn default() -> Self {
        BeliefKind::Factual
    }
}

/// Outcome signal for a procedural belief's helpful/harmful/neutral counters.
/// Maps 1:1 onto `record_belief_outcome`'s counter-increment column choice.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Helpful,
    Harmful,
    Neutral,
}

/// Builder-style draft for a new procedural belief. Used by `store_procedural_belief`
/// instead of widening `store_belief`'s already-7-argument signature (Pitfall 5).
/// `insight` maps onto the existing `content` column (ACE terminology overlay) —
/// there is no parallel content field.
pub struct BeliefDraft {
    pub owner_id: String,
    pub persona_tag: Option<String>,
    pub issue: Option<String>,
    pub insight: String,
    pub keywords: Vec<String>,
    pub session_id: String,
    pub source: String,
    pub tier: Option<PrivacyTier>,
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
    /// Factual (default) or procedural (LEARN-01). Never `Option` — decodes to
    /// `Factual` on NULL/unrecognized column value, matching the SQL `DEFAULT 'factual'`.
    pub kind: BeliefKind,
    /// Procedural skill-matching tags. Empty vec on NULL or malformed JSON — never panics.
    pub keywords: Vec<String>,
    /// The problem/context a procedural belief addresses (ACE "issue" terminology).
    pub issue: Option<String>,
    pub helpful_count: i64,
    pub harmful_count: i64,
    pub neutral_count: i64,
}

/// Core memory abstraction. Every subsystem reads/writes beliefs through this trait.
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a belief and one provenance row; returns the new belief id.
    // A belief + its provenance is 8 flat fields; bundling them into a struct here
    // would force every impl and caller through a one-use wrapper for no gain.
    #[allow(clippy::too_many_arguments)]
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

    /// Store a procedural belief (kind='procedural') + its provenance row. Mirrors
    /// store_belief's atomic belief+provenance transaction; does NOT widen
    /// store_belief (Pitfall 5).
    async fn store_procedural_belief(&self, draft: BeliefDraft) -> anyhow::Result<i64>;

    /// Increment exactly one counter (helpful/harmful/neutral) on an existing belief.
    /// Content untouched. Owner-scoped (IDOR guard) — errors on cross-owner no-op,
    /// same discipline as revoke_belief.
    async fn record_belief_outcome(
        &self,
        owner_id: &str,
        id: i64,
        outcome: Outcome,
    ) -> anyhow::Result<()>;
}

/// Clonable shared-handle alias — mirrors SharedProvider from provider/mod.rs.
pub type SharedMemory = Arc<RwLock<Box<dyn Memory>>>;

pub mod sqlite;
