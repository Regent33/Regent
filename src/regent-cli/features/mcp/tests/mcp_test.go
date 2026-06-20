package tests

import (
	"testing"

	"regent/cli/features/mcp"
)

func TestMcpCommandHasServe(t *testing.T) {
	cmd := mcp.Command(func() string { return "" })
	if cmd.Name() != "mcp" {
		t.Fatalf("name = %q, want mcp", cmd.Name())
	}
	found := false
	for _, c := range cmd.Commands() {
		if c.Name() == "serve" {
			found = true
		}
	}
	if !found {
		t.Fatal("mcp missing subcommand serve")
	}
}
