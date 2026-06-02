use crate::memory::{Belief, Memory};
use async_trait::async_trait;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task;

pub struct SqliteMemory {
    db_path: String,
}

impl SqliteMemory {
    pub fn new(db_path: impl Into<String>) -> Self {
        Self { db_path: db_path.into() }
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
    ) -> anyhow::Result<i64> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        let persona_tag = persona_tag.map(|s| s.to_owned());
        let content = content.to_owned();
        let session_id = session_id.to_owned();
        let source = source.to_owned();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let now = now_nanos();
            conn.execute(
                "INSERT INTO beliefs (owner_id, persona_tag, content, weight, revoked, is_core, created_at) \
                 VALUES (?1, ?2, ?3, 1.0, 0, ?4, ?5)",
                rusqlite::params![owner_id, persona_tag, content, is_core as i32, now],
            )?;
            let belief_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO provenance (belief_id, session_id, source, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![belief_id, session_id, source, now],
            )?;
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
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let mut stmt = conn.prepare(
                "SELECT id, owner_id, persona_tag, content, weight, is_core \
                 FROM beliefs \
                 WHERE owner_id = ?1 AND (persona_tag = ?2 OR persona_tag IS NULL) AND revoked = 0 AND weight > 0",
            )?;
            let beliefs = stmt
                .query_map(rusqlite::params![owner_id, persona_tag], |row| {
                    Ok(Belief {
                        id: row.get(0)?,
                        owner_id: row.get(1)?,
                        persona_tag: row.get(2)?,
                        content: row.get(3)?,
                        weight: row.get(4)?,
                        is_core: row.get::<_, i32>(5)? != 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<Belief>, anyhow::Error>(beliefs)
        })
        .await?
    }

    async fn revoke_belief(&self, id: i64) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let now = now_nanos();
            conn.execute(
                "UPDATE beliefs SET weight = 0, revoked = 1, revoked_at = ?2 WHERE id = ?1",
                rusqlite::params![id, now],
            )?;
            Ok::<(), anyhow::Error>(())
        })
        .await?
    }

    async fn load_core(&self, owner_id: &str) -> anyhow::Result<Vec<Belief>> {
        let path = self.db_path.clone();
        let owner_id = owner_id.to_owned();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let mut stmt = conn.prepare(
                "SELECT id, owner_id, persona_tag, content, weight, is_core \
                 FROM beliefs \
                 WHERE owner_id = ?1 AND is_core = 1 AND revoked = 0",
            )?;
            let beliefs = stmt
                .query_map(rusqlite::params![owner_id], |row| {
                    Ok(Belief {
                        id: row.get(0)?,
                        owner_id: row.get(1)?,
                        persona_tag: row.get(2)?,
                        content: row.get(3)?,
                        weight: row.get(4)?,
                        is_core: row.get::<_, i32>(5)? != 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<Belief>, anyhow::Error>(beliefs)
        })
        .await?
    }

    async fn provenance_for(&self, belief_id: i64) -> anyhow::Result<Vec<(String, String)>> {
        let path = self.db_path.clone();
        task::spawn_blocking(move || {
            let conn = Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let mut stmt = conn.prepare(
                "SELECT session_id, COALESCE(source, '') FROM provenance WHERE belief_id = ?1",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![belief_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<(String, String)>, anyhow::Error>(rows)
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
            .store_belief("owner1", Some("health"), "Mario exercises daily", "sess1", "user", false)
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
            .store_belief("owner1", Some("finance"), "Has savings", "sess1", "user", false)
            .await
            .expect("store");

        // Before revoke: retrieve returns it
        let before = mem.retrieve_tagged("owner1", Some("finance")).await.expect("before");
        assert_eq!(before.len(), 1);

        // Revoke
        mem.revoke_belief(id).await.expect("revoke");

        // After revoke: retrieve_tagged excludes it
        let after = mem.retrieve_tagged("owner1", Some("finance")).await.expect("after");
        assert!(after.is_empty(), "revoked belief must not appear in retrieve_tagged");

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
            .store_belief("owner1", None, "Global belief", "sess1", "dream", false)
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

        mem.revoke_belief(id).await.expect("revoke");

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

        assert_eq!(count_before, count_after, "row must not be deleted on revoke (D-15)");
    }

    #[tokio::test]
    async fn test_global_belief_visible_to_any_persona_tag() {
        // persona_tag IS NULL beliefs appear for any tagged retrieve (MEM-03/04)
        let (_f, mem) = make_db().await;
        mem.store_belief("owner1", None, "Global fact", "sess1", "user", false)
            .await
            .expect("store global");
        mem.store_belief("owner1", Some("health"), "Health-tagged", "sess1", "user", false)
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
        mem.store_belief("owner1", None, "Core belief", "sess1", "system", true)
            .await
            .expect("store core");
        mem.store_belief("owner1", None, "Normal belief", "sess1", "user", false)
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
            .store_belief("owner1", None, "Some fact", "sess-abc", "tool", false)
            .await
            .expect("store");

        let prov = mem.provenance_for(id).await.expect("provenance");
        assert_eq!(prov.len(), 1);
        assert_eq!(prov[0].0, "sess-abc");
        assert_eq!(prov[0].1, "tool");
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
}
