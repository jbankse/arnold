package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

var (
	jobDone  = lipgloss.NewStyle().Foreground(lipgloss.Color("46"))  // green
	jobFail  = lipgloss.NewStyle().Foreground(lipgloss.Color("196")) // red
	jobRun   = lipgloss.NewStyle().Foreground(lipgloss.Color("214")) // orange
	jobQueue = lipgloss.NewStyle().Foreground(lipgloss.Color("245")) // grey
)

func statusGlyph(status string) string {
	switch status {
	case "completed":
		return jobDone.Render("✓")
	case "failed":
		return jobFail.Render("✗")
	case "running", "exporting":
		return jobRun.Render("◌")
	case "queued":
		return jobQueue.Render("…")
	case "cancelled":
		return jobQueue.Render("⊘")
	}
	return "?"
}

func (m *Model) renderJobs(width, height int) string {
	var b strings.Builder
	if len(m.jobs) == 0 {
		b.WriteString("(no 439 jobs yet — Arnold can spin one up with sys_spin_up_439)")
	} else {
		for _, id := range m.jobOrder {
			j := m.jobs[id]
			if j == nil {
				continue
			}
			short := j.ID
			if len(short) > 8 {
				short = short[:8]
			}
			line := fmt.Sprintf("%s  %s  %s", statusGlyph(j.Status), short, truncate(j.PromptPreview, 60))
			b.WriteString(line)
			if j.LastLogLine != "" && !j.Completed {
				b.WriteString("\n     " + jobQueue.Render(truncate(j.LastLogLine, 60)))
			}
			if j.Completed && j.Exported != "" {
				b.WriteString("\n     " + jobDone.Render("→ "+j.Exported))
			}
			b.WriteString("\n")
		}
	}
	return paneBox("jobs", b.String(), width, height, m.focus == PaneJobs)
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n-1] + "…"
}
