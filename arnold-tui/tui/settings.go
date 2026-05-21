package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

var (
	settingsBox = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			Padding(1, 2).
			BorderForeground(lipgloss.Color("39")) // cyan
	settingsHeader   = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("214"))
	settingsConfig   = lipgloss.NewStyle().Foreground(lipgloss.Color("46"))  // green
	settingsMissing  = lipgloss.NewStyle().Foreground(lipgloss.Color("196")) // red
	settingsCursor   = lipgloss.NewStyle().Foreground(lipgloss.Color("214")).Bold(true)
	settingsHint     = lipgloss.NewStyle().Foreground(lipgloss.Color("245"))
	settingsEdit     = lipgloss.NewStyle().Foreground(lipgloss.Color("214"))
)

// renderSettings draws the Ctrl+S overlay listing provider key statuses with
// an inline editor. Never echoes a real key — `configured` is the only signal,
// and the edit buffer is shown literally only while the user is typing it.
func (m *Model) renderSettings() string {
	var b strings.Builder
	b.WriteString(settingsHeader.Render("API keys") + "\n\n")
	for i, st := range m.providerStatuses {
		marker := "  "
		if i == m.settingsCursor {
			marker = settingsCursor.Render("▸ ")
		}
		status := settingsMissing.Render("not set")
		if st.Configured {
			status = settingsConfig.Render("set")
		}
		b.WriteString(fmt.Sprintf("%s%-12s  %s\n", marker, st.Provider, status))
	}
	b.WriteString("\n")
	if m.editingKey {
		current := m.providerStatuses[m.settingsCursor].Provider
		// Show the typed value as plain text (the user is in the middle of
		// pasting it — they want to see they got it right). On Enter the
		// daemon stores it and the next render will revert to "set" only.
		display := m.editBuffer
		if display == "" {
			display = "(paste key, Enter to save, Esc to cancel)"
		}
		b.WriteString(settingsEdit.Render(fmt.Sprintf("→ %s = %s", current, display)))
	} else {
		b.WriteString(settingsHint.Render("j/k navigate · Enter to edit · Esc / Ctrl+S close"))
	}
	return settingsBox.Render(b.String())
}
