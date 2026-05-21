use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
    Exporting,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Exporting => "exporting",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(JobStatus::Queued),
            "running" => Some(JobStatus::Running),
            "exporting" => Some(JobStatus::Exporting),
            "completed" => Some(JobStatus::Completed),
            "failed" => Some(JobStatus::Failed),
            "cancelled" => Some(JobStatus::Cancelled),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled)
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub job_id: Uuid,
    pub session_id: Uuid,
    pub prompt: String,
    pub pack_kind: Option<String>,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub exported_path: Option<String>,
    pub last_log_line: Option<String>,
    pub message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct JobTable(Arc<Mutex<Connection>>);

impl JobTable {
    /// Wrap an existing connection (the same one the Inbox uses) and ensure the
    /// jobs table + indexes exist.
    pub fn attach(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        {
            let c = conn.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
            c.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS jobs (
                    job_id        TEXT PRIMARY KEY,
                    session_id    TEXT NOT NULL,
                    prompt        TEXT NOT NULL,
                    pack_kind     TEXT,
                    status        TEXT NOT NULL,
                    exit_code     INTEGER,
                    exported_path TEXT,
                    last_log_line TEXT,
                    message       TEXT,
                    started_at    TEXT NOT NULL,
                    updated_at    TEXT NOT NULL,
                    completed_at  TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_jobs_session
                    ON jobs(session_id, started_at);
                CREATE INDEX IF NOT EXISTS idx_jobs_active
                    ON jobs(status) WHERE status IN ('queued','running','exporting');
                "#,
            )?;
        }
        Ok(Self(conn))
    }

