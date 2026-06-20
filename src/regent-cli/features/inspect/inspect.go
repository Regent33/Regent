// Package inspect — `regent model[/list/set]`, `regent skills`, `regent
// config`. `model set` is the one mutating verb (config set lands later).
package inspect

import (
	"encoding/json"
	"fmt"
	"time"

	"regent/cli/shared/rpc"

	"github.com/spf13/cobra"
)

type withClient func(func(*rpc.Client) error) error

// ModelCommand — `regent model` shows the active model; `model list` shows the
// catalog; `model set <id>` switches the model for new sessions.
func ModelCommand(run withClient) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "model",
		Short: "Show the active model",
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("model.get", map[string]any{}, 30*time.Second)
				if err != nil {
					return err
				}
				var body struct {
					Model string `json:"model"`
				}
				if err := json.Unmarshal(res, &body); err != nil {
					return err
				}
				fmt.Println(body.Model)
				return nil
			})
		},
	}

	cmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List known models",
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("model.list", map[string]any{}, 30*time.Second)
				if err != nil {
					return err
				}
				var items []struct {
					ID          string `json:"id"`
					DisplayName string `json:"display_name"`
					Current     bool   `json:"current"`
				}
				if err := json.Unmarshal(res, &items); err != nil {
					return err
				}
				for _, m := range items {
					marker := "  "
					if m.Current {
						marker = "* "
					}
					fmt.Printf("%s%-20s %s\n", marker, m.ID, m.DisplayName)
				}
				return nil
			})
		},
	})

	cmd.AddCommand(&cobra.Command{
		Use:   "set <model-id>",
		Short: "Switch the model used for new sessions",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("model.set", map[string]any{"model": args[0]}, 30*time.Second)
				if err != nil {
					return err
				}
				var body struct {
					Model string `json:"model"`
					Note  string `json:"note"`
				}
				if err := json.Unmarshal(res, &body); err != nil {
					return err
				}
				fmt.Printf("model set to %s\n(%s)\n", body.Model, body.Note)
				return nil
			})
		},
	})

	return cmd
}

// SkillsCommand — `regent skills` lists the skill library index.
func SkillsCommand(run withClient) *cobra.Command {
	return &cobra.Command{
		Use:   "skills",
		Short: "List learned skills",
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("skills.list", map[string]any{}, 30*time.Second)
				if err != nil {
					return err
				}
				var skills []struct {
					Name        string   `json:"name"`
					Description string   `json:"description"`
					Tags        []string `json:"tags"`
				}
				if err := json.Unmarshal(res, &skills); err != nil {
					return err
				}
				if len(skills) == 0 {
					fmt.Println("no skills yet — the agent learns them from reviewed sessions")
					return nil
				}
				for _, s := range skills {
					fmt.Printf("%-24s %s\n", s.Name, s.Description)
				}
				return nil
			})
		},
	}
}

// ConfigCommand — `regent config` prints the daemon's loaded config.
func ConfigCommand(run withClient) *cobra.Command {
	return &cobra.Command{
		Use:   "config",
		Short: "Show the loaded config.yaml",
		RunE: func(cmd *cobra.Command, args []string) error {
			return run(func(c *rpc.Client) error {
				res, err := c.Call("config.get", map[string]any{}, 30*time.Second)
				if err != nil {
					return err
				}
				var pretty map[string]any
				if err := json.Unmarshal(res, &pretty); err != nil {
					return err
				}
				out, err := json.MarshalIndent(pretty, "", "  ")
				if err != nil {
					return err
				}
				fmt.Println(string(out))
				return nil
			})
		},
	}
}
