// Package cron — `regent cron list|add|remove` (prospective memory).
package cron

import (
	"encoding/json"
	"fmt"
	"time"

	"regent/cli/shared/rpc"

	"github.com/spf13/cobra"
)

type withClient func(func(*rpc.Client) error) error

// Command builds the `cron` subtree.
func Command(run withClient) *cobra.Command {
	cmd := &cobra.Command{Use: "cron", Short: "Manage scheduled jobs"}

	list := &cobra.Command{
		Use:   "list",
		Short: "List scheduled jobs",
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("cron.list", map[string]any{}, 30*time.Second)
				if err != nil {
					return err
				}
				var jobs []struct {
					ID        string  `json:"id"`
					Name      string  `json:"name"`
					Prompt    string  `json:"prompt"`
					Enabled   bool    `json:"enabled"`
					NextRunAt float64 `json:"next_run_at"`
				}
				if err := json.Unmarshal(res, &jobs); err != nil {
					return err
				}
				if len(jobs) == 0 {
					fmt.Println("no cron jobs")
					return nil
				}
				for _, j := range jobs {
					state := "enabled"
					if !j.Enabled {
						state = "disabled"
					}
					next := time.Unix(int64(j.NextRunAt), 0).Format("2006-01-02 15:04")
					fmt.Printf("%s  %-20s  %-8s  next %s  — %s\n", j.ID, j.Name, state, next, j.Prompt)
				}
				return nil
			})
		},
	}

	var schedule, prompt string
	add := &cobra.Command{
		Use:   "add <name>",
		Short: "Add a job (schedules: 30m / 2h / 1d, daily HH:MM, @epoch)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("cron.add", map[string]any{
					"name": args[0], "schedule": schedule, "prompt": prompt,
				}, 30*time.Second)
				if err != nil {
					return err
				}
				var body struct {
					ID string `json:"id"`
				}
				if err := json.Unmarshal(res, &body); err != nil {
					return err
				}
				fmt.Printf("added %s\n", body.ID)
				return nil
			})
		},
	}
	add.Flags().StringVar(&schedule, "schedule", "", "when to run (required)")
	add.Flags().StringVar(&prompt, "prompt", "", "what the job agent should do (required)")
	_ = add.MarkFlagRequired("schedule")
	_ = add.MarkFlagRequired("prompt")

	remove := &cobra.Command{
		Use:   "remove <job-id>",
		Short: "Remove a job",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("cron.remove", map[string]any{"id": args[0]}, 30*time.Second)
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
					fmt.Println("removed")
				} else {
					fmt.Println("no job with that id")
				}
				return nil
			})
		},
	}

	cmd.AddCommand(list, add, remove)
	return cmd
}
