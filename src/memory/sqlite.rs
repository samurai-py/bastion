use crate::memory::{
    Belief, BeliefDraft, BeliefKind, Memory, Outcome, PendingCorrection, PrivacyTier,
};
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task;

/// Decode the `kind` TEXT column into `BeliefKind`. Fallback to `Factual` on
/// NULL/unrecognized values — never `Option`, matches the SQL `DEFAULT 'factual'`.
fn decode_kind(kind_str: Option<String>) -> BeliefKind {
    match kind_str.as_deref() {
        Some("procedural") => BeliefKind::Procedural,
        _ => BeliefKind::Factual,
    }
}

/// Decode the `keywords` JSON-array-in-TEXT column into `Vec<String>`. Empty vec
/// on NULL or malformed JSON — never panics (T-07-01-04).
fn decode_keywords(keywords_str: Option<String>) -> Vec<String> {
    keywords_str
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

pub struct SqliteMemory {
    db_path: String,
}

impl SqliteMemory {
    pub fn new(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }
}

fn now_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

#[async_trait]
impl Memory for SqliteMemory {
    async fn store_belief(
        &self,
        owner_id: &str,
        persona_tag: Option<&str>,
        content: &str,
        session_id: &str,
        source: &str,
        is_core: bool,
        tier: Option<PrivacyTier>,
    ) -> anyhow::Result<i64> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        let persona_tag = persona_tag.map(|s| s.to_owned());
        let content = content.to_owned();
        let session_id = session_id.to_owned();
        let source = source.to_owned();
        let tier_str: Option<String> = tier.map(|t| match t {
            PrivacyTier::CloudOk => "cloud-ok".to_string(),
            PrivacyTier::LocalOnly => "local-only".to_string(),
        });
        task::spawn_blocking(move || {
            let mut conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            let now = now_nanos();
            // Atomic: belief + its provenance row commit together or not at all
            // (audit-trail integrity — no orphan belief without provenance).
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO beliefs (owner_id, persona_tag, content, weight, revoked, is_core, created_at, privacy_tier) \
                 VALUES (?1, ?2, ?3, 1.0, 0, ?4, ?5, ?6)",
                rusqlite::params![owner_id, persona_tag, content, is_core as i32, now, tier_str],
            )?;
            let belief_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO provenance (belief_id, session_id, source, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![belief_id, session_id, source, now],
            )?;
            tx.commit()?;
            Ok::<i64, anyhow::Error>(belief_id)
        })
        .await?
    }

    async fn retrieve_tagged(
        &self,
        owner_id: &str,
        persona_tag: Option<&str>,
    ) -> anyhow::Result<Vec<Belief>> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        let persona_tag = persona_tag.map(|s| s.to_owned());
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            let mut stmt = conn.prepare(
                "SELECT id, owner_id, persona_tag, content, weight, is_core, privacy_tier, \
                        kind, keywords, issue, helpful_count, harmful_count, neutral_count \
                 FROM beliefs \
                 WHERE owner_id = ?1 AND (persona_tag = ?2 OR persona_tag IS NULL) AND revoked = 0 AND weight > 0",
            )?;
            let beliefs = stmt
                .query_map(rusqlite::params![owner_id, persona_tag], |row| {
                    let tier_str: Option<String> = row.get(6)?;
                    let tier = tier_str.as_deref().and_then(|s| match s {
                        "cloud-ok" => Some(PrivacyTier::CloudOk),
                        "local-only" => Some(PrivacyTier::LocalOnly),
                        _ => None,
                    });
                    Ok(Belief {
                        id: row.get(0)?,
                        owner_id: row.get(1)?,
                        persona_tag: row.get(2)?,
                        content: row.get(3)?,
                        weight: row.get(4)?,
                        is_core: row.get::<_, i32>(5)? != 0,
                        tier,
                        kind: decode_kind(row.get(7)?),
                        keywords: decode_keywords(row.get(8)?),
                        issue: row.get(9)?,
                        helpful_count: row.get(10)?,
                        harmful_count: row.get(11)?,
                        neutral_count: row.get(12)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<Belief>, anyhow::Error>(beliefs)
        })
        .await?
    }

    async fn revoke_belief(&self, owner_id: &str, id: i64) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            let now = now_nanos();
            // Owner-scoped UPDATE (IDOR guard): a belief can only be revoked by its owner.
            let changed = conn.execute(
                "UPDATE beliefs SET weight = 0, revoked = 1, revoked_at = ?3 \
                 WHERE id = ?1 AND owner_id = ?2",
                rusqlite::params![id, owner_id, now],
            )?;
            if changed == 0 {
                anyhow::bail!("belief {id} not found for owner (no row revoked)");
            }
            Ok::<(), anyhow::Error>(())
        })
        .await?
    }

    async fn reinforce_belief(&self, owner_id: &str, id: i64, delta: f64) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            // Cap (100.0) stops a single hot trail from dominating retrieval forever. Best-effort:
            // no-match is a silent no-op (the trail may have been revoked since selection).
            conn.execute(
                "UPDATE beliefs SET weight = MIN(weight + ?3, 100.0) \
                 WHERE id = ?1 AND owner_id = ?2 AND revoked = 0 \
                       AND kind = 'procedural' AND persona_tag IS NULL",
                rusqlite::params![id, owner_id, delta],
            )?;
            Ok::<(), anyhow::Error>(())
        })
        .await?
    }

    async fn evaporate_beliefs(
        &self,
        owner_id: &str,
        factor: f64,
        floor: f64,
    ) -> anyhow::Result<u64> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            // floor > 0 keeps a decayed trail faintly retrievable; it must never hit 0, which is
            // the revoked sentinel (weight=0 + the `weight > 0` retrieval gate). Scoped to the
            // untagged procedural playbook — the same set reinforce_belief targets.
            let changed = conn.execute(
                "UPDATE beliefs SET weight = MAX(?2, weight * ?3) \
                 WHERE owner_id = ?1 AND revoked = 0 AND kind = 'procedural' \
                       AND persona_tag IS NULL AND weight > 0",
                rusqlite::params![owner_id, floor, factor],
            )?;
            Ok::<u64, anyhow::Error>(changed as u64)
        })
        .await?
    }

    async fn load_core(&self, owner_id: &str) -> anyhow::Result<Vec<Belief>> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            let mut stmt = conn.prepare(
                "SELECT id, owner_id, persona_tag, content, weight, is_core, privacy_tier, \
                        kind, keywords, issue, helpful_count, harmful_count, neutral_count \
                 FROM beliefs \
                 WHERE owner_id = ?1 AND is_core = 1 AND revoked = 0",
            )?;
            let beliefs = stmt
                .query_map(rusqlite::params![owner_id], |row| {
                    let tier_str: Option<String> = row.get(6)?;
                    let tier = tier_str.as_deref().and_then(|s| match s {
                        "cloud-ok" => Some(PrivacyTier::CloudOk),
                        "local-only" => Some(PrivacyTier::LocalOnly),
                        _ => None,
                    });
                    Ok(Belief {
                        id: row.get(0)?,
                        owner_id: row.get(1)?,
                        persona_tag: row.get(2)?,
                        content: row.get(3)?,
                        weight: row.get(4)?,
                        is_core: row.get::<_, i32>(5)? != 0,
                        tier,
                        kind: decode_kind(row.get(7)?),
                        keywords: decode_keywords(row.get(8)?),
                        issue: row.get(9)?,
                        helpful_count: row.get(10)?,
                        harmful_count: row.get(11)?,
                        neutral_count: row.get(12)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<Belief>, anyhow::Error>(beliefs)
        })
        .await?
    }

    async fn provenance_for(
        &self,
        owner_id: &str,
        belief_id: i64,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            // Owner-scoped JOIN (IDOR guard): only return provenance when the
            // belief belongs to the caller; cross-owner probes get an empty vec.
            let mut stmt = conn.prepare(
                "SELECT p.session_id, COALESCE(p.source, '') \
                 FROM provenance p JOIN beliefs b ON b.id = p.belief_id \
                 WHERE p.belief_id = ?1 AND b.owner_id = ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![belief_id, owner_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<(String, String)>, anyhow::Error>(rows)
        })
        .await?
    }

    async fn store_procedural_belief(&self, draft: BeliefDraft) -> anyhow::Result<i64> {
        let path = self.db_path.clone();
        let owner_id = draft.owner_id;
        let persona_tag = draft.persona_tag;
        let content = draft.insight;
        let session_id = draft.session_id;
        let source = draft.source;
        let issue = draft.issue;
        let keywords_json = serde_json::to_string(&draft.keywords)?;
        let tier_str: Option<String> = draft.tier.map(|t| match t {
            PrivacyTier::CloudOk => "cloud-ok".to_string(),
            PrivacyTier::LocalOnly => "local-only".to_string(),
        });
        task::spawn_blocking(move || {
            let mut conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            let now = now_nanos();
            // Atomic: belief + its provenance row commit together or not at all —
            // mirrors store_belief's exact transaction shape.
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO beliefs (owner_id, persona_tag, content, weight, revoked, is_core, \
                                       created_at, privacy_tier, kind, keywords, issue) \
                 VALUES (?1, ?2, ?3, 1.0, 0, 0, ?4, ?5, 'procedural', ?6, ?7)",
                rusqlite::params![
                    owner_id,
                    persona_tag,
                    content,
                    now,
                    tier_str,
                    keywords_json,
                    issue
                ],
            )?;
            let belief_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO provenance (belief_id, session_id, source, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![belief_id, session_id, source, now],
            )?;
            tx.commit()?;
            Ok::<i64, anyhow::Error>(belief_id)
        })
        .await?
    }

    async fn record_belief_outcome(
        &self,
        owner_id: &str,
        id: i64,
        outcome: Outcome,
    ) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        // Column name is a fixed 3-way match, never string-interpolated from user
        // input — no injection surface.
        let column = match outcome {
            Outcome::Helpful => "helpful_count",
            Outcome::Harmful => "harmful_count",
            Outcome::Neutral => "neutral_count",
        };
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            // Owner-scoped UPDATE (IDOR guard): same discipline as revoke_belief.
            let sql = format!(
                "UPDATE beliefs SET {column} = {column} + 1 WHERE id = ?1 AND owner_id = ?2"
            );
            let changed = conn.execute(&sql, rusqlite::params![id, owner_id])?;
            if changed == 0 {
                anyhow::bail!("belief {id} not found for owner (no outcome recorded)");
            }
            Ok::<(), anyhow::Error>(())
        })
        .await?
    }

    async fn record_pending_correction(
        &self,
        owner_id: &str,
        belief_id: i64,
        tier: Option<PrivacyTier>,
    ) -> anyhow::Result<i64> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        let tier_str: Option<String> = tier.map(|t| match t {
            PrivacyTier::CloudOk => "cloud-ok".to_string(),
            PrivacyTier::LocalOnly => "local-only".to_string(),
        });
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            // Owner-scoped IDOR guard (WR-01): a correction may only be queued against a
            // belief the caller actually owns. Matches revoke_belief/record_belief_outcome —
            // bail rather than silently insert a row pointing at another owner's belief.
            let owns: bool = conn
                .query_row(
                    "SELECT 1 FROM beliefs WHERE id = ?1 AND owner_id = ?2",
                    rusqlite::params![belief_id, owner_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !owns {
                anyhow::bail!("belief {belief_id} not found for owner (no pending correction queued)");
            }
            let now = now_nanos();
            conn.execute(
                "INSERT INTO pending_corrections (belief_id, owner_id, tier, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![belief_id, owner_id, tier_str, now],
            )?;
            Ok::<i64, anyhow::Error>(conn.last_insert_rowid())
        })
        .await?
    }

    async fn take_pending_corrections(
        &self,
        owner_id: &str,
    ) -> anyhow::Result<Vec<PendingCorrection>> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        task::spawn_blocking(move || {
            let mut conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
            let tx = conn.transaction()?;
            let mut stmt = tx.prepare(
                "SELECT id, belief_id, owner_id, tier, created_at FROM pending_corrections WHERE owner_id = ?1",
            )?;
            let rows: Vec<PendingCorrection> = stmt
                .query_map(rusqlite::params![owner_id], |row| {
                    let tier_str: Option<String> = row.get(3)?;
                    let tier = tier_str.as_deref().and_then(|s| match s {
                        "cloud-ok" => Some(PrivacyTier::CloudOk),
                        "local-only" => Some(PrivacyTier::LocalOnly),
                        _ => None,
                    });
                    Ok(PendingCorrection {
                        id: row.get(0)?,
                        belief_id: row.get(1)?,
                        owner_id: row.get(2)?,
                        tier,
                        created_at: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);
            // Owner-scoped delete (IDOR guard) — dequeue exactly the rows just read,
            // same owner.
            tx.execute(
                "DELETE FROM pending_corrections WHERE owner_id = ?1",
                rusqlite::params![owner_id],
            )?;
            tx.commit()?;
            Ok::<Vec<PendingCorrection>, anyhow::Error>(rows)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;
    use crate::session::sqlite::SessionManager;
    use tempfile::NamedTempFile;

    async fn make_db() -> (NamedTempFile, SqliteMemory) {
        let f = NamedTempFile::new().expect("tempfile");
        let path = f.path().to_str().unwrap().to_owned();
        let session_mgr = SessionManager::new(&path);
        session_mgr.init_schema().await.expect("init_schema");
        let mem = SqliteMemory::new(&path);
        (f, mem)
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief(
                "owner1",
                Some("health"),
                "Mario exercises daily",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store");
        assert!(id > 0);

        let beliefs = mem
            .retrieve_tagged("owner1", Some("health"))
            .await
            .expect("retrieve");
        assert_eq!(beliefs.len(), 1);
        assert_eq!(beliefs[0].content, "Mario exercises daily");
        assert!(!beliefs[0].is_core);
    }

    #[tokio::test]
    async fn test_revoke_excludes_from_retrieve_but_row_preserved() {
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief(
                "owner1",
                Some("finance"),
                "Has savings",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store");

        // Before revoke: retrieve returns it
        let before = mem
            .retrieve_tagged("owner1", Some("finance"))
            .await
            .expect("before");
        assert_eq!(before.len(), 1);

        // Revoke
        mem.revoke_belief("owner1", id).await.expect("revoke");

        // After revoke: retrieve_tagged excludes it
        let after = mem
            .retrieve_tagged("owner1", Some("finance"))
            .await
            .expect("after");
        assert!(
            after.is_empty(),
            "revoked belief must not appear in retrieve_tagged"
        );

        // But raw SELECT still returns it with revoked=1, weight=0
        let path = mem.db_path.clone();
        let (revoked, weight) = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path).unwrap();
            let mut stmt = conn
                .prepare("SELECT revoked, weight FROM beliefs WHERE id = ?1")
                .unwrap();
            stmt.query_row(rusqlite::params![id], |row| {
                Ok((row.get::<_, i32>(0)?, row.get::<_, f64>(1)?))
            })
            .unwrap()
        })
        .await
        .unwrap();

        assert_eq!(revoked, 1, "revoked flag must be 1");
        assert_eq!(weight, 0.0, "weight must be 0 after revoke");
    }

    #[tokio::test]
    async fn test_no_delete_on_revoke() {
        // D-15: soft-revoke only — never hard-delete a belief row.
        // This test verifies the row count stays the same after revoke
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief(
                "owner1",
                None,
                "Global belief",
                "sess1",
                "dream",
                false,
                None,
            )
            .await
            .expect("store");

        let path = mem.db_path.clone();
        let count_before = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path).unwrap();
            let c: i64 = conn
                .query_row("SELECT COUNT(*) FROM beliefs", [], |r| r.get(0))
                .unwrap();
            c
        })
        .await
        .unwrap();

        mem.revoke_belief("owner1", id).await.expect("revoke");

        let path2 = mem.db_path.clone();
        let count_after = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path2).unwrap();
            let c: i64 = conn
                .query_row("SELECT COUNT(*) FROM beliefs", [], |r| r.get(0))
                .unwrap();
            c
        })
        .await
        .unwrap();

        assert_eq!(
            count_before, count_after,
            "row must not be deleted on revoke (D-15)"
        );
    }

    #[tokio::test]
    async fn test_global_belief_visible_to_any_persona_tag() {
        // persona_tag IS NULL beliefs appear for any tagged retrieve (MEM-03/04)
        let (_f, mem) = make_db().await;
        mem.store_belief("owner1", None, "Global fact", "sess1", "user", false, None)
            .await
            .expect("store global");
        mem.store_belief(
            "owner1",
            Some("health"),
            "Health-tagged",
            "sess1",
            "user",
            false,
            None,
        )
        .await
        .expect("store tagged");

        let results = mem
            .retrieve_tagged("owner1", Some("health"))
            .await
            .expect("retrieve");
        // Should see both: the health-tagged one AND the NULL-tagged global one
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_load_core() {
        let (_f, mem) = make_db().await;
        mem.store_belief("owner1", None, "Core belief", "sess1", "system", true, None)
            .await
            .expect("store core");
        mem.store_belief(
            "owner1",
            None,
            "Normal belief",
            "sess1",
            "user",
            false,
            None,
        )
        .await
        .expect("store normal");

        let core = mem.load_core("owner1").await.expect("load_core");
        assert_eq!(core.len(), 1);
        assert!(core[0].is_core);
        assert_eq!(core[0].content, "Core belief");
    }

    #[tokio::test]
    async fn test_provenance_stored() {
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief("owner1", None, "Some fact", "sess-abc", "tool", false, None)
            .await
            .expect("store");

        let prov = mem.provenance_for("owner1", id).await.expect("provenance");
        assert_eq!(prov.len(), 1);
        assert_eq!(prov[0].0, "sess-abc");
        assert_eq!(prov[0].1, "tool");
    }

    #[tokio::test]
    async fn test_owner_isolation_revoke_and_provenance() {
        // IDOR guard: owner2 cannot revoke or read provenance of owner1's belief.
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief(
                "owner1",
                None,
                "Owner1 secret",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store");

        // Wrong owner cannot revoke (errors, does not silently no-op)
        let revoked = mem.revoke_belief("owner2", id).await;
        assert!(revoked.is_err(), "cross-owner revoke must error");

        // Belief still active for the real owner
        let still = mem.retrieve_tagged("owner1", None).await.expect("retrieve");
        assert_eq!(
            still.len(),
            1,
            "belief must survive cross-owner revoke attempt"
        );

        // Wrong owner gets empty provenance (indistinguishable from missing id)
        let prov_wrong = mem.provenance_for("owner2", id).await.expect("prov wrong");
        assert!(
            prov_wrong.is_empty(),
            "cross-owner provenance must be empty"
        );

        // Real owner still sees provenance
        let prov_ok = mem.provenance_for("owner1", id).await.expect("prov ok");
        assert_eq!(prov_ok.len(), 1);
    }

    #[tokio::test]
    async fn test_privacy_tier_deserialize() {
        use crate::memory::PrivacyTier;
        // serde kebab-case: "local-only" and "cloud-ok"
        let t: PrivacyTier = serde_json::from_str("\"local-only\"").expect("local-only");
        assert_eq!(t, PrivacyTier::LocalOnly);
        let t2: PrivacyTier = serde_json::from_str("\"cloud-ok\"").expect("cloud-ok");
        assert_eq!(t2, PrivacyTier::CloudOk);
    }

    #[tokio::test]
    async fn test_tier_persists_and_survives_filter_for_mesh() {
        use crate::memory::PrivacyTier;
        use crate::mesh::allowlist::{filter_for_mesh, OwnerAllowlist};

        let (_f, mem) = make_db().await;

        // Store a CloudOk belief with a tag in the allowlist
        mem.store_belief(
            "owner1",
            Some("mercado"),
            "Alice spends 2k/month on groceries",
            "sess1",
            "user",
            false,
            Some(PrivacyTier::CloudOk),
        )
        .await
        .expect("store cloud-ok belief");

        // Store a LocalOnly belief — should be stripped
        mem.store_belief(
            "owner1",
            Some("mercado"),
            "Alice's bank password",
            "sess2",
            "user",
            false,
            Some(PrivacyTier::LocalOnly),
        )
        .await
        .expect("store local-only belief");

        // Retrieve from real DB (not hand-built Beliefs)
        let beliefs = mem
            .retrieve_tagged("owner1", Some("mercado"))
            .await
            .expect("retrieve");
        assert_eq!(beliefs.len(), 2, "both beliefs should be retrieved");

        // filter_for_mesh with allowlist that includes 'mercado'
        let allowlist = OwnerAllowlist {
            owner_id: "owner1".to_string(),
            allowed_tags: vec!["mercado".to_string()],
        };
        let passed = filter_for_mesh(beliefs, &allowlist);

        // Only CloudOk belief survives
        assert_eq!(
            passed.len(),
            1,
            "only CloudOk belief must survive filter_for_mesh"
        );
        assert_eq!(passed[0].content, "Alice spends 2k/month on groceries");
        assert_eq!(passed[0].tier, Some(PrivacyTier::CloudOk));
    }

    #[tokio::test]
    async fn test_procedural_kind_tier_persists_and_survives_filter_for_mesh() {
        use crate::memory::{BeliefDraft, BeliefKind};
        use crate::mesh::allowlist::{filter_for_mesh, OwnerAllowlist};

        let (_f, mem) = make_db().await;

        // Store a CloudOk procedural belief with a tag in the allowlist
        mem.store_procedural_belief(BeliefDraft {
            owner_id: "owner1".to_string(),
            persona_tag: Some("mercado".to_string()),
            issue: Some("Overspending on groceries".to_string()),
            insight: "Alice spends 2k/month on groceries".to_string(),
            keywords: vec!["budget".to_string()],
            session_id: "sess1".to_string(),
            source: "reflector".to_string(),
            tier: Some(PrivacyTier::CloudOk),
        })
        .await
        .expect("store cloud-ok procedural belief");

        // Store a LocalOnly procedural belief — should be stripped
        mem.store_procedural_belief(BeliefDraft {
            owner_id: "owner1".to_string(),
            persona_tag: Some("mercado".to_string()),
            issue: Some("Sensitive info".to_string()),
            insight: "Alice's bank password".to_string(),
            keywords: vec!["secret".to_string()],
            session_id: "sess2".to_string(),
            source: "reflector".to_string(),
            tier: Some(PrivacyTier::LocalOnly),
        })
        .await
        .expect("store local-only procedural belief");

        // Retrieve from real DB (not hand-built Beliefs)
        let beliefs = mem
            .retrieve_tagged("owner1", Some("mercado"))
            .await
            .expect("retrieve");
        assert_eq!(beliefs.len(), 2, "both beliefs should be retrieved");
        assert!(
            beliefs.iter().all(|b| b.kind == BeliefKind::Procedural),
            "both retrieved beliefs must decode as Procedural"
        );

        // filter_for_mesh with allowlist that includes 'mercado'
        let allowlist = OwnerAllowlist {
            owner_id: "owner1".to_string(),
            allowed_tags: vec!["mercado".to_string()],
        };
        let passed = filter_for_mesh(beliefs, &allowlist);

        // Only CloudOk belief survives
        assert_eq!(
            passed.len(),
            1,
            "only CloudOk procedural belief must survive filter_for_mesh"
        );
        assert_eq!(passed[0].content, "Alice spends 2k/month on groceries");
        assert_eq!(passed[0].tier, Some(PrivacyTier::CloudOk));
        assert_eq!(
            passed[0].kind,
            BeliefKind::Procedural,
            "kind must survive retrieve_tagged -> filter_for_mesh unchanged"
        );
    }

    #[tokio::test]
    async fn test_store_procedural_belief_round_trip() {
        use crate::memory::{BeliefDraft, BeliefKind};

        let (_f, mem) = make_db().await;
        let draft = BeliefDraft {
            owner_id: "owner1".to_string(),
            persona_tag: Some("coding".to_string()),
            issue: Some("Merge conflicts on rebase".to_string()),
            insight: "Always rebase onto origin/main before pushing".to_string(),
            keywords: vec!["git".to_string(), "rebase".to_string()],
            session_id: "sess1".to_string(),
            source: "reflector".to_string(),
            tier: Some(PrivacyTier::CloudOk),
        };
        let id = mem
            .store_procedural_belief(draft)
            .await
            .expect("store procedural belief");
        assert!(id > 0);

        let beliefs = mem
            .retrieve_tagged("owner1", Some("coding"))
            .await
            .expect("retrieve");
        assert_eq!(beliefs.len(), 1);
        let belief = &beliefs[0];
        assert_eq!(belief.kind, BeliefKind::Procedural);
        assert_eq!(belief.issue.as_deref(), Some("Merge conflicts on rebase"));
        assert_eq!(
            belief.keywords,
            vec!["git".to_string(), "rebase".to_string()]
        );
        assert_eq!(
            belief.content,
            "Always rebase onto origin/main before pushing"
        );
        assert_eq!(belief.tier, Some(PrivacyTier::CloudOk));
        assert_eq!(belief.helpful_count, 0);
        assert_eq!(belief.harmful_count, 0);
        assert_eq!(belief.neutral_count, 0);
    }

    #[tokio::test]
    async fn test_legacy_store_belief_defaults_kind_factual() {
        use crate::memory::BeliefKind;

        // OLD store_belief call shape (unchanged signature, Phase 1-6 behavior) —
        // proves the additive migration is invisible to existing callers.
        let (_f, mem) = make_db().await;
        mem.store_belief(
            "owner1",
            Some("health"),
            "Mario exercises daily",
            "sess1",
            "user",
            false,
            None,
        )
        .await
        .expect("store");

        let beliefs = mem
            .retrieve_tagged("owner1", Some("health"))
            .await
            .expect("retrieve");
        assert_eq!(beliefs.len(), 1);
        let belief = &beliefs[0];
        assert_eq!(belief.kind, BeliefKind::Factual);
        assert!(belief.keywords.is_empty());
        assert!(belief.issue.is_none());
        assert_eq!(belief.helpful_count, 0);
        assert_eq!(belief.harmful_count, 0);
        assert_eq!(belief.neutral_count, 0);
    }

    #[tokio::test]
    async fn test_record_belief_outcome_increments_exactly_one_counter() {
        use crate::memory::Outcome;

        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief(
                "owner1",
                Some("coding"),
                "Some procedural-ish content",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store");

        mem.record_belief_outcome("owner1", id, Outcome::Helpful)
            .await
            .expect("first outcome");
        mem.record_belief_outcome("owner1", id, Outcome::Helpful)
            .await
            .expect("second outcome");

        let beliefs = mem
            .retrieve_tagged("owner1", Some("coding"))
            .await
            .expect("retrieve");
        assert_eq!(beliefs.len(), 1);
        let belief = &beliefs[0];
        assert_eq!(belief.helpful_count, 2);
        assert_eq!(belief.harmful_count, 0);
        assert_eq!(belief.neutral_count, 0);
        assert_eq!(belief.content, "Some procedural-ish content");
    }

    #[tokio::test]
    async fn test_record_belief_outcome_cross_owner_errors() {
        use crate::memory::Outcome;

        // IDOR guard: owner2 cannot record an outcome on owner1's belief.
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief(
                "owner1",
                None,
                "Owner1 procedural belief",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store");

        let result = mem
            .record_belief_outcome("owner2", id, Outcome::Harmful)
            .await;
        assert!(result.is_err(), "cross-owner outcome must error");

        // Real owner's counters are unaffected by the failed cross-owner attempt.
        let beliefs = mem.retrieve_tagged("owner1", None).await.expect("retrieve");
        assert_eq!(beliefs.len(), 1);
        assert_eq!(beliefs[0].helpful_count, 0);
        assert_eq!(beliefs[0].harmful_count, 0);
        assert_eq!(beliefs[0].neutral_count, 0);
    }

    #[tokio::test]
    async fn test_pending_correction_round_trip() {
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief("owner1", None, "Some belief", "sess1", "user", false, None)
            .await
            .expect("store");

        mem.record_pending_correction("owner1", id, Some(PrivacyTier::CloudOk))
            .await
            .expect("record_pending_correction");

        let taken = mem
            .take_pending_corrections("owner1")
            .await
            .expect("take_pending_corrections");
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].belief_id, id);
        assert_eq!(taken[0].owner_id, "owner1");
        assert_eq!(taken[0].tier, Some(PrivacyTier::CloudOk));

        // Dequeue-on-read: a second immediate take must return empty.
        let taken_again = mem
            .take_pending_corrections("owner1")
            .await
            .expect("take_pending_corrections second call");
        assert!(
            taken_again.is_empty(),
            "second immediate take must return empty (dequeue-on-read)"
        );
    }

    #[tokio::test]
    async fn test_pending_correction_owner_scoped() {
        let (_f, mem) = make_db().await;
        let id = mem
            .store_belief(
                "owner1",
                None,
                "Owner1 belief",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .expect("store");

        mem.record_pending_correction("owner1", id, None)
            .await
            .expect("record_pending_correction");

        // owner2's take must not see owner1's row, and must not consume it.
        let taken_by_owner2 = mem
            .take_pending_corrections("owner2")
            .await
            .expect("take by owner2");
        assert!(
            taken_by_owner2.is_empty(),
            "cross-owner take must return empty (IDOR guard)"
        );

        // owner1's row must still be there — owner2's take must not have dropped it.
        let taken_by_owner1 = mem
            .take_pending_corrections("owner1")
            .await
            .expect("take by owner1");
        assert_eq!(
            taken_by_owner1.len(),
            1,
            "owner1's pending correction must survive an unrelated owner2 take"
        );
        assert_eq!(taken_by_owner1[0].belief_id, id);
    }

    #[tokio::test]
    async fn record_pending_correction_rejects_cross_owner_belief() {
        use crate::memory::BeliefDraft;
        let (_f, mem) = make_db().await;
        // alice owns a procedural belief.
        let alice_belief = mem
            .store_procedural_belief(BeliefDraft {
                owner_id: "alice".to_string(),
                persona_tag: None,
                issue: None,
                insight: "alice-only strategy".to_string(),
                keywords: vec![],
                session_id: "s".to_string(),
                source: "test".to_string(),
                tier: Some(PrivacyTier::CloudOk),
            })
            .await
            .expect("store");

        // bob must NOT be able to queue a correction against alice's belief (WR-01 IDOR guard).
        let res = mem
            .record_pending_correction("bob", alice_belief, Some(PrivacyTier::CloudOk))
            .await;
        assert!(
            res.is_err(),
            "record_pending_correction must reject a belief_id the caller does not own"
        );

        // and no row must have been queued for bob.
        let bob_pending = mem.take_pending_corrections("bob").await.expect("drain");
        assert!(
            bob_pending.is_empty(),
            "no cross-owner correction row must be inserted"
        );
    }
}
