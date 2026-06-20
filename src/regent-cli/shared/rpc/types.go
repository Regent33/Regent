// Package rpc — newline-delimited JSON-RPC 2.0 over the daemon's stdio
// (the child-process transport from ADR-011).
package rpc

import "encoding/json"

// Request is an outbound JSON-RPC call. ID is a pointer so notifications
// (no id) marshal without the field.
type Request struct {
	JSONRPC string `json:"jsonrpc"`
	Method  string `json:"method"`
	Params  any    `json:"params,omitempty"`
	ID      *int64 `json:"id,omitempty"`
}

// ErrorBody is the JSON-RPC error object.
type ErrorBody struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

// Message is any inbound line: a response (ID set, Method empty) or a
// server notification (Method set, no ID).
type Message struct {
	JSONRPC string          `json:"jsonrpc"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *ErrorBody      `json:"error,omitempty"`
	ID      *int64          `json:"id,omitempty"`
	Method  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
}

// IsNotification reports whether the message is a server-initiated event.
func (m *Message) IsNotification() bool {
	return m.Method != ""
}
