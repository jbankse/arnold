package wire

import (
	"encoding/json"

	"github.com/google/uuid"
)

// ClientRequest is the tagged union sent from CLI to daemon.
// Mirrors arnold-wire/src/lib.rs's ClientRequest enum with snake_case tags.
type ClientRequest struct {
	Type      string     `json:"type"` // one of: open_session, user_message, close_session, ping
	Cwd       string     `json:"cwd,omitempty"`
	SessionID *uuid.UUID `json:"session_id,omitempty"`
	Text      string     `json:"text,omitempty"`
}

// Constructors avoid stringly-typed Type fields at call sites.

func OpenSession(cwd string) ClientRequest {
	return ClientRequest{Type: "open_session", Cwd: cwd}
}
func UserMessage(sessionID uuid.UUID, text string) ClientRequest {
	return ClientRequest{Type: "user_message", SessionID: &sessionID, Text: text}
}
func CloseSession(sessionID uuid.UUID) ClientRequest {
	return ClientRequest{Type: "close_session", SessionID: &sessionID}
}
func Ping() ClientRequest {
	return ClientRequest{Type: "ping"}
}

// DaemonEvent is the union from daemon to CLI. Type-tagged like ClientRequest.
type DaemonEvent struct {
	Type      string     `json:"type"`
	SessionID *uuid.UUID `json:"session_id,omitempty"`

	// Reply / TurnComplete / Error
	Text    string `json:"text,omitempty"`
	Message string `json:"message,omitempty"`

	// JobSpawned / JobUpdate / JobCompleted
	JobID             *uuid.UUID `json:"job_id,omitempty"`
	PromptPreview     string     `json:"prompt_preview,omitempty"`
	Status            string     `json:"status,omitempty"`
	LastLogLine       *string    `json:"last_log_line,omitempty"`
	ExitCode          *int       `json:"exit_code,omitempty"`
	ExportedWorkspace *string    `json:"exported_workspace,omitempty"`

	// CostUpdate
	SessionCostUSD   float64 `json:"session_cost_usd,omitempty"`
	LastTurnCostUSD  float64 `json:"last_turn_cost_usd,omitempty"`
	LastTurnProvider string  `json:"last_turn_provider,omitempty"`
	LastTurnModel    string  `json:"last_turn_model,omitempty"`

	// Notification
	Title   string `json:"title,omitempty"`
	Body    string `json:"body,omitempty"`
	Urgency string `json:"urgency,omitempty"`
}

// DecodeEvent reads one JSON-line frame.
func DecodeEvent(b []byte) (DaemonEvent, error) {
	var ev DaemonEvent
	err := json.Unmarshal(b, &ev)
	return ev, err
}

// Encode writes one JSON-line frame (caller appends \n).
func (r ClientRequest) Encode() ([]byte, error) {
	return json.Marshal(r)
}
