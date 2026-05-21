use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxEvent {
    UserMessage { session_id: Uuid, text: String },
    ClientConnected { session_id: Uuid },
    ClientDisconnected { session_id: Uuid },
}

#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: i64,
    pub event: InboxEvent,
    pub created_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct Inbox(Arc<Mutex<Connection>>);

impl Inbox {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS inbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                consumed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_inbox_unconsumed
                ON inbox(consumed_at) WHERE consumed_at IS NULL;
            "#,
        )?;
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    pub fn push(&self, event: &InboxEvent) -> Result<i64> {
        let conn = self.0.lock().unwrap();
        let kind = match event {
            InboxEvent::UserMessage { .. } => "user_message",
            InboxEvent::ClientConnected { .. } => "client_connected",
            InboxEvent::ClientDisconnected { .. } => "client_disconnected",
        };
        let payload = serde_json::to_string(event)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO inbox (kind, payload_json, created_at) VALUES (?1, ?2, ?3)",
            params![kind, payload, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn pending_for_session(&self, session_id: Uuid) -> Result<Vec<StoredEvent>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, payload_json, created_at FROM inbox
             WHERE consumed_at IS NULL ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let payload: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            Ok((id, payload, created_at))
        })?;
        let mut out = vec![];
        for r in rows {
            let (id, payload, created_at) = r?;
            let event: InboxEvent = serde_json::from_str(&payload)?;
            let session_matches = match &event {
                InboxEvent::UserMessage { session_id: s, .. } => *s == session_id,
                InboxEvent::ClientConnected { session_id: s } => *s == session_id,
                InboxEvent::ClientDisconnected { session_id: s } => *s == session_id,
            };
            if !session_matches { continue; }
            out.push(StoredEvent {
                id,
                event,
                created_at: DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
                consumed_at: None,
            });
        }
        Ok(out)
    }

    /// Return a clone of the shared connection handle so other subsystems
    /// (e.g. JobTable) can attach to the same SQLite database without opening
    /// a second file handle.
    pub fn connection(&self) -> std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        self.0.clone()
    }

    pub fn mark_consumed(&self, id: i64) -> Result<()> {
        let conn = self.0.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute("UPDATE inbox SET consumed_at = ?1 WHERE id = ?2", params![now, id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn push_pending_consume_round_trip() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("inbox.db");
        let inbox = Inbox::open(&db).unwrap();

        let sid = Uuid::new_v4();
        inbox.push(&InboxEvent::UserMessage { session_id: sid, text: "hi".into() }).unwrap();
        inbox.push(&InboxEvent::ClientConnected { session_id: sid }).unwrap();

        let pending = inbox.pending_for_session(sid).unwrap();
        assert_eq!(pending.len(), 2);

        inbox.mark_consumed(pending[0].id).unwrap();
        let remaining = inbox.pending_for_session(sid).unwrap();
        assert_eq!(remaining.len(), 1);
    }
}
