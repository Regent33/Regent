package tests

import (
	"testing"

	"regent/cli/features/sessions"
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

func TestSessionsCommandWiring(t *testing.T) {
	cmd := sessions.Command(noClient)
	if cmd.Name() != "sessions" {
		t.Fatalf("name = %q, want sessions", cmd.Name())
	}
	for _, sub := range []string{"list", "search"} {
		if !hasSub(cmd, sub) {
			t.Fatalf("sessions missing subcommand %q", sub)
		}
	}
}
