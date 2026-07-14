pub mod export;
pub mod import;

use serde::{Deserialize, Serialize};

pub const AF_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFile {
    pub version: u32,
    pub mode: String,
    pub exported_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentityBlock>,
    pub config: ConfigBlock,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memories: Vec<MemoryEntry>,
    pub personas: Vec<PersonaEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub goals: Vec<GoalEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<SkillEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityBlock {
    pub age_secret: String,
    pub ed25519_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigBlock {
    pub agent: AgentConfigExport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigExport {
    pub default_model: String,
    pub daily_budget_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub persona_tag: Option<String>,
    pub content: String,
    pub tier: String,
    pub kind: String,
    pub keywords: Vec<String>,
    pub issue: Option<String>,
    pub weight: f64,
    pub is_core: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaEntry {
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: String,
    pub tier: String,
    pub weight: f32,
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalEntry {
    pub description: String,
    pub metric: Option<String>,
    pub deadline: Option<i64>,
    pub guardian_persona: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub path: String,
}

pub(crate) fn check_version(v: u32) -> anyhow::Result<()> {
    if v != AF_VERSION {
        anyhow::bail!(
            "Unsupported .af version {}. This Bastion supports version {}.",
            v,
            AF_VERSION
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::{GoalEngine, ScoringConfig};
    use crate::identity::age_identity::AgeIdentity;
    use crate::memory::sqlite::SqliteMemory;
    use crate::memory::{Memory, SharedMemory};
    use crate::persona::{Persona, PersonaRegistry};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;

    #[test]
    fn test_check_version_accepts_current() {
        assert!(check_version(AF_VERSION).is_ok());
    }

    #[test]
    fn test_check_version_rejects_unknown() {
        let err = check_version(99).unwrap_err();
        assert!(err.to_string().contains("Unsupported .af version 99"));
    }

    #[test]
    fn test_memory_entry_roundtrip() {
        let entry = MemoryEntry {
            persona_tag: Some("test".into()),
            content: "some belief".into(),
            tier: "cloud-ok".into(),
            kind: "factual".into(),
            keywords: vec!["key".into()],
            issue: None,
            weight: 1.0,
            is_core: false,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, "some belief");
    }

    /// Full export → serialize → deserialize → import roundtrip.
    /// Verifies identity, memories, goals, and personas survive the cycle.
    #[tokio::test]
    async fn test_full_export_import_roundtrip() {
        // --- Setup source DB ---
        let src = NamedTempFile::new().unwrap();
        let src_path = src.path().to_str().unwrap().to_owned();
        let sm = crate::session::sqlite::SessionManager::new(&src_path);
        sm.init_schema().await.unwrap();
        let src_mem: SharedMemory = Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(&src_path)) as Box<dyn Memory>
        ));
        let src_goals = GoalEngine::new(&src_path, ScoringConfig::default());

        // Store a belief
        {
            let m = src_mem.read().await;
            m.store_belief(
                "owner1",
                Some("health"),
                "exercises daily",
                "sess1",
                "user",
                false,
                None,
            )
            .await
            .unwrap();
            m.store_belief(
                "owner1",
                Some("identity"),
                "Bastion assistant",
                "sess1",
                "agent",
                true,
                None,
            )
            .await
            .unwrap();
        }

        // Create a goal
        src_goals
            .create_goal("owner1", "be healthy", None, None::<i64>, None)
            .await
            .unwrap();

        // Create personas
        let mut p_map = HashMap::new();
        p_map.insert(
            "helper".into(),
            Persona {
                name: "helper".into(),
                description: Some("helper persona".into()),
                system_prompt: "you help".into(),
                tier: crate::memory::PrivacyTier::CloudOk,
                weight: 1.0,
                skills: vec![],
            },
        );
        let registry = PersonaRegistry::new_from_map(p_map);
        let identity = AgeIdentity::generate();
        let config = crate::types::AgentConfig {
            default_model: "test".into(),
            daily_budget_usd: 0.01,
            fallback_models: vec![],
        };

        // --- Export ---
        let af = crate::interop::export::export_full(
            &src_mem,
            &registry,
            &src_goals,
            &config,
            Some(&identity),
            "owner1",
        )
        .await
        .unwrap();

        assert_eq!(af.mode, "full");
        assert!(af.identity.is_some());
        assert_eq!(af.memories.len(), 2);
        assert_eq!(af.goals.len(), 1);
        assert_eq!(af.personas.len(), 1);

        // --- Serialize roundtrip ---
        let json = serde_json::to_string_pretty(&af).unwrap();
        let af2: AgentFile = serde_json::from_str(&json).unwrap();

        // --- Setup dest DB ---
        let dst = NamedTempFile::new().unwrap();
        let dst_path = dst.path().to_str().unwrap().to_owned();
        let sm2 = crate::session::sqlite::SessionManager::new(&dst_path);
        sm2.init_schema().await.unwrap();
        let dst_mem: SharedMemory = Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(&dst_path)) as Box<dyn Memory>
        ));
        let dst_goals = GoalEngine::new(&dst_path, ScoringConfig::default());
        let dst_registry = PersonaRegistry::new_from_map(HashMap::new());

        // --- Import ---
        let restored_identity =
            crate::interop::import::import(af2, &dst_mem, &dst_registry, &dst_goals, "owner1")
                .await
                .unwrap()
                .expect("should return identity");

        // --- Verify ---
        assert_eq!(
            restored_identity.age_secret_bech32(),
            identity.age_secret_bech32()
        );
        assert_eq!(
            restored_identity.ed25519_secret_base64(),
            identity.ed25519_secret_base64()
        );

        let m = dst_mem.read().await;
        let beliefs = m.retrieve_all_beliefs("owner1").await.unwrap();
        assert_eq!(beliefs.len(), 2, "both beliefs should be restored");

        let goals = dst_goals.list_goals("owner1").await.unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].description, "be healthy");
    }
}
