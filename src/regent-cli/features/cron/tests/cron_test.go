// Package tests holds black-box tests for the cron feature, driving it through
// its exported Command(). White-box unit tests (unexported helpers) live beside
// the code — Go only exposes a package's exported API to a separate test
// package like this one.
package tests

import (
	"testing"

	"regent/cli/features/cron"
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

func TestCronCommandWiring(t *testing.T) {
	cmd := cron.Command(noClient)
	if cmd.Name() != "cron" {
		t.Fatalf("name = %q, want cron", cmd.Name())
	}
	for _, sub := range []string{"list", "add", "remove"} {
		if !hasSub(cmd, sub) {
			t.Fatalf("cron missing subcommand %q", sub)
		}
	}
}
