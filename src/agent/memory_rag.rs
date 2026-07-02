//! SEAM #2 — `MemoryRagProvider`: recall de beliefs por INJEÇÃO de contexto.
//!
//! A perna "injeção RAG" da decisão pendente do BIG-1 (tool-calling vs injeção
//! vs híbrido). Recupera beliefs relevantes pro turn e injeta como bloco opaco no
//! system prompt — funciona com QUALQUER provider, incluindo terminal-agents
//! (PROV-09) que nunca emitem `tool_calls`, e permanece egress-safe: os blocos
//! saem separados por tier, então `build_system_prompt` derruba só o bloco
//! LocalOnly quando o provider é cloud (Pitfall 5).
//!
//! Relevância é LÉXICA e barata (overlap de termos + weight + recência) — de
//! propósito: recall semântico de verdade é papel do memupalace (embedding
//! local), acessível via tool ou apontando o terminal-agent pro MCP dele.
//! Este provider cobre o caminho que não depende do modelo decidir chamar tool.
//!
//! Opt-in via env `BASTION_MEMORY_RAG=1` (wiring em `AgentLoop::new`) até a
//! decisão do híbrido: default-on duplicaria a exposição de memória em providers
//! com function-calling (que já recebem as tools de memória) e cresce o prompt.

use crate::agent::context::{ContextBlock, TurnContextProvider};
use crate::memory::{Belief, PrivacyTier, SharedMemory};

/// Máximo de beliefs injetados por turn (após ranking).
const DEFAULT_MAX_BELIEFS: usize = 8;

/// Termos do turn com menos caracteres que isso não contam pro overlap
/// (artigos/preposições dominariam o score).
const MIN_TERM_LEN: usize = 4;

pub struct MemoryRagProvider {
    memory: SharedMemory,
    max_beliefs: usize,
}

impl MemoryRagProvider {
    pub fn new(memory: SharedMemory) -> Self {
        Self {
            memory,
            max_beliefs: DEFAULT_MAX_BELIEFS,
        }
    }

    #[cfg(test)]
    fn with_max(memory: SharedMemory, max_beliefs: usize) -> Self {
        Self {
            memory,
            max_beliefs,
        }
    }
}

/// Overlap léxico: quantos termos (≥ MIN_TERM_LEN chars, case-insensitive) do
/// turn aparecem no conteúdo do belief. Zero = sem relação detectável.
fn lexical_overlap(turn_msg: &str, content: &str) -> usize {
    let content_lower = content.to_lowercase();
    turn_msg
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= MIN_TERM_LEN)
        .filter(|t| content_lower.contains(&t.to_lowercase()))
        .count()
}

/// Formata um grupo de beliefs como bloco opaco. O id entra no texto de
/// propósito: é o handle de contestação por NL (`/contest <id>`, D-14).
fn render_block(beliefs: &[&Belief]) -> String {
    let mut s = String::from(
        "<memory_recall>\nLong-term memories about this owner (contest with /contest <id> if wrong):\n",
    );
    for b in beliefs {
        s.push_str(&format!("- [id {}] {}\n", b.id, b.content));
    }
    s.push_str("</memory_recall>");
    s
}

#[async_trait::async_trait]
impl TurnContextProvider for MemoryRagProvider {
    async fn context_for_turn(&self, owner: &str, turn_msg: &str) -> Vec<ContextBlock> {
        let beliefs = {
            let mem = self.memory.read().await;
            match mem.retrieve_tagged(owner, None).await {
                Ok(b) => b,
                Err(e) => {
                    // Recall é enriquecimento, nunca bloqueia o turn (fail-open aqui é
                    // correto: sem memória o agente ainda responde; o erro fica visível).
                    tracing::warn!(event = "memory_rag_retrieve_failed", error = %e);
                    return vec![];
                }
            }
        };

        // Identidade já é injetada pelo IdentityProvider — não duplicar.
        let mut candidates: Vec<&Belief> = beliefs
            .iter()
            .filter(|b| b.persona_tag.as_deref() != Some("identity"))
            .collect();
        if candidates.is_empty() {
            return vec![];
        }

        // Rank: overlap léxico desc → weight desc → id desc (mais recente primeiro).
        candidates.sort_by(|a, b| {
            let score_a = lexical_overlap(turn_msg, &a.content);
            let score_b = lexical_overlap(turn_msg, &b.content);
            score_b
                .cmp(&score_a)
                .then(
                    b.weight
                        .partial_cmp(&a.weight)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(b.id.cmp(&a.id))
        });
        candidates.truncate(self.max_beliefs);

        // Um bloco POR TIER, pra que o egress check derrube só o que precisa:
        // tier None = LocalOnly (deny-on-ambiguity, consistente com CR-03).
        let (cloud_ok, local_only): (Vec<&Belief>, Vec<&Belief>) = candidates
            .into_iter()
            .partition(|b| b.tier == Some(PrivacyTier::CloudOk));

        let mut blocks = Vec::with_capacity(2);
        if !cloud_ok.is_empty() {
            blocks.push(ContextBlock {
                content: render_block(&cloud_ok),
                max_tier: PrivacyTier::CloudOk,
            });
        }
        if !local_only.is_empty() {
            blocks.push(ContextBlock {
                content: render_block(&local_only),
                max_tier: PrivacyTier::LocalOnly,
            });
        }
        blocks
    }
}

// ---------------------------------------------------------------------------
// Tests (offline — temp-DB SqliteMemory, pattern from agent/command.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::sqlite::SqliteMemory;
    use crate::memory::Memory;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;

