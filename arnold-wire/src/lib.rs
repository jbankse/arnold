use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client → Daemon: anything the CLI client can send.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    /// Open a new session against a given working directory.
    OpenSession { cwd: String },
    /// User typed something in the current session.
    UserMessage { session_id: Uuid, text: String },
    /// Cleanly close the current session.
    CloseSession { session_id: Uuid },
    /// Lightweight health check.
    Ping,
    /// Set or clear an LLM provider API key. `provider` is the short name
    /// (`anthropic`, `openai`, `xai`, …). Empty `key` clears the entry.
    /// The daemon persists to ~/.arnold/secrets.toml (chmod 600) and picks up
    /// the new value on the next cpu spawn — no daemon restart needed.
    SetProviderKey { provider: String, key: String },
    /// Ask the daemon which provider keys are currently set. Returns a
    /// SecretsStatus event listing each provider and whether its key is
    /// non-empty (the actual key value is never sent back over the wire).
    GetSecretsStatus,
}

/// Daemon → Client: anything the daemon can push back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    SessionOpened { session_id: Uuid },
    /// Arnold's `sys_reply` text routed to this client.
    Reply { session_id: Uuid, text: String },
    /// Arnold finished its turn and is waiting on the inbox.
    TurnComplete { session_id: Uuid },
    /// Non-fatal error message for the user.
    Error { session_id: Option<Uuid>, message: String },
    Pong,
    /// A 439 background job has started.
    JobSpawned {
        session_id: Uuid,
        job_id: Uuid,
        /// Truncated preview of the LLM-supplied prompt; daemon trims to ~80 chars.
        prompt_preview: String,
    },
    /// Heartbeat update for an in-flight 439 job.
    JobUpdate {
        session_id: Uuid,
        job_id: Uuid,
        /// Closed set: `queued | running | exporting | completed | failed | cancelled`.
        status: String,
        /// Most recent BIOS stdout line for liveness; `None` if no output yet.
        last_log_line: Option<String>,
    },
    /// Terminal event for a 439 job.
    JobCompleted {
        session_id: Uuid,
        job_id: Uuid,
        /// Closed set: `queued | running | exporting | completed | failed | cancelled`.
        status: String,
        exit_code: Option<i32>,
        exported_workspace: Option<String>,
        message: String,
    },
    /// Per-turn cost telemetry derived from cpu's `usage` up-frame.
    CostUpdate {
        session_id: Uuid,
        session_cost_usd: f64,
        last_turn_cost_usd: f64,
        last_turn_provider: String,
        last_turn_model: String,
    },
    /// Arnold dispatched an OS notification via `sys_send_notification`.
    Notification {
        session_id: Uuid,
        title: String,
        body: String,
        /// Closed set: `low | normal | critical`.
        urgency: String,
    },
    /// Response to GetSecretsStatus or SetProviderKey. Lists each provider
    /// the daemon knows about and whether its key is non-empty. Values are
    /// never echoed back — the TUI only needs to render "set" vs "not set".
    SecretsStatus { providers: Vec<ProviderStatus> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderStatus {
    pub provider: String,
    pub configured: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_request_round_trip() {
        let req = ClientRequest::UserMessage {
            session_id: Uuid::nil(),
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: ClientRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);
        assert!(json.contains("\"type\":\"user_message\""));
    }

    #[test]
    fn daemon_event_round_trip() {
        let ev = DaemonEvent::Reply {
            session_id: Uuid::nil(),
            text: "hi".to_string(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"reply\""));
    }

    #[test]
    fn job_spawned_round_trip() {
        let ev = DaemonEvent::JobSpawned {
            session_id: Uuid::nil(),
            job_id: Uuid::nil(),
            prompt_preview: "make a counter app".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"job_spawned\""));
    }

    #[test]
    fn job_completed_round_trip() {
        let ev = DaemonEvent::JobCompleted {
            session_id: Uuid::nil(),
            job_id: Uuid::nil(),
            status: "completed".into(),
            exit_code: Some(0),
            exported_workspace: Some("/Users/joshua/.arnold/jobs/abc/workspace".into()),
            message: "runtime completed successfully".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"job_completed\""));
    }

    #[test]
    fn cost_update_round_trip() {
        // Values chosen to round-trip exactly through JSON (no f64 precision loss).
        let ev = DaemonEvent::CostUpdate {
            session_id: Uuid::nil(),
            session_cost_usd: 0.0123,
            last_turn_cost_usd: 0.0042,
            last_turn_provider: "anthropic".into(),
            last_turn_model: "claude-sonnet-4-6".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"cost_update\""));
    }

    #[test]
    fn notification_round_trip() {
        let ev = DaemonEvent::Notification {
            session_id: Uuid::nil(),
            title: "Job complete".into(),
            body: "Counter app build succeeded".into(),
            urgency: "normal".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, parsed);
        assert!(json.contains("\"type\":\"notification\""));
    }

    #[test]
    fn set_provider_key_round_trip() {
        let req = ClientRequest::SetProviderKey { provider: "anthropic".into(), key: "sk-x".into() };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"set_provider_key\""));
        let back: ClientRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn secrets_status_round_trip() {
        let ev = DaemonEvent::SecretsStatus {
            providers: vec![
                ProviderStatus { provider: "anthropic".into(), configured: true },
                ProviderStatus { provider: "openai".into(), configured: false },
            ],
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"secrets_status\""));
        let back: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back);
    }
}