    pub fn create(&self, session_id: Uuid, prompt: &str, pack_kind: Option<&str>) -> Result<Uuid> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let job_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO jobs (job_id, session_id, prompt, pack_kind, status, started_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?5)",
            params![job_id.to_string(), session_id.to_string(), prompt, pack_kind, now],
        )?;
        Ok(job_id)
    }

    pub fn update_status(&self, job_id: Uuid, status: JobStatus, last_log_line: Option<&str>) -> Result<()> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();
        if status.is_terminal() {
            conn.execute(
                "UPDATE jobs SET status=?1, last_log_line=COALESCE(?2, last_log_line), updated_at=?3, completed_at=?3
                 WHERE job_id=?4",
                params![status.as_str(), last_log_line, now, job_id.to_string()],
            )?;
        } else {
            conn.execute(
                "UPDATE jobs SET status=?1, last_log_line=COALESCE(?2, last_log_line), updated_at=?3
                 WHERE job_id=?4",
                params![status.as_str(), last_log_line, now, job_id.to_string()],
            )?;
        }
        Ok(())
    }

    /// Set the terminal state and completion artifacts for a job.
    ///
    /// Precondition: `status` should be a terminal state
    /// (`Completed` / `Failed` / `Cancelled`). The caller is responsible
    /// for upholding this — `finalize` does not enforce it because in
    /// some error paths we want to record exit_code/message even if the
    /// status arrived in an unusual way.
    pub fn finalize(&self, job_id: Uuid, status: JobStatus, exit_code: Option<i32>, exported_path: Option<&str>, message: &str) -> Result<()> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE jobs SET status=?1, exit_code=?2, exported_path=?3, message=?4,
                              updated_at=?5, completed_at=?5
             WHERE job_id=?6",
            params![status.as_str(), exit_code, exported_path, message, now, job_id.to_string()],
        )?;
        Ok(())
    }

    pub fn get(&self, job_id: Uuid) -> Result<Option<Job>> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT job_id, session_id, prompt, pack_kind, status, exit_code,
                    exported_path, last_log_line, message, started_at, updated_at, completed_at
             FROM jobs WHERE job_id=?1",
        )?;
        let mut rows = stmt.query(params![job_id.to_string()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_job(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_active_for_session(&self, session_id: Uuid) -> Result<Vec<Job>> {
        let conn = self.0.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT job_id, session_id, prompt, pack_kind, status, exit_code,
                    exported_path, last_log_line, message, started_at, updated_at, completed_at
             FROM jobs
             WHERE session_id=?1 AND status IN ('queued','running','exporting')
             ORDER BY started_at ASC",
        )?;
        let mut rows = stmt.query(params![session_id.to_string()])?;
        let mut out = vec![];
        while let Some(row) = rows.next()? {
            out.push(row_to_job(row)?);
        }
        Ok(out)
    }
}

fn row_to_job(row: &rusqlite::Row<'_>) -> Result<Job> {
    let job_id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let status_str: String = row.get(4)?;
    let started_at: String = row.get(9)?;
    let updated_at: String = row.get(10)?;
    let completed_at: Option<String> = row.get(11)?;
    Ok(Job {
        job_id: Uuid::parse_str(&job_id)?,
        session_id: Uuid::parse_str(&session_id)?,
        prompt: row.get(2)?,
        pack_kind: row.get(3)?,
        status: JobStatus::from_str(&status_str)
            .ok_or_else(|| anyhow::anyhow!("unknown job status '{status_str}'"))?,
        exit_code: row.get(5)?,
        exported_path: row.get(6)?,
        last_log_line: row.get(7)?,
        message: row.get(8)?,
        started_at: DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)?.with_timezone(&Utc),
        completed_at: completed_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_conn(dir: &std::path::Path) -> Arc<Mutex<Connection>> {
        let conn = Connection::open(dir.join("inbox.db")).unwrap();
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn create_then_get_round_trip() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let jid = table.create(sid, "make a counter app", Some("rust_cli")).unwrap();
        let job = table.get(jid).unwrap().expect("just-created job missing");
        assert_eq!(job.job_id, jid);
        assert_eq!(job.session_id, sid);
        assert_eq!(job.prompt, "make a counter app");
        assert_eq!(job.pack_kind.as_deref(), Some("rust_cli"));
        assert_eq!(job.status, JobStatus::Queued);
        assert!(job.completed_at.is_none());
    }

    #[test]
    fn finalize_sets_completed_at() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let jid = table.create(sid, "x", None).unwrap();
        table.finalize(jid, JobStatus::Completed, Some(0), Some("/tmp/exported"), "ok").unwrap();
        let job = table.get(jid).unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(job.exit_code, Some(0));
        assert_eq!(job.exported_path.as_deref(), Some("/tmp/exported"));
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn list_active_excludes_terminal() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let a = table.create(sid, "a", None).unwrap();
        let b = table.create(sid, "b", None).unwrap();
        let c = table.create(sid, "c", None).unwrap();
        table.finalize(b, JobStatus::Completed, Some(0), None, "done").unwrap();
        table.update_status(c, JobStatus::Running, Some("npm install")).unwrap();

        let active = table.list_active_for_session(sid).unwrap();
        assert_eq!(active.len(), 2, "expected 2 active jobs (a queued, c running)");
        let ids: Vec<Uuid> = active.iter().map(|j| j.job_id).collect();
        assert!(ids.contains(&a));
        assert!(ids.contains(&c));
        assert!(!ids.contains(&b), "completed job b should be excluded from active list");
    }

    #[test]
    fn update_status_none_log_line_preserves_prior() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let jid = table.create(sid, "x", None).unwrap();
        table.update_status(jid, JobStatus::Running, Some("npm install")).unwrap();
        // Now bump status without supplying a new log line — prior must be kept.
        table.update_status(jid, JobStatus::Exporting, None).unwrap();
        let job = table.get(jid).unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Exporting);
        assert_eq!(job.last_log_line.as_deref(), Some("npm install"));
    }

    #[test]
    fn finalize_failed_records_exit_code() {
        let tmp = TempDir::new().unwrap();
        let table = JobTable::attach(open_conn(tmp.path())).unwrap();
        let sid = Uuid::new_v4();
        let jid = table.create(sid, "x", None).unwrap();
        table.finalize(jid, JobStatus::Failed, Some(1), None, "exit 1").unwrap();
        let job = table.get(jid).unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.exit_code, Some(1));
        assert!(job.exported_path.is_none());
        assert_eq!(job.message.as_deref(), Some("exit 1"));
        assert!(job.completed_at.is_some());
    }
}
