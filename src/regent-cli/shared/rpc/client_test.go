package rpc

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"testing"
	"time"
)

// fakeDaemon answers every request method with a canned result and can
// inject notifications before the response — the order chat relies on.
func fakeDaemon(t *testing.T, r io.Reader, w io.Writer, notifyFirst []string) {
	t.Helper()
	scanner := bufio.NewScanner(r)
	for scanner.Scan() {
		var req Request
		if err := json.Unmarshal(scanner.Bytes(), &req); err != nil {
			t.Errorf("fake daemon got bad JSON: %v", err)
			return
		}
		for _, m := range notifyFirst {
			fmt.Fprintf(w, `{"jsonrpc":"2.0","method":%q,"params":{"session_id":"s1"}}`+"\n", m)
		}
		fmt.Fprintf(w, `{"jsonrpc":"2.0","result":{"echo":%q},"id":%d}`+"\n", req.Method, *req.ID)
	}
}

func pipePair() (clientIn io.Reader, daemonOut io.Writer, daemonIn io.Reader, clientOut io.Writer) {
	r1, w1 := io.Pipe() // daemon → client
	r2, w2 := io.Pipe() // client → daemon
	return r1, w1, r2, w2
}

func TestCallRoutesResponseById(t *testing.T) {
	cIn, dOut, dIn, cOut := pipePair()
	go fakeDaemon(t, dIn, dOut, nil)
	c := NewClient(cIn, cOut)

	res, err := c.Call("health", map[string]any{}, 5*time.Second)
	if err != nil {
		t.Fatalf("call failed: %v", err)
	}
	var body map[string]string
	if err := json.Unmarshal(res, &body); err != nil {
		t.Fatalf("bad result: %v", err)
	}
	if body["echo"] != "health" {
		t.Fatalf("wrong routing: %v", body)
	}
}

func TestNotificationsArriveOnTheirChannel(t *testing.T) {
	cIn, dOut, dIn, cOut := pipePair()
	go fakeDaemon(t, dIn, dOut, []string{"turn.started", "tool.start"})
	c := NewClient(cIn, cOut)

	ch, err := c.CallAsync("prompt.submit", map[string]any{"text": "hi"})
	if err != nil {
		t.Fatalf("submit failed: %v", err)
	}
	var got []string
	for range 2 {
		select {
		case n := <-c.Notifications:
			got = append(got, n.Method)
		case <-time.After(5 * time.Second):
			t.Fatal("notification never arrived")
		}
	}
	if got[0] != "turn.started" || got[1] != "tool.start" {
		t.Fatalf("wrong notifications: %v", got)
	}
	select {
	case resp := <-ch:
		if resp.Error != nil {
			t.Fatalf("unexpected error: %v", resp.Error)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("response never arrived")
	}
}

func TestErrorResponsesSurfaceAsErrors(t *testing.T) {
	cIn, dOut, dIn, cOut := pipePair()
	go func() {
		scanner := bufio.NewScanner(dIn)
		for scanner.Scan() {
			var req Request
			json.Unmarshal(scanner.Bytes(), &req)
			fmt.Fprintf(dOut, `{"jsonrpc":"2.0","error":{"code":-32601,"message":"nope"},"id":%d}`+"\n", *req.ID)
		}
	}()
	c := NewClient(cIn, cOut)

	_, err := c.Call("no.such", nil, 5*time.Second)
	if err == nil {
		t.Fatal("expected an error")
	}
}
