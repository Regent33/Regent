// Package memory — `regent memory pending|approve|reject`: the human gate for
// long-term memory writes the agent proposes (security §10.2).
package memory

import (
	"encoding/json"
	"fmt"
	"time"

	"regent/cli/shared/rpc"

	"github.com/spf13/cobra"
)

type withClient func(func(*rpc.Client) error) error

// Command builds the `memory` subtree.
func Command(run withClient) *cobra.Command {
	cmd := &cobra.Command{Use: "memory", Short: "Review memory writes awaiting approval"}

	pending := &cobra.Command{
		Use:   "pending",
		Short: "List memory writes awaiting approval",
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("memory.pending", map[string]any{}, 30*time.Second)
				if err != nil {
					return err
				}
				var items []struct {
					ID         string  `json:"id"`
					Kind       string  `json:"kind"`
					Provenance string  `json:"provenance"`
					Trust      float64 `json:"trust"`
					Content    string  `json:"content"`
				}
				if err := json.Unmarshal(res, &items); err != nil {
					return err
				}
				if len(items) == 0 {
					fmt.Println("no memory writes awaiting approval")
					return nil
				}
				for _, w := range items {
					fmt.Printf("%s  [%s/%s trust %.1f]  %s\n",
						w.ID, w.Kind, w.Provenance, w.Trust, w.Content)
				}
				return nil
			})
		},
	}

	approve := &cobra.Command{
		Use:   "approve <id>",
		Short: "Approve a staged memory write (commits it to long-term memory)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("memory.approve", map[string]any{"id": args[0]}, 30*time.Second)
				if err != nil {
					return err
				}
				var body struct {
					Approved bool    `json:"approved"`
					NodeID   *string `json:"node_id"`
				}
				if err := json.Unmarshal(res, &body); err != nil {
					return err
				}
				if body.Approved && body.NodeID != nil {
					fmt.Printf("approved → %s\n", *body.NodeID)
				} else {
					fmt.Println("no such pending write (already resolved or expired)")
				}
				return nil
			})
		},
	}

	reject := &cobra.Command{
		Use:   "reject <id>",
		Short: "Reject a staged memory write (discards it)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("memory.reject", map[string]any{"id": args[0]}, 30*time.Second)
				if err != nil {
					return err
				}
				var body struct {
					Removed bool `json:"removed"`
				}
				if err := json.Unmarshal(res, &body); err != nil {
					return err
				}
				if body.Removed {
					fmt.Println("rejected")
				} else {
					fmt.Println("no such pending write")
				}
				return nil
			})
		},
	}

	cmd.AddCommand(pending, approve, reject)
	return cmd
}
