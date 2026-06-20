// Package chat — the interactive TUI (bubbletea): a scrollable transcript, a
// persistent input box, live streamed replies, inline approval, and Ctrl-C
// interrupt. Daemon notifications and responses arrive as tea.Msgs.
package chat

import (
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"regent/cli/shared/rpc"
	"regent/cli/shared/ui"

	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
)

// footerReserve is the rows kept below the viewport (status line + input).
const footerReserve = 3

// Run opens a session and launches the bubbletea program.
func Run(c *rpc.Client) error {
	res, err := c.Call("session.create", map[string]any{}, 30*time.Second)
	if err != nil {
		return fmt.Errorf("session.create: %w", err)
	}
	var ref struct {
		SessionID string `json:"session_id"`
	}
	if err := json.Unmarshal(res, &ref); err != nil {
		return fmt.Errorf("bad session.create result: %w", err)
	}
	p := tea.NewProgram(newModel(c, ref.SessionID), tea.WithAltScreen(), tea.WithMouseCellMotion())
	_, err = p.Run()
	return err
}

type model struct {
	client    *rpc.Client
	sessionID string

	vp    viewport.Model
	input textinput.Model
	spin  spinner.Model

	// Welcome-panel data (fetched once at startup).
	modelName string
	skills    int
	commands  string

	lines     []string        // committed transcript lines
	stream    strings.Builder // current assistant streaming text
	streaming bool
	busy      bool
	approving bool
	ready     bool
}

func newModel(c *rpc.Client, sessionID string) *model {
	in := textinput.New()
	in.Placeholder = "Type a message…"
	in.Prompt = ui.Teal + "❯ " + ui.Reset
	in.Focus()
	in.CharLimit = 8000

	sp := spinner.New()
	sp.Spinner = spinner.Dot

	m := &model{client: c, sessionID: sessionID, input: in, spin: sp}
	m.fetchWelcome()
	return m
}

// fetchWelcome populates the panel fields with one round-trip each.
func (m *model) fetchWelcome() {
	m.modelName = "-"
	if res, err := m.client.Call("model.get", map[string]any{}, 10*time.Second); err == nil {
		var b struct {
			Model string `json:"model"`
		}
		if json.Unmarshal(res, &b) == nil {
			m.modelName = b.Model
		}
	}
	if res, err := m.client.Call("skills.list", map[string]any{}, 10*time.Second); err == nil {
		var items []json.RawMessage
		if json.Unmarshal(res, &items) == nil {
			m.skills = len(items)
		}
	}
	m.commands = "-"
	if res, err := m.client.Call("commands.list", map[string]any{}, 10*time.Second); err == nil {
		var items []struct {
			Name string `json:"name"`
		}
		if json.Unmarshal(res, &items) == nil {
			names := make([]string, 0, len(items))
			for _, i := range items {
				names = append(names, "/"+i.Name)
			}
			m.commands = strings.Join(names, " ")
		}
	}
}

func (m *model) Init() tea.Cmd {
	return tea.Batch(textinput.Blink, m.spin.Tick, m.listen())
}

func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		if !m.ready {
			m.vp = viewport.New(msg.Width, msg.Height-footerReserve)
			m.ready = true
		} else {
			m.vp.Width = msg.Width
			m.vp.Height = msg.Height - footerReserve
		}
		m.input.Width = msg.Width - 4
		m.refresh()
		return m, nil

	case tea.KeyMsg:
		return m.onKey(msg)

	case notifMsg:
		m.handleNotif(msg.n)
		return m, m.listen() // keep listening

	case respMsg:
		// Backstop: the reply already streamed via notifications; surface only
		// an error the notifications didn't carry.
		if msg.resp.Error != nil {
			m.lines = append(m.lines, ui.Grey+"⚠ "+msg.resp.Error.Message+ui.Reset)
		}
		m.busy = false
		m.refresh()
		return m, nil

	case streamClosedMsg:
		m.lines = append(m.lines, ui.Grey+"daemon stream closed"+ui.Reset)
		m.refresh()
		return m, nil

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spin, cmd = m.spin.Update(msg)
		return m, cmd

	default:
		var cmd tea.Cmd
		m.vp, cmd = m.vp.Update(msg)
		return m, cmd
	}
}

func (m *model) onKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "ctrl+c":
		if m.busy {
			return m, m.interrupt()
		}
		return m, tea.Quit
	case "enter":
		text := strings.TrimSpace(m.input.Value())
		if m.approving {
			cmd := m.resolveApproval(text)
			m.input.Reset()
			m.input.Placeholder = "Type a message…"
			m.refresh()
			return m, cmd
		}
		if m.busy || text == "" {
			return m, nil
		}
		if text == "/quit" || text == "/exit" {
			return m, tea.Quit
		}
		m.lines = append(m.lines, ui.Teal+"❯ "+ui.Reset+ui.White+text+ui.Reset)
		m.input.Reset()
		m.busy = true
		cmd := m.submit(text)
		m.refresh()
		return m, cmd
	case "pgup", "pgdown", "up", "down":
		var cmd tea.Cmd
		m.vp, cmd = m.vp.Update(msg)
		return m, cmd
	default:
		var cmd tea.Cmd
		m.input, cmd = m.input.Update(msg)
		return m, cmd
	}
}
