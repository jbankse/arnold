package tui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
)

var (
	convAuthorYou    = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("39"))  // cyan
	convAuthorArnold = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("214")) // orange
	convAuthorSys    = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("196")) // red
	convPrompt       = lipgloss.NewStyle().Foreground(lipgloss.Color("245"))
)

func (m *Model) renderConversation(width, height int) string {
	var b strings.Builder
	for _, row := range m.conversation {
		var label string
		switch row.Author {
		case "you":
			label = convAuthorYou.Render("you: ")
		case "arnold":
			label = convAuthorArnold.Render("arnold: ")
		default:
			label = convAuthorSys.Render(row.Author + ": ")
		}
		b.WriteString(label)
		b.WriteString(row.Text)
		b.WriteString("\n")
	}
	if m.mode == ModeInsert {
		b.WriteString(convPrompt.Render("> ") + m.input + "_")
	} else {
		b.WriteString(convPrompt.Render("> (press i to insert)"))
	}
	return paneBox("conversation", b.String(), width, height, m.focus == PaneConversation)
}
