package setup

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestSecureWriteFileContentAtomicOverwriteAndPerms(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, ".env")

	if err := secureWriteFile(p, []byte("REGENT_API_KEY=sk-1\n")); err != nil {
		t.Fatalf("first write: %v", err)
	}
	if got, _ := os.ReadFile(p); string(got) != "REGENT_API_KEY=sk-1\n" {
		t.Fatalf("content = %q", got)
	}

	// Overwriting an existing file must succeed (atomic rename replaces it).
	if err := secureWriteFile(p, []byte("REGENT_API_KEY=sk-2\n")); err != nil {
		t.Fatalf("overwrite: %v", err)
	}
	if got, _ := os.ReadFile(p); string(got) != "REGENT_API_KEY=sk-2\n" {
		t.Fatalf("overwrite content = %q", got)
	}

	// No leftover temp files in the dir.
	entries, _ := os.ReadDir(dir)
	if len(entries) != 1 {
		t.Fatalf("expected only .env, got %d entries", len(entries))
	}

	if runtime.GOOS != "windows" {
		info, err := os.Stat(p)
		if err != nil {
			t.Fatal(err)
		}
		if info.Mode().Perm() != 0o600 {
			t.Fatalf("perm = %o, want 0600", info.Mode().Perm())
		}
	}
}

func TestWriteEnvUpsertsKeyPreservingOtherLines(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, ".env"),
		[]byte("OTHER=keep\nREGENT_API_KEY=old\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := writeEnv(dir, "new-key"); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(filepath.Join(dir, ".env"))
	s := string(got)
	if !strings.Contains(s, "OTHER=keep") {
		t.Fatalf("dropped unrelated line: %q", s)
	}
	if !strings.Contains(s, "REGENT_API_KEY=new-key") {
		t.Fatalf("key not upserted: %q", s)
	}
	if strings.Contains(s, "REGENT_API_KEY=old") {
		t.Fatalf("stale key survived: %q", s)
	}
}
