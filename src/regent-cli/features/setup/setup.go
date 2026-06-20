// Package setup wires `regent setup` — first-time configuration: pick a
// provider + model and store the API key. Secrets go to $REGENT_HOME/.env
// (loaded when the daemon is spawned); behavior goes to config.yaml.
package setup

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"
)

var providers = map[string]bool{
	"anthropic": true, "openai": true, "openrouter": true, "groq": true,
	"deepseek": true, "together": true, "ollama": true,
}

// Command builds `regent setup`. home resolves the active profile's REGENT_HOME.
func Command(home func() string) *cobra.Command {
	var provider, model, baseURL, key string
	cmd := &cobra.Command{
		Use:   "setup",
		Short: "First-time setup: provider, model, and API key",
		Args:  cobra.NoArgs,
		RunE: func(_ *cobra.Command, _ []string) error {
			return run(home(), provider, model, baseURL, key)
		},
	}
	f := cmd.Flags()
	f.StringVar(&provider, "provider", "", "anthropic|openai|openrouter|groq|deepseek|together|ollama")
	f.StringVar(&model, "model", "", "default model id")
	f.StringVar(&baseURL, "base-url", "", "override the provider base URL (optional)")
	f.StringVar(&key, "key", "", "API key (else taken from REGENT_API_KEY, else prompted)")
	return cmd
}

func run(home, provider, model, baseURL, key string) error {
	in := bufio.NewScanner(os.Stdin)
	if provider == "" {
		provider = ask(in, "Provider", "anthropic")
	}
	if !providers[provider] {
		return fmt.Errorf("unknown provider %q (choose: anthropic, openai, openrouter, groq, deepseek, together, ollama)", provider)
	}
	if model == "" {
		model = ask(in, "Default model", "claude-sonnet-4-6")
	}
	if key == "" {
		key = os.Getenv("REGENT_API_KEY")
	}
	if key == "" {
		fmt.Println("Enter the API key (input is visible — or re-run with --key, or set REGENT_API_KEY):")
		key = ask(in, "API key", "")
	}

	// 0700: $REGENT_HOME holds .env + state.db; keep it owner-only.
	if err := os.MkdirAll(home, 0o700); err != nil {
		return fmt.Errorf("create REGENT_HOME %s: %w", home, err)
	}
	if err := writeEnv(home, key); err != nil {
		return err
	}
	if err := writeConfigIfAbsent(home, provider, model, baseURL); err != nil {
		return err
	}
	fmt.Printf("\nSetup complete (REGENT_HOME=%s).\nNext: `regent doctor`, then `regent chat`.\n", home)
	return nil
}

func ask(in *bufio.Scanner, label, def string) string {
	if def != "" {
		fmt.Printf("%s [%s]: ", label, def)
	} else {
		fmt.Printf("%s: ", label)
	}
	if !in.Scan() {
		return def
	}
	if v := strings.TrimSpace(in.Text()); v != "" {
		return v
	}
	return def
}

// writeEnv upserts REGENT_API_KEY in $REGENT_HOME/.env, preserving any other
// lines. Written 0600 — it holds a secret.
func writeEnv(home, key string) error {
	if key == "" {
		fmt.Println("warning: no API key set — export REGENT_API_KEY before running the agent")
		return nil
	}
	path := filepath.Join(home, ".env")
	var kept []string
	if data, err := os.ReadFile(path); err == nil {
		for _, line := range strings.Split(string(data), "\n") {
			t := strings.TrimSpace(line)
			if t == "" || strings.HasPrefix(t, "REGENT_API_KEY=") {
				continue
			}
			kept = append(kept, line)
		}
	}
	kept = append(kept, "REGENT_API_KEY="+key)
	return secureWriteFile(path, []byte(strings.Join(kept, "\n")+"\n"))
}

// secureWriteFile writes secret data to path with owner-only perms, created so
// the bytes are never briefly world-readable: a temp file in the same dir is
// created with O_EXCL at 0600 (born private, not via the umask), synced, then
// atomically renamed over the target — closing the TOCTOU window a plain
// write-then-chmod leaves open. The parent dir is tightened to 0700. On Windows
// POSIX modes are advisory; the user-profile ACLs already restrict access.
func secureWriteFile(path string, data []byte) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	_ = os.Chmod(dir, 0o700) // best-effort; no-op on Windows

	tmp := filepath.Join(dir, fmt.Sprintf(".%s.tmp.%d", filepath.Base(path), os.Getpid()))
	f, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return err
	}
	// Clean up the temp file on any failure before the rename.
	defer func() { _ = os.Remove(tmp) }()
	if _, err := f.Write(data); err != nil {
		_ = f.Close()
		return err
	}
	if err := f.Sync(); err != nil {
		_ = f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil { // atomic replace
		return err
	}
	_ = os.Chmod(path, 0o600) // ensure 0600 even if an older file had looser perms
	return nil
}

// writeConfigIfAbsent writes a minimal config.yaml only when none exists, so an
// existing (possibly richer) config is never clobbered.
func writeConfigIfAbsent(home, provider, model, baseURL string) error {
	path := filepath.Join(home, "config.yaml")
	if _, err := os.Stat(path); err == nil {
		fmt.Println("config.yaml exists — left unchanged (use `regent config set` to change provider/model)")
		return nil
	}
	var b strings.Builder
	b.WriteString("_config_version: 1\n")
	b.WriteString("model:\n")
	fmt.Fprintf(&b, "  provider: %s\n", provider)
	fmt.Fprintf(&b, "  default: %s\n", model)
	if baseURL != "" {
		fmt.Fprintf(&b, "  base_url: %q\n", baseURL)
	}
	return os.WriteFile(path, []byte(b.String()), 0o644)
}
