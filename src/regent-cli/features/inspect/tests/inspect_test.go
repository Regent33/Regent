package tests

import (
	"testing"

	"regent/cli/features/inspect"
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

func TestModelCommandHasListAndSet(t *testing.T) {
	model := inspect.ModelCommand(noClient)
	if model.Name() != "model" {
		t.Fatalf("name = %q, want model", model.Name())
	}
	for _, sub := range []string{"list", "set"} {
		if !hasSub(model, sub) {
			t.Fatalf("model missing subcommand %q", sub)
		}
	}
}

func TestSkillsAndConfigCommandNames(t *testing.T) {
	if got := inspect.SkillsCommand(noClient).Name(); got != "skills" {
		t.Fatalf("skills command name = %q", got)
	}
	if got := inspect.ConfigCommand(noClient).Name(); got != "config" {
		t.Fatalf("config command name = %q", got)
	}
}
