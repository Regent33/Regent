package tests

import (
	"testing"

	"regent/cli/features/memory"
	"regent/cli/shared/rpc"

	"github.com/spf13/cobra"
)

func noClient(func(*rpc.Client) error) error { return nil }

func hasSub(cmd *cobra.Command, name string) bool {
	for _, c := range cmd.Commands() {
		if c.Name() == name {
			return true
		}
	}
	return false
}

func TestMemoryCommandWiring(t *testing.T) {
	cmd := memory.Command(noClient)
	if cmd.Name() != "memory" {
		t.Fatalf("name = %q, want memory", cmd.Name())
	}
	for _, sub := range []string{"pending", "approve", "reject"} {
		if !hasSub(cmd, sub) {
			t.Fatalf("memory missing subcommand %q", sub)
		}
	}
}
