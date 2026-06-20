package tests

import (
	"testing"

	"regent/cli/features/logs"
)

func TestLogsCommandHasFollowFlag(t *testing.T) {
	cmd := logs.Command(func() string { return "" })
	if cmd.Name() != "logs" {
		t.Fatalf("name = %q, want logs", cmd.Name())
	}
	if cmd.Flags().Lookup("follow") == nil {
		t.Fatal("logs missing --follow flag")
	}
}
