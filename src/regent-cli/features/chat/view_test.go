package chat

import (
	"strings"
	"testing"
)

func TestShortTruncatesOnlyLongIds(t *testing.T) {
	// <= 18 chars: returned unchanged.
	if got := short("session-123"); got != "session-123" {
		t.Fatalf("short shortened a short id: %q", got)
	}
	if exactly := strings.Repeat("b", 18); short(exactly) != exactly {
		t.Fatalf("short truncated an 18-char id: %q", short(exactly))
	}

	// > 18 chars: first 18 + ellipsis (19 runes total).
	got := short(strings.Repeat("a", 30))
	if r := []rune(got); len(r) != 19 || r[18] != '…' {
		t.Fatalf("short = %q (want 18 chars + …)", got)
	}
}
