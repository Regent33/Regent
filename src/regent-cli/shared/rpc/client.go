package rpc

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// Client speaks JSON-RPC 2.0 over a reader/writer pair. Responses are
// routed to their callers by id; notifications fan out on Notifications.
// Constructor injection: tests pass in-memory pipes, production passes the
// spawned daemon's stdio.
type Client struct {
	mu      sync.Mutex
	nextID  int64
	pending map[int64]chan Message

	// Notifications delivers server events (tool.start, approval.request…).
	Notifications chan Message

	w   io.Writer
	cmd *exec.Cmd
}

// NewClient wraps an existing transport and starts the demux loop.
func NewClient(r io.Reader, w io.Writer) *Client {
	c := &Client{
		pending:       make(map[int64]chan Message),
		Notifications: make(chan Message, 64),
		w:             w,
	}
	go c.demux(r)
	return c
}

// Spawn starts the daemon as a child process (stdio mode) and connects.
// The daemon inherits stderr so its logs stay visible during development.
func Spawn(daemonPath, regentHome string) (*Client, error) {
	cmd := exec.Command(daemonPath)
	cmd.Env = os.Environ()
	if regentHome != "" {
		cmd.Env = append(cmd.Env, "REGENT_HOME="+regentHome)
		// Load $REGENT_HOME/.env (secrets like REGENT_API_KEY). The real
		// environment wins, so .env never overrides an explicit export.
		cmd.Env = appendDotEnv(cmd.Env, regentHome)
	}
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("daemon stdin: %w", err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, fmt.Errorf("daemon stdout: %w", err)
	}
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		return nil, fmt.Errorf("spawn daemon %q: %w", daemonPath, err)
	}
	c := NewClient(stdout, stdin)
	c.cmd = cmd
	return c, nil
}

// appendDotEnv merges $REGENT_HOME/.env into env, skipping keys already present
// (so an explicit export always wins) and ignoring blanks/comments.
func appendDotEnv(env []string, home string) []string {
	data, err := os.ReadFile(filepath.Join(home, ".env"))
	if err != nil {
		return env
	}
	present := make(map[string]bool, len(env))
	for _, e := range env {
		if i := strings.IndexByte(e, '='); i > 0 {
			present[e[:i]] = true
		}
	}
	for _, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		i := strings.IndexByte(line, '=')
		if i <= 0 {
			continue
		}
		key := strings.TrimSpace(line[:i])
		if present[key] {
			continue
		}
		val := strings.Trim(strings.TrimSpace(line[i+1:]), `"`)
		env = append(env, key+"="+val)
	}
	return env
}

func (c *Client) demux(r io.Reader) {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)
	for scanner.Scan() {
		var msg Message
		if err := json.Unmarshal(scanner.Bytes(), &msg); err != nil {
			continue // non-protocol noise on stdout is dropped
		}
		if msg.IsNotification() {
			select {
			case c.Notifications <- msg:
			default: // a slow consumer must never block the demux
			}
			continue
		}
		if msg.ID == nil {
			continue
		}
		c.mu.Lock()
		ch, ok := c.pending[*msg.ID]
		if ok {
			delete(c.pending, *msg.ID)
		}
		c.mu.Unlock()
		if ok {
			ch <- msg
		}
	}
	close(c.Notifications)
}

// CallAsync sends a request and returns the channel its response will
// arrive on. Chat uses this to render notifications while a turn runs.
func (c *Client) CallAsync(method string, params any) (<-chan Message, error) {
	c.mu.Lock()
	c.nextID++
	id := c.nextID
	ch := make(chan Message, 1)
	c.pending[id] = ch
	c.mu.Unlock()

	req := Request{JSONRPC: "2.0", Method: method, Params: params, ID: &id}
	line, err := json.Marshal(req)
	if err != nil {
		return nil, err
	}
	if _, err := c.w.Write(append(line, '\n')); err != nil {
		return nil, fmt.Errorf("write %s: %w", method, err)
	}
	return ch, nil
}

// Call sends a request and blocks for its response.
func (c *Client) Call(method string, params any, timeout time.Duration) (json.RawMessage, error) {
	ch, err := c.CallAsync(method, params)
	if err != nil {
		return nil, err
	}
	select {
	case msg := <-ch:
		if msg.Error != nil {
			return nil, fmt.Errorf("%s: %s (code %d)", method, msg.Error.Message, msg.Error.Code)
		}
		return msg.Result, nil
	case <-time.After(timeout):
		return nil, fmt.Errorf("%s: timed out after %s", method, timeout)
	}
}

// Close shuts the transport; a spawned daemon sees EOF and drains.
func (c *Client) Close() error {
	if closer, ok := c.w.(io.Closer); ok {
		closer.Close()
	}
	if c.cmd != nil {
		return c.cmd.Wait()
	}
	return nil
}
