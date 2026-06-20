// Package daemon resolves where the regent-daemon binary lives and what
// REGENT_HOME a profile maps to.
package daemon

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

const exeSuffix = ".exe"

// Locate resolves the regent-daemon binary.
func Locate() (string, error) {
	return LocateBinary("regent-daemon", "REGENT_DAEMON_PATH")
}

// LocateBinary resolves a Regent binary by base name (no extension): the
// `envVar` override wins, then a sibling of the CLI executable, then PATH, then
// the cargo dev build. Shared by the daemon and `regent mcp serve`.
func LocateBinary(base, envVar string) (string, error) {
	if p := os.Getenv(envVar); p != "" {
		if _, err := os.Stat(p); err == nil {
			return p, nil
		}
		return "", fmt.Errorf("%s set but not found: %s", envVar, p)
	}
	binaryName := base + exeSuffix
	if exe, err := os.Executable(); err == nil {
		sibling := filepath.Join(filepath.Dir(exe), binaryName)
		if _, err := os.Stat(sibling); err == nil {
			return sibling, nil
		}
	}
	if p, err := exec.LookPath(base); err == nil {
		return p, nil
	}
	// Dev fallback: walk up from cwd looking for the cargo target dir.
	dir, err := os.Getwd()
	if err == nil {
		for range 6 {
			candidate := filepath.Join(dir, "target", "debug", binaryName)
			if _, err := os.Stat(candidate); err == nil {
				return candidate, nil
			}
			parent := filepath.Dir(dir)
			if parent == dir {
				break
			}
			dir = parent
		}
	}
	return "", fmt.Errorf("%s not found (set %s or build with `cargo build -p regent-daemon`)", base, envVar)
}

// Home maps a profile name to its REGENT_HOME directory. Empty profile =
// $REGENT_HOME if set, else ~/.regent; named profiles always isolate under
// ~/.regent-profiles (a profile is an explicit choice — env never wins).
func Home(profile string) string {
	base, err := os.UserHomeDir()
	if err != nil {
		base = "."
	}
	if profile == "" {
		if h := os.Getenv("REGENT_HOME"); h != "" {
			return h
		}
		return filepath.Join(base, ".regent")
	}
	return filepath.Join(base, ".regent-profiles", profile)
}
