package rpc

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func hasEntry(ss []string, want string) bool {
	for _, s := range ss {
		if s == want {
			return true
		}
	}
	return false
}

func countPrefix(ss []string, prefix string) int {
	n := 0
	for _, s := range ss {
		if strings.HasPrefix(s, prefix) {
			n++
		}
	}
	return n
}

func TestAppendDotEnvMergesMissingKeysOnly(t *testing.T) {
	dir := t.TempDir()
	contents := "# a comment\n\nREGENT_API_KEY=\"sk-secret\"\nALREADY=fromfile\n"
	if err := os.WriteFile(filepath.Join(dir, ".env"), []byte(contents), 0o600); err != nil {
		t.Fatal(err)
	}

	// ALREADY is already in the process env → .env must not override it.
	out := appendDotEnv([]string{"ALREADY=fromenv"}, dir)

	if !hasEntry(out, "REGENT_API_KEY=sk-secret") {
		t.Fatalf("key not merged (or quotes not stripped): %v", out)
	}
	if countPrefix(out, "ALREADY=") != 1 || !hasEntry(out, "ALREADY=fromenv") {
		t.Fatalf("real env must win over .env: %v", out)
	}
	for _, e := range out {
		if e == "" || strings.HasPrefix(e, "#") {
			t.Fatalf("blank/comment leaked into env: %q", e)
		}
	}
}

func TestAppendDotEnvNoFileIsNoop(t *testing.T) {
	out := appendDotEnv([]string{"X=1"}, t.TempDir()) // no .env present
	if len(out) != 1 || out[0] != "X=1" {
		t.Fatalf("expected unchanged env, got %v", out)
	}
}
