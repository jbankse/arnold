package tui

import (
	tea "github.com/charmbracelet/bubbletea"
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

// rootKeybindings handles global keys (mode changes, pane focus, quit, help).
// Pane-specific keys are handled in each pane's Update.
func (m *Model) rootKeybindings(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
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
	}
	return m, nil
}
