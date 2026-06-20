// Package sessions — `regent sessions list|search`.
package sessions

import (
	"encoding/json"
	"fmt"
	"time"

	"regent/cli/shared/rpc"

	"github.com/spf13/cobra"
)

type withClient func(func(*rpc.Client) error) error

// Command builds the `sessions` subtree.
func Command(run withClient) *cobra.Command {
	cmd := &cobra.Command{Use: "sessions", Short: "List and search past sessions"}

	var limit int
	list := &cobra.Command{
		Use:   "list",
		Short: "List recent sessions",
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("session.list", map[string]any{"limit": limit}, 30*time.Second)
				if err != nil {
					return err
				}
				var items []struct {
					SessionID    string  `json:"session_id"`
					Source       string  `json:"source"`
					Model        *string `json:"model"`
					MessageCount int     `json:"message_count"`
					StartedAt    float64 `json:"started_at"`
				}
				if err := json.Unmarshal(res, &items); err != nil {
					return err
				}
				if len(items) == 0 {
					fmt.Println("no sessions yet")
					return nil
				}
				for _, s := range items {
					model := "-"
					if s.Model != nil {
						model = *s.Model
					}
					started := time.Unix(int64(s.StartedAt), 0).Format("2006-01-02 15:04")
					fmt.Printf("%s  %-8s  %-24s  %3d msgs  %s\n",
						s.SessionID, s.Source, model, s.MessageCount, started)
				}
				return nil
			})
		},
	}
	list.Flags().IntVar(&limit, "limit", 20, "max sessions to show")

	search := &cobra.Command{
		Use:   "search <query>",
		Short: "Full-text search across all session messages",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			query := args[0]
			return run(func(c *rpc.Client) error {
				res, err := c.Call("session.search",
					map[string]any{"query": query, "limit": 20}, 30*time.Second)
				if err != nil {
					return err
				}
				var hits []struct {
					SessionID string `json:"session_id"`
					Role      string `json:"role"`
					Snippet   string `json:"snippet"`
				}
				if err := json.Unmarshal(res, &hits); err != nil {
					return err
				}
				if len(hits) == 0 {
					fmt.Println("no matches")
					return nil
				}
				for _, h := range hits {
					fmt.Printf("%s [%s] %s\n", h.SessionID, h.Role, h.Snippet)
				}
				return nil
			})
		},
	}

	cmd.AddCommand(list, search)
	return cmd
}
