package tui

import "github.com/charmbracelet/lipgloss"

var helpBox = lipgloss.NewStyle().
	Border(lipgloss.RoundedBorder()).
	Padding(1, 2).
	BorderForeground(lipgloss.Color("214"))

func renderHelp() string {
	return helpBox.Render(`arnold tui — keybindings

Normal mode:
  i           insert mode (type a message in the conversation pane)
  tab         next pane
  shift+tab   previous pane
  ?           toggle this help
  ctrl+s      API key settings (set Anthropic/OpenAI/xAI keys)
  q / ctrl+c  quit

Insert mode:
  esc         back to normal
  enter       send the message
  backspace   erase one char

Settings (Ctrl+S):
  j / k       move cursor between providers
  enter       edit the highlighted provider's key
  esc         leave edit mode / close overlay

Panes:
  conversation  you ↔ arnold
  inbox         recent events (jobs, notifications)
  jobs          in-flight 439 dispatches`)
}
