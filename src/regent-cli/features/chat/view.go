package chat

import (
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"regent/cli/shared/rpc"
	"regent/cli/shared/ui"

	tea "github.com/charmbracelet/bubbletea"
)

// ── tea messages + commands ─────────────────────────────────────────────────

type notifMsg struct{ n rpc.Message }
type respMsg struct{ resp rpc.Message }
type streamClosedMsg struct{}

// listen reads one daemon notification; re-issued after each one so the
// stream is drained for the life of the session.
func (m *model) listen() tea.Cmd {
	return func() tea.Msg {
		n, ok := <-m.client.Notifications
		if !ok {
			return streamClosedMsg{}
		}
		return notifMsg{n}
	}
}

func waitResp(ch <-chan rpc.Message) tea.Cmd {
	return func() tea.Msg { return respMsg{<-ch} }
}

func (m *model) submit(text string) tea.Cmd {
	ch, err := m.client.CallAsync("prompt.submit",
		map[string]any{"session_id": m.sessionID, "text": text})
	if err != nil {
		m.lines = append(m.lines, ui.Grey+"⚠ "+err.Error()+ui.Reset)
		m.busy = false
		return nil
	}
	return waitResp(ch)
}

func (m *model) interrupt() tea.Cmd {
	sid, c := m.sessionID, m.client
	return func() tea.Msg {
		_, _ = c.Call("turn.interrupt", map[string]any{"session_id": sid}, 10*time.Second)
		return nil
	}
}

func (m *model) resolveApproval(text string) tea.Cmd {
	approved := strings.EqualFold(text, "y") || strings.EqualFold(text, "yes")
	verb := "✗ denied"
	if approved {
		verb = "✓ approved"
	}
	m.lines = append(m.lines, "  "+ui.Grey+verb+ui.Reset)
	sid, c := m.sessionID, m.client
	return func() tea.Msg {
		_, _ = c.Call("approval.respond",
			map[string]any{"session_id": sid, "approved": approved}, 10*time.Second)
		return nil
	}
}

// handleNotif folds one daemon notification into the transcript.
func (m *model) handleNotif(n rpc.Message) {
	var p map[string]any
	_ = json.Unmarshal(n.Params, &p)
	str := func(k string) string { s, _ := p[k].(string); return s }

	switch n.Method {
	case "turn.started":
		m.busy = true
	case "message.delta":
		m.stream.WriteString(str("text"))
		m.streaming = true
	case "tool.start":
		m.commitStream()
		m.lines = append(m.lines, "  "+ui.TealD+"⚙ "+str("tool")+"…"+ui.Reset)
	case "tool.complete":
		if e, _ := p["is_error"].(bool); e {
			m.lines = append(m.lines, "  "+ui.Grey+"✗ "+str("tool")+" hit a snag"+ui.Reset)
		}
	case "approval.request":
		m.commitStream()
		m.lines = append(m.lines,
			ui.Teal+"⚠ "+str("tool")+" wants to run a sensitive action:"+ui.Reset,
			"  "+str("action"))
		m.approving = true
		m.input.Placeholder = "Allow it? [y/N]"
	case "message.outbound":
		m.commitStream()
		m.lines = append(m.lines,
			ui.Teal+"✉ delivered to "+str("target")+ui.Reset+": "+ui.White+str("text")+ui.Reset)
	case "turn.interrupted":
		m.commitStream()
		m.lines = append(m.lines, "  "+ui.Grey+"🛑 interrupted"+ui.Reset)
		m.busy = false
	case "message.complete":
		if !m.streaming {
			if r := str("reply"); r != "" {
				m.lines = append(m.lines, ui.White+r+ui.Reset)
			}
		}
		m.commitStream()
	case "turn.complete":
		m.commitStream()
		m.busy = false
	}
	m.refresh()
}

// commitStream moves the live streaming buffer into the transcript.
func (m *model) commitStream() {
	if m.streaming && m.stream.Len() > 0 {
		m.lines = append(m.lines, ui.White+m.stream.String()+ui.Reset)
	}
	m.stream.Reset()
	m.streaming = false
}

// ── rendering ────────────────────────────────────────────────────────────────

func (m *model) View() string {
	if !m.ready {
		return "starting Regent…"
	}
	return m.vp.View() + "\n" + m.statusLine() + "\n" + m.input.View()
}

func (m *model) statusLine() string {
	switch {
	case m.approving:
		return ui.Teal + "  awaiting your approval" + ui.Reset
	case m.busy:
		return m.spin.View() + ui.Grey + " thinking… (Ctrl-C to interrupt)" + ui.Reset
	default:
		return ui.Grey + "  /quit to exit · Enter to send" + ui.Reset
	}
}

// refresh rebuilds the viewport: banner + welcome panel + transcript + any
// in-flight streaming text, scrolled to the bottom.
func (m *model) refresh() {
	if !m.ready {
		return
	}
	var b strings.Builder
	b.WriteString(ui.RenderBanner())
	b.WriteString("\n")
	b.WriteString(m.welcome())
	b.WriteString("\n\n")
	b.WriteString(ui.Bold + ui.White + "Welcome! I'm Regent — at your service. 🤍" + ui.Reset)
	for _, l := range m.lines {
		b.WriteString("\n" + l)
	}
	if m.streaming && m.stream.Len() > 0 {
		b.WriteString("\n\n" + ui.White + m.stream.String() + ui.Reset)
	}
	m.vp.SetContent(b.String())
	m.vp.GotoBottom()
}

// welcome renders the bordered panel: king mark on the left, session info on
// the right (the Hermes layout in Regent's palette).
func (m *model) welcome() string {
	info := []string{
		"",
		ui.Header("Session"),
		ui.Label("model", m.modelName),
		ui.Label("session", short(m.sessionID)),
		"",
		ui.Header("Commands"),
		ui.Note(m.commands),
		"",
		ui.Header("Skills"),
		ui.Note(fmt.Sprintf("%d learned — they grow as we work together", m.skills)),
	}
	return ui.Panel("Regent v0.1.0", ui.SideBySide(info))
}

func short(id string) string {
	if len(id) > 18 {
		return id[:18] + "…"
	}
	return id
}
