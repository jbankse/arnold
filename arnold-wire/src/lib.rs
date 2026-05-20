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
}
