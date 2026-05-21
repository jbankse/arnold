package tui

import (
	tea "github.com/charmbracelet/bubbletea"

	"github.com/jbankse/arnold/arnold-tui/wire"
)

// PaneID identifies which pane has focus.
type PaneID int

const (
	PaneConversation PaneID = iota
	PaneInbox
	PaneJobs
)

func (p PaneID) String() string {
	switch p {
	case PaneConversation:
		return "conversation"
	case PaneInbox:
		return "inbox"
	case PaneJobs:
		return "jobs"
	}
	return "?"
}

// Mode is vim-ish: Normal navigates, Insert types into the input buffer.
type Mode int

const (
	ModeNormal Mode = iota
	ModeInsert
)

// nextPane / prevPane wrap around the 3 panes.
func nextPane(p PaneID) PaneID {
	switch p {
	case PaneConversation:
		return PaneInbox
	case PaneInbox:
		return PaneJobs
	case PaneJobs:
		return PaneConversation
	}
	return PaneConversation
}

func prevPane(p PaneID) PaneID {
	switch p {
	case PaneConversation:
		return PaneJobs
	case PaneInbox:
		return PaneConversation
	case PaneJobs:
		return PaneInbox
	}
	return PaneConversation
}

// rootKeybindings handles global keys (mode changes, pane focus, quit, help,
// settings overlay). Pane-specific keys are handled in each pane's Update.
func (m *Model) rootKeybindings(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	// Settings overlay takes input priority when open.
	if m.settingsOpen {
		return m.settingsKeybindings(msg)
	}
	if m.mode == ModeInsert {
		switch msg.Type {
		case tea.KeyEsc:
			m.mode = ModeNormal
			return m, nil
		case tea.KeyEnter:
			text := m.input
			m.input = ""
			m.mode = ModeNormal
			return m, m.sendUserMessage(text)
		case tea.KeyBackspace:
			if len(m.input) > 0 {
				m.input = m.input[:len(m.input)-1]
			}
			return m, nil
		default:
			if msg.Type == tea.KeyRunes {
				m.input += string(msg.Runes)
			}
			return m, nil
		}
	}
	// Normal mode
	switch msg.String() {
	case "q", "ctrl+c":
		return m, tea.Quit
	case "i":
		if m.focus == PaneConversation {
			m.mode = ModeInsert
		}
		return m, nil
	case "tab":
		m.focus = nextPane(m.focus)
		return m, nil
	case "shift+tab":
		m.focus = prevPane(m.focus)
		return m, nil
	case "?":
		m.helpOpen = !m.helpOpen
		return m, nil
	case "ctrl+s":
		m.settingsOpen = true
		// Refresh statuses while we're at it — the daemon could have been
		// edited externally (config.toml hand-edit).
		_ = m.cli.Send(wire.GetSecretsStatus())
		return m, nil
	}
	return m, nil
}

// settingsKeybindings drives the Ctrl+S overlay. Two sub-modes:
//   - browsing: j/k or arrows to move cursor; Enter to start editing; Esc/Ctrl+S close.
//   - editing : Enter saves to the daemon (which persists secrets.toml); Esc cancels.
func (m *Model) settingsKeybindings(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if m.editingKey {
		switch msg.Type {
		case tea.KeyEsc:
			m.editingKey = false
			m.editBuffer = ""
			return m, nil
		case tea.KeyEnter:
			if len(m.providerStatuses) > 0 {
				p := m.providerStatuses[m.settingsCursor].Provider
				_ = m.cli.Send(wire.SetProviderKey(p, m.editBuffer))
			}
			m.editingKey = false
			m.editBuffer = ""
			return m, nil
		case tea.KeyBackspace:
			if len(m.editBuffer) > 0 {
				m.editBuffer = m.editBuffer[:len(m.editBuffer)-1]
			}
			return m, nil
		default:
			if msg.Type == tea.KeyRunes {
				m.editBuffer += string(msg.Runes)
			}
			return m, nil
		}
	}
	// Browsing
	switch msg.String() {
	case "esc", "ctrl+s":
		m.settingsOpen = false
		return m, nil
	case "j", "down":
		if len(m.providerStatuses) > 0 {
			m.settingsCursor = (m.settingsCursor + 1) % len(m.providerStatuses)
		}
		return m, nil
	case "k", "up":
		if len(m.providerStatuses) > 0 {
			m.settingsCursor = (m.settingsCursor - 1 + len(m.providerStatuses)) % len(m.providerStatuses)
		}
		return m, nil
	case "enter":
		m.editingKey = true
		m.editBuffer = ""
		return m, nil
	}
	return m, nil
}
