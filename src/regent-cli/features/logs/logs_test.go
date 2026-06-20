package logs

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLatestLogPicksNewestByName(t *testing.T) {
	dir := t.TempDir()
	// Daily appender names sort chronologically by their date suffix.
	for _, name := range []string{
		"regent.log.2026-06-15", "regent.log.2026-06-17", "regent.log.2026-06-16",
	} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	// An unrelated file must be ignored by the regent.log* glob.
	_ = os.WriteFile(filepath.Join(dir, "notes.txt"), []byte("x"), 0o644)

	got, err := latestLog(dir)
	if err != nil {
		t.Fatal(err)
	}
	if filepath.Base(got) != "regent.log.2026-06-17" {
		t.Fatalf("latestLog = %q, want the 06-17 file", got)
	}
}

func TestLatestLogErrorsWhenNoLogs(t *testing.T) {
	if _, err := latestLog(t.TempDir()); err == nil {
		t.Fatal("expected an error when no log files exist")
	}
}
