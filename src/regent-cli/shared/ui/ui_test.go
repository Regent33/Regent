package ui

import (
	"strings"
	"testing"
)

func plain(s string) string { return ansiPattern.ReplaceAllString(s, "") }

func TestVisibleLenIgnoresAnsi(t *testing.T) {
	if got := visibleLen(Teal + "abc" + Reset); got != 3 {
		t.Fatalf("visibleLen = %d, want 3", got)
	}
}

func TestPadToPadsByVisibleWidthAndNeverTruncates(t *testing.T) {
	padded := padTo(Teal+"ab"+Reset, 5)
	if visibleLen(padded) != 5 {
		t.Fatalf("padded visible width = %d, want 5", visibleLen(padded))
	}
	// Already wider than the target → returned unchanged (no truncation).
	if got := padTo("abcdef", 3); got != "abcdef" {
		t.Fatalf("padTo truncated: %q", got)
	}
}

func TestLabelRendersKeyAndValue(t *testing.T) {
	if got := plain(Label("model", "sonnet")); got != "model: sonnet" {
		t.Fatalf("label = %q", got)
	}
}

func TestPanelFramesTitleAndRows(t *testing.T) {
	p := Panel("Status", []string{"ready", "ok"})
	flat := plain(p)
	for _, want := range []string{"Status", "ready", "ok"} {
		if !strings.Contains(flat, want) {
			t.Fatalf("panel missing %q in:\n%s", want, flat)
		}
	}
	// Rounded border corners are present.
	for _, corner := range []string{"╭", "╮", "╰", "╯"} {
		if !strings.Contains(p, corner) {
			t.Fatalf("panel missing border corner %q", corner)
		}
	}
}
