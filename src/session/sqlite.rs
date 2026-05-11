use crate::types::{Message, MessageContent, Role, BastionError};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SessionManager {
    db_path: String,
}

impl SessionManager {
    pub fn new(db_path: impl Into<String>) -> Self {
        Self { db_path: db_path.into() }
    }

    pub async fn init_schema(&self) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("
                PRAGMA journal_mode=WAL;

                CREATE TABLE IF NOT EXISTS sessions (
                    id         TEXT    PRIMARY KEY,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS messages (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id  TEXT    NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                    role        TEXT    NOT NULL,
                    content     TEXT    NOT NULL,
                    tokens_used INTEGER,
                    created_at  INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_messages_session
                    ON messages(session_id, created_at);

                CREATE TABLE IF NOT EXISTS budget (
                    date      TEXT PRIMARY KEY,
                    total_usd REAL NOT NULL DEFAULT 0.0
                );
            ")?;
            Ok::<_, anyhow::Error>(())
        }).await?
    }

    pub async fn create_session(&self) -> anyhow::Result<String> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let now: i64 = now_nanos();
            // Use nanosecond timestamp as session ID — unique enough for single-user daemon
            let session_id = now.to_string();
            conn.execute(
                "INSERT INTO sessions (id, created_at, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![session_id, now, now],
            )?;
            Ok::<_, anyhow::Error>(session_id)
        }).await?
    }

    pub async fn load_most_recent_id(&self) -> anyhow::Result<Option<String>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let mut stmt = conn.prepare(
                "SELECT id FROM sessions ORDER BY updated_at DESC LIMIT 1"
            )?;
            let mut rows = stmt.query([])?;
            if let Some(row) = rows.next()? {
                Ok::<_, anyhow::Error>(Some(row.get::<_, String>(0)?))
            } else {
                Ok(None)
            }
        }).await?
    }

    pub async fn load_recent(&self, session_id: &str) -> anyhow::Result<Vec<Message>> {
        let path = self.db_path.clone();
        let sid = session_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let mut stmt = conn.prepare(
                "SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY created_at ASC"
            )?;
            let messages: Vec<Message> = stmt
                .query_map(rusqlite::params![sid], |row| {
                    let role_str: String = row.get(0)?;
                    let content_str: String = row.get(1)?;
                    Ok((role_str, content_str))
                })?
                .map(|r| -> anyhow::Result<Message> {
                    let (role_str, content_str) = r?;
                    let role: Role = role_str.parse()?;
                    let content: MessageContent = serde_json::from_str(&content_str)
                        .map_err(|e| anyhow::anyhow!("failed to parse content: {}", e))?;
                    Ok(Message { role, content })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok::<_, anyhow::Error>(messages)
        }).await?
    }

    pub async fn append(
        &self,
        session_id: &str,
        msg: Message,
        tokens_used: Option<u32>,
    ) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        let sid = session_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;

            // Enforce role sequence integrity: Tool message must follow Assistant
            if msg.role == Role::Tool {
                let mut stmt = conn.prepare(
                    "SELECT role FROM messages WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 1"
                )?;
                let mut rows = stmt.query(rusqlite::params![sid])?;
                let preceding_role = rows.next()?
                    .map(|r| r.get::<_, String>(0))
                    .transpose()?;

                match preceding_role.as_deref() {
                    Some("assistant") => {}, // OK
                    _ => return Err(anyhow::anyhow!(BastionError::OrphanedToolResult)),
                }
            }

            let now = now_nanos();
            let role_str = msg.role.to_string();
            let content_str = serde_json::to_string(&msg.content)
                .map_err(|e| anyhow::anyhow!("failed to serialize content: {}", e))?;

            conn.execute(
                "INSERT INTO messages (session_id, role, content, tokens_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![sid, role_str, content_str, tokens_used, now],
            )?;

            conn.execute(
                "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, sid],
            )?;

            Ok::<_, anyhow::Error>(())
        }).await?
    }

    pub async fn replace_with_summary(
        &self,
        session_id: &str,
        summary: String,
        recent: &[Message],
    ) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        let sid = session_id.to_owned();
        let recent = recent.to_vec();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;

            // Delete all old messages for this session
            conn.execute(
                "DELETE FROM messages WHERE session_id = ?1",
                rusqlite::params![sid],
            )?;

            let now: i64 = now_nanos();

            // Insert summary as system message
            let summary_content = serde_json::to_string(&MessageContent::Text(summary))
                .map_err(|e| anyhow::anyhow!("failed to serialize summary: {}", e))?;
            conn.execute(
                "INSERT INTO messages (session_id, role, content, tokens_used, created_at) VALUES (?1, 'system', ?2, NULL, ?3)",
                rusqlite::params![sid, summary_content, now],
            )?;

            // Insert recent messages in order
            for (i, msg) in recent.iter().enumerate() {
                let ts: i64 = now + (i as i64) + 1;
                let role_str = msg.role.to_string();
                let content_str = serde_json::to_string(&msg.content)
                    .map_err(|e| anyhow::anyhow!("failed to serialize content: {}", e))?;
                conn.execute(
                    "INSERT INTO messages (session_id, role, content, tokens_used, created_at) VALUES (?1, ?2, ?3, NULL, ?4)",
                    rusqlite::params![sid, role_str, content_str, ts],
                )?;
            }

            conn.execute(
                "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, sid],
            )?;

            Ok::<_, anyhow::Error>(())
        }).await?
    }

    pub async fn update_budget(&self, cost_usd: f64) -> anyhow::Result<()> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let today = today_utc();
            conn.execute(
                "INSERT INTO budget(date, total_usd) VALUES(?1, ?2) \
                 ON CONFLICT(date) DO UPDATE SET total_usd = total_usd + ?2",
                rusqlite::params![today, cost_usd],
            )?;
            Ok::<_, anyhow::Error>(())
        }).await?
    }

    pub async fn check_budget(&self, daily_limit: f64) -> anyhow::Result<bool> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            let today = today_utc();
            let mut stmt = conn.prepare(
                "SELECT total_usd FROM budget WHERE date = ?1"
            )?;
            let mut rows = stmt.query(rusqlite::params![today])?;
            if let Some(row) = rows.next()? {
                let total: f64 = row.get(0)?;
                Ok::<_, anyhow::Error>(total < daily_limit)
            } else {
                Ok(true) // no spend today
            }
        }).await?
    }
}

fn now_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

fn today_utc() -> String {
    // Simple UTC date — seconds since epoch / 86400 → days since epoch
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    // days since 1970-01-01 → year/month/day
    // Use a simple calculation (no chrono dependency)
    let (y, m, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let leap = is_leap(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}
