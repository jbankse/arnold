package tui

import (
	"strings"
)

func (m *Model) renderInbox(width, height int) string {
	var b strings.Builder
	start := 0
	if len(m.inboxEvents) > 12 {
		start = len(m.inboxEvents) - 12 // keep last 12 visible
	}
	for _, ev := range m.inboxEvents[start:] {
		b.WriteString(ev)
		b.WriteString("\n")
	}
	if len(m.inboxEvents) == 0 {
		b.WriteString("(no inbox events yet)")
	}
	return paneBox("inbox", b.String(), width, height, m.focus == PaneInbox)
}
