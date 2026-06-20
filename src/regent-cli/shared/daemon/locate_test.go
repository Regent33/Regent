package daemon

import (
	"os"
	"path/filepath"
	"testing"
)

func TestHomeDefaultAndEnvOverride(t *testing.T) {
	// No profile + REGENT_HOME set → use it verbatim.
	t.Setenv("REGENT_HOME", "/custom/home")
	if got := Home(""); got != "/custom/home" {
		t.Fatalf("env override: got %q", got)
	}

	// No profile + REGENT_HOME empty → ~/.regent.
	t.Setenv("REGENT_HOME", "")
	base, err := os.UserHomeDir()
	if err != nil {
		t.Skip("no home dir on this runner")
	}
	if got := Home(""); got != filepath.Join(base, ".regent") {
		t.Fatalf("default: got %q", got)
	}
}

func TestHomeNamedProfileIgnoresEnv(t *testing.T) {
	// A named profile always isolates under ~/.regent-profiles — env never wins.
	t.Setenv("REGENT_HOME", "/should/be/ignored")
	base, err := os.UserHomeDir()
	if err != nil {
		t.Skip("no home dir on this runner")
	}
	want := filepath.Join(base, ".regent-profiles", "work")
	if got := Home("work"); got != want {
		t.Fatalf("named profile: got %q want %q", got, want)
	}
}
