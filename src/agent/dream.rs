use crate::types::Message;
use crate::memory::SharedMemory;

/// Dream extracts durable facts from idle session history.
#[async_trait::async_trait]
pub trait Dream: Send + Sync {
    async fn extract_facts(&self, messages: &[Message]) -> anyhow::Result<Vec<String>>;
}

/// No-op implementation — returns no facts.
pub struct NoDream;

#[async_trait::async_trait]
impl Dream for NoDream {
    async fn extract_facts(&self, _messages: &[Message]) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
}

/// Heuristic dream implementation: extracts facts by finding user messages that
/// assert statements about the owner (simple keyword-based heuristic, offline, zero LLM).
///
/// This is the Phase-2 "scripted" implementation. A real LLM-backed variant can be
/// swapped in by implementing the Dream trait with an LLM call.
pub struct HeuristicDream;

#[async_trait::async_trait]
impl Dream for HeuristicDream {
    async fn extract_facts(&self, messages: &[Message]) -> anyhow::Result<Vec<String>> {
        use crate::types::{MessageContent, Role};

        let mut facts = Vec::new();
        for msg in messages {
            if msg.role != Role::User {
                continue;
            }
            // Extract text content
            let text = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => {
                    parts.iter().filter_map(|p| {
                        if let crate::types::ContentPart::Text { text } = p {
                            Some(text.clone())
                        } else {
                            None
                        }
                    }).collect::<Vec<_>>().join(" ")
                }
            };

            // Simple heuristic: user messages that look like factual self-disclosures.
            // Triggers on "I am", "I have", "I like", "I work", "I live", "meu", "eu sou", "eu tenho".
            let lower = text.to_lowercase();
            let triggers = [
                "i am ", "i have ", "i like ", "i work ", "i live ",
                "eu sou ", "eu tenho ", "eu gosto ", "eu trabalho ", "eu moro ",
                "meu ", "minha ", "my ",
            ];
            if triggers.iter().any(|t| lower.contains(t)) && text.len() > 10 {
                facts.push(text.trim().to_string());
            }
        }
        Ok(facts)
    }
}

/// MEM-09: memory_flush — distil recent messages to beliefs and persist them.
///
/// Runs BEFORE compaction is invoked in run_turn (loop_.rs compaction branch).
/// Uses HeuristicDream so it is always offline and never makes LLM calls.
///
/// Errors are logged and silently swallowed — flush failure must not abort the turn.
pub async fn memory_flush(messages: &[Message], memory: &SharedMemory, owner: &str) {
    let dream = HeuristicDream;
    let facts = match dream.extract_facts(messages).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(event = "memory_flush_extract_error", error = %e);
            return;
        }
    };

    if facts.is_empty() {
        return;
    }

    let mem = memory.read().await;
    for fact in &facts {
        if let Err(e) = mem
            .store_belief(owner, None, fact, "dream_flush", "dream", false)
            .await
        {
            tracing::warn!(event = "memory_flush_store_error", error = %e);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (offline, temp-DB — no LLM)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::sqlite::SqliteMemory;
    use crate::memory::Memory;
    use crate::types::{MessageContent, Role};
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;

    fn user_msg(text: &str) -> Message {
        Message {
            role: Role::User,
            content: MessageContent::Text(text.to_string()),
        }
    }

    fn assistant_msg(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: MessageContent::Text(text.to_string()),
        }
    }

    async fn make_memory(db_path: &str) -> SharedMemory {
        let session = crate::session::SessionManager::new(db_path);
        session.init_schema().await.expect("init_schema");
        Arc::new(RwLock::new(Box::new(SqliteMemory::new(db_path)) as Box<dyn Memory>))
    }

    #[tokio::test]
    async fn no_dream_returns_empty() {
        let dream = NoDream;
        let messages = vec![user_msg("I am a developer"), assistant_msg("Got it!")];
        let facts = dream.extract_facts(&messages).await.expect("extract_facts");
        assert!(facts.is_empty(), "NoDream must always return empty");
    }

    #[tokio::test]
    async fn heuristic_dream_extracts_self_disclosure() {
        let dream = HeuristicDream;
        let messages = vec![
            user_msg("I am a software developer living in Brazil"),
            assistant_msg("That's great!"),
            user_msg("what is the weather?"),  // no trigger → not extracted
        ];
        let facts = dream.extract_facts(&messages).await.expect("extract_facts");
        assert_eq!(facts.len(), 1, "should extract exactly the self-disclosure message");
        assert!(facts[0].contains("developer"), "fact: {}", facts[0]);
    }

    #[tokio::test]
    async fn memory_flush_stores_beliefs_in_temp_db() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        let mem = make_memory(&path).await;

        let messages = vec![
            user_msg("I have a dog named Rex"),
            user_msg("Eu gosto de café pela manhã"),
            assistant_msg("Nice to know!"),
            user_msg("what's the weather?"),  // no trigger
        ];

        memory_flush(&messages, &mem, "_local").await;

        let beliefs = {
            let m = mem.read().await;
            m.retrieve_tagged("_local", None).await.expect("retrieve")
        };

        assert!(beliefs.len() >= 1, "at least 1 belief must be stored; got {}", beliefs.len());
        let contents: Vec<&str> = beliefs.iter().map(|b| b.content.as_str()).collect();
        assert!(
            contents.iter().any(|c| c.contains("dog") || c.contains("Rex") || c.contains("café")),
            "expected a belief about dog or café; got: {:?}", contents
        );
    }
}
