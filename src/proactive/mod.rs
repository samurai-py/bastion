//! CronService: heartbeat + event + idle proactive message queue (PROACT-01 – PROACT-04).
//!
//! # Design (02-RESEARCH §Code Examples, Pitfall 3)
//! Uses `tokio::time::interval` with `MissedTickBehavior::Skip` — never `Runtime::new` or
//! `block_on` inside a callback (T-02-29 nested-runtime anti-pattern).
//!
//! # PROACT-05 guarantee
//! Messages are enqueued into `pending_tx`. The daemon's `select!` arm drains `pending_rx`
//! only BETWEEN turns (structural: select! processes one branch per iteration, and run_turn
//! fully awaits). This module does NOT enforce that — it only enqueues.

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};

use crate::goal::GoalEngine;

/// Proactive message queue producer.
///
/// Multiple methods can send messages into `pending_tx`; the daemon selects from
/// `pending_rx` only between turns (PROACT-05 structural guarantee).
pub struct CronService {
    pending_tx: mpsc::Sender<String>,
    goals: GoalEngine,
}

impl CronService {
    /// Create a new CronService bound to `pending_tx` and a `GoalEngine`.
    ///
    /// No background tasks are spawned here — callers decide when to activate each job.
    /// NO nested runtime (T-02-29): all methods are async and must be `.await`-ed.
    pub fn new(pending_tx: mpsc::Sender<String>, goals: GoalEngine) -> Self {
        Self { pending_tx, goals }
    }

    /// PROACT-01 / PROACT-02: heartbeat ticker.
    ///
    /// Ticks on `period` (recommended: 24h in production; short in tests).
    /// Uses `MissedTickBehavior::Skip` so a slow turn never causes a burst of ticks
    /// when the daemon was busy (Pitfall 3 from 02-RESEARCH).
    ///
    /// On each tick: fetches the first goal for `owner` and sends its drift nudge
    /// into `pending_tx` if `drift_nudge` returns `Some(text)` (GOAL-03).
    /// Silently skips if no goals exist or `drift_nudge` returns `None`.
    ///
    /// This method loops forever — callers must `tokio::spawn` it and cancel via
    /// task abortion when the daemon shuts down.
    pub async fn run_heartbeat(&self, period: Duration, owner: &str) {
        let mut iv = interval(period);
        iv.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            iv.tick().await;

            // List goals; silently skip on error
            let goals = match self.goals.list_goals(owner).await {
                Ok(g) => g,
                Err(e) => {
                    tracing::warn!(event = "heartbeat_list_goals_error", error = %e);
                    continue;
                }
            };

            for goal in &goals {
                match self.goals.drift_nudge(owner, goal.id).await {
                    Ok(Some(text)) => {
                        if self.pending_tx.send(text).await.is_err() {
                            tracing::warn!(event = "heartbeat_pending_tx_closed");
                            return; // channel closed → daemon is shutting down
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(event = "heartbeat_drift_nudge_error", error = %e);
                    }
                }
            }
        }
    }

    /// PROACT-03: on-demand event trigger.
    ///
    /// Enqueues a proactive message for an external event (e.g. webhook/calendar payload).
    /// Fire-and-forget: if `pending_tx` is closed the error is silently swallowed.
    pub async fn on_event(&self, event_text: String) {
        if self.pending_tx.send(event_text).await.is_err() {
            tracing::warn!(event = "on_event_pending_tx_closed");
        }
    }

    /// PROACT-04: idle distillation trigger.
    ///
    /// When called after an idle period, runs `dream.extract_facts` on `recent` messages
    /// and persists each fact as a belief via `memory` (MEM-05).
    ///
    /// Optionally enqueues a follow-up nudge into `pending_tx` after storing beliefs.
    /// Errors are logged and swallowed — idle failures must not abort the daemon.
    pub async fn idle_tick(
        &self,
        dream: &dyn crate::agent::dream::Dream,
        recent: &[crate::types::Message],
        memory: &crate::memory::SharedMemory,
        owner: &str,
    ) {
        let facts = match dream.extract_facts(recent).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(event = "idle_tick_extract_error", error = %e);
                return;
            }
        };

        if facts.is_empty() {
            return;
        }

        let mem = memory.read().await;
        let mut stored = 0usize;
        for fact in &facts {
            match mem
                .store_belief(owner, None, fact, "idle_dream", "dream", false, None)
                .await
            {
                Ok(_) => stored += 1,
                Err(e) => tracing::warn!(event = "idle_tick_store_error", error = %e),
            }
        }

        tracing::info!(event = "idle_tick_complete", owner, stored);
    }
}

