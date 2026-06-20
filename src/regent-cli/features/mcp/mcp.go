// Package mcp wires `regent mcp` — exposing Regent over the Model Context
// Protocol. `serve` runs the regent-mcp server on stdin/stdout so an MCP client
// can spawn `regent mcp serve` and call Regent's tools.
package mcp

import (
	"os"
	"os/exec"

	"regent/cli/shared/daemon"

	"github.com/spf13/cobra"
)

// Command builds `regent mcp` with its subcommands. home resolves the active
// profile's REGENT_HOME (passed to the server child).
func Command(home func() string) *cobra.Command {
	mcp := &cobra.Command{
		Use:   "mcp",
		Short: "Model Context Protocol — expose Regent's tools to MCP clients",
	}
	mcp.AddCommand(&cobra.Command{
		Use:   "serve",
		Short: "Run an MCP server over stdio (an MCP client spawns this)",
		Long: "Speaks MCP JSON-RPC on stdin/stdout, exposing Regent's core tools. " +
			"Point an MCP client at `regent mcp serve` as a stdio server. " +
			"Logs go to stderr; tools run with approval denied by default.",
		// stdio belongs to the protocol — no extra args.
		Args: cobra.NoArgs,
		RunE: func(_ *cobra.Command, _ []string) error {
			bin, err := daemon.LocateBinary("regent-mcp", "REGENT_MCP_PATH")
			if err != nil {
				return err
			}
			child := exec.Command(bin)
			// Inherit stdio so the client's connection reaches the server child
			// transparently; stderr passes logs through.
			child.Stdin = os.Stdin
			child.Stdout = os.Stdout
			child.Stderr = os.Stderr
			child.Env = append(os.Environ(), "REGENT_HOME="+home())
			return child.Run()
		},
	})
	return mcp
}
