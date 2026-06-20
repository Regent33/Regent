// Package doctor — `regent doctor`: verifies the installation end to end
// (daemon binary, REGENT_HOME, config load, provider key, RPC round-trip).
package doctor

import (
	"fmt"
	"os"
	"time"

	"regent/cli/shared/daemon"
	"regent/cli/shared/rpc"
)

// Run executes every check and returns an error when a hard check fails.
func Run(profile, cliVersion string) error {
	fmt.Printf("regent doctor (cli %s)\n\n", cliVersion)
	hardFailure := false

	// 1. Daemon binary.
	daemonPath, err := daemon.Locate()
	if err != nil {
		fail("daemon binary", err.Error())
		return fmt.Errorf("doctor found problems")
	}
	pass("daemon binary", daemonPath)

	// 2. REGENT_HOME.
	home := daemon.Home(profile)
	if err := os.MkdirAll(home, 0o755); err != nil {
		fail("REGENT_HOME", fmt.Sprintf("%s: %v", home, err))
		hardFailure = true
	} else {
		pass("REGENT_HOME", home)
	}

	// 3. Provider key (warn-only: the daemon boots without it).
	if os.Getenv("REGENT_API_KEY") == "" {
		warn("REGENT_API_KEY", "not set — prompt.submit will fail until exported")
	} else {
		pass("REGENT_API_KEY", "set")
	}

	// 4. Daemon round-trip: spawn → health → config.get → clean EOF exit.
	client, err := rpc.Spawn(daemonPath, home)
	if err != nil {
		fail("daemon spawn", err.Error())
		return fmt.Errorf("doctor found problems")
	}
	defer client.Close()

	if _, err := client.Call("health", map[string]any{}, 15*time.Second); err != nil {
		fail("health round-trip", err.Error())
		hardFailure = true
	} else {
		pass("health round-trip", "ok")
	}

	if _, err := client.Call("config.get", map[string]any{}, 15*time.Second); err != nil {
		fail("config.yaml", err.Error())
		hardFailure = true
	} else {
		pass("config.yaml", "loads and validates")
	}

	if hardFailure {
		return fmt.Errorf("doctor found problems")
	}
	fmt.Println("\nall checks passed")
	return nil
}

func pass(check, detail string) { fmt.Printf("  ✓ %-18s %s\n", check, detail) }
func warn(check, detail string) { fmt.Printf("  ! %-18s %s\n", check, detail) }
func fail(check, detail string) { fmt.Printf("  ✗ %-18s %s\n", check, detail) }
