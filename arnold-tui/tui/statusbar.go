package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

var (
	statusBar   = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("250"))
	statusOK    = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("46"))
	statusBad   = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("196"))
	statusBadge = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("214")).Bold(true)
)

func (m *Model) renderStatusBar(width int) string {
	connDot := statusOK.Render("●")
	connWord := "connected"
	switch m.connState {
	case ConnConnecting:
		connDot = statusBadge.Render("●")
		connWord = "connecting"
	case ConnDisconnected:
		connDot = statusBad.Render("●")
		connWord = "disconnected"
	}

	provider := m.cost.Provider
	model := m.cost.Model
	if provider == "" {
		provider = "?"
		model = "?"
	}

	left := statusBar.Render(fmt.Sprintf(" %s/%s ", provider, model))
	mid := statusBar.Render(fmt.Sprintf(" session $%.4f ", m.cost.SessionUSD))
	right := statusBar.Render(fmt.Sprintf(" %s %s ", connDot, connWord))
	focus := statusBadge.Render(fmt.Sprintf(" [%s] ", m.focus))

	bar := lipgloss.JoinHorizontal(lipgloss.Top, left, mid, right, focus)
	if lipgloss.Width(bar) < width {
		pad := width - lipgloss.Width(bar)
		bar += statusBar.Render(strings.Repeat(" ", pad))
	}
	return bar
}