    async fn make_memory(db_path: &str) -> SharedMemory {
        let session = crate::session::SessionManager::new(db_path);
        session.init_schema().await.expect("init_schema");
        Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(db_path)) as Box<dyn Memory>
        ))
    }

    async fn store(
        mem: &SharedMemory,
        owner: &str,
        content: &str,
        tag: Option<&str>,
        tier: Option<PrivacyTier>,
    ) -> i64 {
        let m = mem.read().await;
        m.store_belief(owner, tag, content, "sess1", "test", false, tier)
            .await
            .expect("store")
    }

    #[tokio::test]
    async fn empty_memory_returns_no_blocks() {
        let f = NamedTempFile::new().unwrap();
        let mem = make_memory(f.path().to_str().unwrap()).await;
        let provider = MemoryRagProvider::new(mem);
        let blocks = provider.context_for_turn("_local", "hello").await;
        assert!(blocks.is_empty());
    }

    #[tokio::test]
    async fn blocks_are_split_by_tier_and_none_is_local_only() {
        let f = NamedTempFile::new().unwrap();
        let mem = make_memory(f.path().to_str().unwrap()).await;
        store(
            &mem,
            "_local",
            "likes coffee",
            None,
            Some(PrivacyTier::CloudOk),
        )
        .await;
        store(
            &mem,
            "_local",
            "medical condition X",
            None,
            Some(PrivacyTier::LocalOnly),
        )
        .await;
        store(&mem, "_local", "untagged legacy belief", None, None).await;

        let provider = MemoryRagProvider::new(mem);
        let blocks = provider.context_for_turn("_local", "hello").await;

        assert_eq!(blocks.len(), 2, "one CloudOk block + one LocalOnly block");
        let cloud = blocks
            .iter()
            .find(|b| b.max_tier == PrivacyTier::CloudOk)
            .expect("cloud block");
        let local = blocks
            .iter()
            .find(|b| b.max_tier == PrivacyTier::LocalOnly)
            .expect("local block");
        assert!(cloud.content.contains("likes coffee"));
        assert!(!cloud.content.contains("medical condition"));
        assert!(local.content.contains("medical condition X"));
        // Deny-on-ambiguity: NULL tier must land in the LocalOnly block, never CloudOk.
        assert!(local.content.contains("untagged legacy belief"));
        assert!(!cloud.content.contains("untagged legacy belief"));
    }

    #[tokio::test]
    async fn cap_is_respected() {
        let f = NamedTempFile::new().unwrap();
        let mem = make_memory(f.path().to_str().unwrap()).await;
        for i in 0..12 {
            store(
                &mem,
                "_local",
                &format!("fact number {i}"),
                None,
                Some(PrivacyTier::CloudOk),
            )
            .await;
        }
        let provider = MemoryRagProvider::with_max(mem, 5);
        let blocks = provider.context_for_turn("_local", "hello").await;
        assert_eq!(blocks.len(), 1);
        let bullets = blocks[0].content.matches("- [id ").count();
        assert_eq!(bullets, 5, "must inject at most max_beliefs");
    }

    #[tokio::test]
    async fn lexical_relevance_wins_over_recency_at_the_cap() {
        let f = NamedTempFile::new().unwrap();
        let mem = make_memory(f.path().to_str().unwrap()).await;
        // Relevant belief stored FIRST (oldest, lowest id)…
        store(
            &mem,
            "_local",
            "the dog is called Rex",
            None,
            Some(PrivacyTier::CloudOk),
        )
        .await;
        // …then bury it under newer irrelevant ones, past the cap.
        for i in 0..6 {
            store(
                &mem,
                "_local",
                &format!("unrelated note {i}"),
                None,
                Some(PrivacyTier::CloudOk),
            )
            .await;
        }
        let provider = MemoryRagProvider::with_max(mem, 3);
        let blocks = provider
            .context_for_turn("_local", "what is my dog called?")
            .await;
        assert_eq!(blocks.len(), 1);
        assert!(
            blocks[0].content.contains("Rex"),
            "keyword-matching belief must survive the cap: {}",
            blocks[0].content
        );
    }

    #[tokio::test]
    async fn identity_beliefs_are_excluded() {
        let f = NamedTempFile::new().unwrap();
        let mem = make_memory(f.path().to_str().unwrap()).await;
        store(
            &mem,
            "_local",
            "I am Bastion, warm and direct",
            Some("identity"),
            Some(PrivacyTier::CloudOk),
        )
        .await;
        let provider = MemoryRagProvider::new(mem);
        let blocks = provider.context_for_turn("_local", "hello").await;
        assert!(
            blocks.is_empty(),
            "identity is IdentityProvider's job — no duplication"
        );
    }

    #[tokio::test]
    async fn owner_scoping_holds() {
        let f = NamedTempFile::new().unwrap();
        let mem = make_memory(f.path().to_str().unwrap()).await;
        store(
            &mem,
            "alice",
            "alice secret",
            None,
            Some(PrivacyTier::CloudOk),
        )
        .await;
        let provider = MemoryRagProvider::new(mem);
        let blocks = provider.context_for_turn("bob", "hello").await;
        assert!(blocks.is_empty(), "bob must never see alice's beliefs");
    }
}
