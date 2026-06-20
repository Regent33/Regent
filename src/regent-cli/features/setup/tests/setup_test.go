// Package tests black-box-tests the setup command's exported surface. The
// secret-write internals (secureWriteFile/writeEnv) are unit-tested inline in
// the setup package, where their unexported symbols are reachable.
package tests

import (
	"testing"

	"regent/cli/features/setup"
)

func TestSetupCommandHasProviderAndKeyFlags(t *testing.T) {
	cmd := setup.Command(func() string { return "" })
	if cmd.Name() != "setup" {
		t.Fatalf("name = %q, want setup", cmd.Name())
	}
	for _, flag := range []string{"provider", "model", "key", "base-url"} {
		if cmd.Flags().Lookup(flag) == nil {
			t.Fatalf("setup missing --%s flag", flag)
		}
	}
}