// ---------------------------------------------------------------------------
// Tests (offline — short interval, bounded pending_rx; MockDream; temp DB)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::dream::Dream;
    use crate::goal::ScoringConfig;
    use crate::memory::sqlite::SqliteMemory;
    use crate::memory::Memory;
    use crate::types::Message;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::sync::RwLock;

    // --- MockDream: returns scripted facts ---

    struct MockDream {
        facts: Vec<String>,
    }

    #[async_trait]
    impl Dream for MockDream {
        async fn extract_facts(&self, _: &[Message]) -> anyhow::Result<Vec<String>> {
            Ok(self.facts.clone())
        }

        async fn consolidate(
            &self,
            _: &[crate::memory::Belief],
        ) -> anyhow::Result<crate::agent::dream::ConsolidationPlan> {
            Ok(crate::agent::dream::ConsolidationPlan::default())
        }
    }

    async fn setup_db(path: &str) {
        let sm = crate::session::SessionManager::new(path);
        sm.init_schema().await.expect("init_schema");
    }

    // -----------------------------------------------------------------------
    // Heartbeat: ticks at very short interval and enqueues ≥1 message.
    // We create a goal with interactions above threshold so drift_nudge returns Some.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn heartbeat_enqueues_at_least_one_message() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        setup_db(&path).await;

        let sm = crate::session::SessionManager::new(&path);
        let engine = GoalEngine::new(
            &path,
            ScoringConfig {
                window_days: 7,
                progress_threshold: 1,
            },
        );

        // Create a goal
        let _goal_id = engine
            .create_goal("_local", "exercise daily", None, None, None)
            .await
            .expect("create_goal");

        // Insert enough matching messages to hit threshold=1
        let sid = sm.create_session_for("_local").await.expect("session");
        insert_raw_message(&path, &sid, "I exercise every day").await;

        let (tx, mut rx) = mpsc::channel::<String>(16);
        let svc = CronService::new(tx, engine);

        // Spawn heartbeat with a very short interval (10ms)
        let handle = tokio::spawn(async move {
            svc.run_heartbeat(Duration::from_millis(10), "_local").await;
        });

        // Wait up to 500ms to receive at least one message
        let msg = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("timeout waiting for heartbeat message")
            .expect("channel closed");

        handle.abort();

        assert!(
            !msg.is_empty(),
            "heartbeat message must not be empty; got: {msg:?}"
        );
    }

    // -----------------------------------------------------------------------
    // on_event: sends an event text into the channel immediately.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn on_event_enqueues_message() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        setup_db(&path).await;

        let engine = GoalEngine::new(&path, ScoringConfig::default());
        let (tx, mut rx) = mpsc::channel::<String>(16);
        let svc = CronService::new(tx, engine);

        svc.on_event("calendar: meeting in 10 minutes".to_string())
            .await;

        let msg = rx.recv().await.expect("message expected");
        assert_eq!(msg, "calendar: meeting in 10 minutes");
    }

    // -----------------------------------------------------------------------
    // idle_tick: MockDream returns scripted facts → stored as beliefs in temp DB.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn idle_tick_stores_beliefs_in_db() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        setup_db(&path).await;

        let memory: crate::memory::SharedMemory = Arc::new(RwLock::new(
            Box::new(SqliteMemory::new(&path)) as Box<dyn Memory>,
        ));

        let engine = GoalEngine::new(&path, ScoringConfig::default());
        let (tx, _rx) = mpsc::channel::<String>(16);
        let svc = CronService::new(tx, engine);

        let dream = MockDream {
            facts: vec![
                "Mario exercises every morning".to_string(),
                "Mario drinks coffee".to_string(),
            ],
        };

        let messages: Vec<Message> = vec![]; // unused by MockDream

        svc.idle_tick(&dream, &messages, &memory, "_local").await;

        let beliefs = {
            let m = memory.read().await;
            m.retrieve_tagged("_local", None).await.expect("retrieve")
        };

        assert_eq!(
            beliefs.len(),
            2,
            "idle_tick must store all 2 facts; got {}",
            beliefs.len()
        );
    }

    // -----------------------------------------------------------------------
    // Helper: insert a raw message directly into SQLite (bypasses Role parsing)
    // -----------------------------------------------------------------------

    async fn insert_raw_message(db_path: &str, session_id: &str, content: &str) {
        let path = db_path.to_owned();
        let sid = session_id.to_owned();
        let content = content.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64;
            conn.execute(
                "INSERT INTO messages (session_id, role, content, created_at) VALUES (?1, 'user', ?2, ?3)",
                rusqlite::params![sid, content, now],
            ).unwrap();
        })
        .await
        .unwrap();
    }
}
