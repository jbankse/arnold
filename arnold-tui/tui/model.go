package tui

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/google/uuid"

	"github.com/jbankse/arnold/arnold-tui/client"
	"github.com/jbankse/arnold/arnold-tui/wire"
)

// Conn state for the top-of-screen status badge.
type ConnState int

const (
	ConnConnecting ConnState = iota
	ConnConnected
	ConnDisconnected
)

// MessageRow is one rendered line in the conversation pane.
type MessageRow struct {
	Author string // "you" | "arnold" | "system"
	Text   string
}

// Job is a row in the jobs pane.
type Job struct {
	ID            string
	PromptPreview string
	Status        string
	LastLogLine   string
	Completed     bool
	Exported      string
}

// Model is the root bubbletea state.
type Model struct {
	cli       *client.Client
	connState ConnState
	sessionID *uuid.UUID

	focus    PaneID
	mode     Mode
	helpOpen bool
	input    string

	conversation []MessageRow // append-only log
	inboxEvents  []string     // pre-rendered strings
	jobs         map[string]*Job
	jobOrder     []string

	cost   CostState
	width  int
	height int
}

type CostState struct {
	SessionUSD  float64
	LastTurnUSD float64
	Provider    string
	Model       string
}

// New returns the initial Model. Caller must call .Run() to enter the bubbletea loop.
func New(cli *client.Client) *Model {
	return &Model{
		cli:       cli,
		connState: ConnConnecting,
		focus:     PaneConversation,
		mode:      ModeNormal,
		jobs:      make(map[string]*Job),
	}
}

// daemonEventMsg wraps a single DaemonEvent for the bubbletea Update loop.
type daemonEventMsg wire.DaemonEvent

// daemonClosedMsg fires when the events channel closes (daemon hung up).
type daemonClosedMsg struct{}

// listenForEvents returns a tea.Cmd that pulls the next event from the client.
func (m *Model) listenForEvents() tea.Cmd {
	return func() tea.Msg {
		ev, ok := <-m.cli.Events()
		if !ok {
			return daemonClosedMsg{}
		}
		return daemonEventMsg(ev)
	}
}

// Init opens a session against the current working directory.
func (m *Model) Init() tea.Cmd {
	cwd, err := os.Getwd()
	if err != nil {
		cwd = "/"
	}
	if err := m.cli.Send(wire.OpenSession(cwd)); err != nil {
		// Defer the error to a tea.Msg
		return func() tea.Msg { return daemonClosedMsg{} }
	}
	return m.listenForEvents()
}

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		return m, nil
	case tea.KeyMsg:
		return m.rootKeybindings(msg)
	case daemonEventMsg:
		m.applyEvent(wire.DaemonEvent(msg))
		return m, m.listenForEvents()
	case daemonClosedMsg:
		m.connState = ConnDisconnected
		return m, nil
	}
	return m, nil
}

// applyEvent mutates state based on a daemon event.
func (m *Model) applyEvent(ev wire.DaemonEvent) {
	switch ev.Type {
	case "session_opened":
		m.sessionID = ev.SessionID
		m.connState = ConnConnected
	case "reply":
		m.conversation = append(m.conversation, MessageRow{Author: "arnold", Text: ev.Text})
	case "turn_complete":
		// no-op for the conversation log; status bar may pulse
	case "error":
		m.conversation = append(m.conversation, MessageRow{Author: "system", Text: "[error] " + ev.Message})
	case "job_spawned":
		if ev.JobID == nil {
			return
		}
		id := ev.JobID.String()
		m.jobs[id] = &Job{ID: id, PromptPreview: ev.PromptPreview, Status: "running"}
		m.jobOrder = append(m.jobOrder, id)
		m.inboxEvents = append(m.inboxEvents, fmt.Sprintf("job spawned: %s", id[:8]))
	case "job_update":
		if ev.JobID == nil {
			return
		}
		id := ev.JobID.String()
		if j, ok := m.jobs[id]; ok {
			j.Status = ev.Status
			if ev.LastLogLine != nil {
				j.LastLogLine = *ev.LastLogLine
			}
		}
	case "job_completed":
		if ev.JobID == nil {
			return
		}
		id := ev.JobID.String()
		if j, ok := m.jobs[id]; ok {
			j.Status = ev.Status
			j.Completed = true
			if ev.ExportedWorkspace != nil {
				j.Exported = *ev.ExportedWorkspace
			}
		}
		summary := fmt.Sprintf("job %s completed (%s)", id[:8], ev.Status)
		if ev.ExportedWorkspace != nil {
			summary += " → " + filepath.Base(*ev.ExportedWorkspace)
		}
		m.inboxEvents = append(m.inboxEvents, summary)
	case "cost_update":
		m.cost = CostState{
			SessionUSD:  ev.SessionCostUSD,
			LastTurnUSD: ev.LastTurnCostUSD,
			Provider:    ev.LastTurnProvider,
			Model:       ev.LastTurnModel,
		}
	case "notification":
		m.inboxEvents = append(m.inboxEvents, "[notify] "+ev.Title+": "+ev.Body)
	}
}

func (m *Model) sendUserMessage(text string) tea.Cmd {
	if m.sessionID == nil || strings.TrimSpace(text) == "" {
		return nil
	}
	m.conversation = append(m.conversation, MessageRow{Author: "you", Text: text})
	if err := m.cli.Send(wire.UserMessage(*m.sessionID, text)); err != nil {
		m.conversation = append(m.conversation, MessageRow{Author: "system", Text: "[error] send failed: " + err.Error()})
	}
	return nil
}

// Run starts the bubbletea program. Returns when the user quits or the daemon disconnects.
func (m *Model) Run(_ context.Context) error {
	p := tea.NewProgram(m, tea.WithAltScreen())
	_, err := p.Run()
	return err
}

func (m *Model) View() string {
	if m.helpOpen {
		return renderHelp()
	}
	return fmt.Sprintf("arnold tui (focus=%s, mode=%d, conn=%d, sid=%v)\n%d messages | %d events | %d jobs\n[i] insert  [tab] switch pane  [?] help  [q] quit\n",
		m.focus, m.mode, m.connState, m.sessionID,
		len(m.conversation), len(m.inboxEvents), len(m.jobs),
	)
}

func renderHelp() string {
	return "Help: see Task 18 for the real help screen\n"
}
