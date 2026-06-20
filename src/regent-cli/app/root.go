// Package app — the cobra root and composition root (canonical app/di):
// the only place the daemon client is wired into commands.
package app

import (
	"fmt"
	"os"
	"time"

	"regent/cli/features/chat"
	"regent/cli/features/cron"
	"regent/cli/features/doctor"
	"regent/cli/features/inspect"
	"regent/cli/features/logs"
	"regent/cli/features/mcp"
	"regent/cli/features/memory"
	"regent/cli/features/sessions"
	"regent/cli/features/setup"
	"regent/cli/shared/daemon"
	"regent/cli/shared/rpc"
	"regent/cli/shared/ui"

	"github.com/spf13/cobra"
)

const cliVersion = "0.1.0"

var profile string

// connect spawns the daemon for the active profile (stdio child mode).
func connect() (*rpc.Client, error) {
	path, err := daemon.Locate()
	if err != nil {
		return nil, err
	}
	home := daemon.Home(profile)
	if err := os.MkdirAll(home, 0o755); err != nil {
		return nil, fmt.Errorf("create REGENT_HOME %s: %w", home, err)
	}
	return rpc.Spawn(path, home)
}

// Execute builds the command tree and runs it.
func Execute() {
	// Enable ANSI on legacy Windows consoles for the non-TUI subcommands
	// (the bubbletea chat sets up its own terminal).
	ui.EnableVT()
	root := &cobra.Command{
		Use:           "regent",
		Short:         "Regent — a personal AI agent",
		SilenceUsage:  true,
		SilenceErrors: false,
		// Bare `regent` opens chat, the Hermes convention.
		RunE: func(cmd *cobra.Command, args []string) error {
			return withClient(chat.Run)
		},
	}
	root.PersistentFlags().StringVarP(&profile, "profile", "p", "",
		"profile name (isolates state under ~/.regent-profiles/<name>)")

	root.AddCommand(
		&cobra.Command{
			Use:   "chat",
			Short: "Interactive chat with the agent",
			RunE:  func(cmd *cobra.Command, args []string) error { return withClient(chat.Run) },
		},
		setup.Command(func() string { return daemon.Home(profile) }),
		sessions.Command(withClient),
		cron.Command(withClient),
		memory.Command(withClient),
		mcp.Command(func() string { return daemon.Home(profile) }),
		logs.Command(func() string { return daemon.Home(profile) }),
		inspect.ModelCommand(withClient),
		inspect.SkillsCommand(withClient),
		inspect.ConfigCommand(withClient),
		&cobra.Command{
			Use:   "doctor",
			Short: "Check the installation: daemon, config, provider key",
			RunE: func(cmd *cobra.Command, args []string) error {
				return doctor.Run(profile, cliVersion)
			},
		},
		&cobra.Command{
			Use:   "version",
			Short: "Print the CLI version",
			Run: func(cmd *cobra.Command, args []string) {
				fmt.Println("regent", cliVersion)
			},
		},
	)

	if err := root.Execute(); err != nil {
		os.Exit(1)
	}
}

// withClient runs fn with a connected client and always closes it (EOF =
// the daemon drains and exits).
func withClient(fn func(*rpc.Client) error) error {
	client, err := connect()
	if err != nil {
		return err
	}
	defer client.Close()
	// Fail fast when the daemon is unhealthy.
	if _, err := client.Call("health", map[string]any{}, 10*time.Second); err != nil {
		return fmt.Errorf("daemon health check failed: %w", err)
	}
	return fn(client)
}
