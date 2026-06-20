// Package ui — the Regent visual identity for terminal surfaces: a silver/
// teal palette, a silver outlined panel, and the dotted braille art (banner +
// kneeling-king mark) in art.go. Teal is the accent; silver/white is the main
// tone, rendered as a top-to-bottom gradient across the 256-color grey ramp.
package ui

import (
	"regexp"
	"strings"
)

// ANSI palette.
const (
	Reset  = "\x1b[0m"
	Teal   = "\x1b[38;5;44m"
	TealD  = "\x1b[38;5;30m"
	Silver = "\x1b[38;5;252m" // panel borders / mid silver
	White  = "\x1b[97m"
	Grey   = "\x1b[38;5;245m"
	Bold   = "\x1b[1m"
)

// silverRamp runs bright white → dim grey; gradient lines index into it.
var silverRamp = []string{
	"\x1b[38;5;231m", "\x1b[38;5;255m", "\x1b[38;5;253m", "\x1b[38;5;251m",
	"\x1b[38;5;250m", "\x1b[38;5;248m", "\x1b[38;5;246m", "\x1b[38;5;245m",
}

// shade maps row i of n into the silver ramp (clamped).
func shade(i, n int) string {
	if n <= 1 {
		return silverRamp[0]
	}
	idx := i * (len(silverRamp) - 1) / (n - 1)
	if idx >= len(silverRamp) {
		idx = len(silverRamp) - 1
	}
	return silverRamp[idx]
}

// ── Panel ──────────────────────────────────────────────────────────────────

var ansiPattern = regexp.MustCompile("\x1b\\[[0-9;]*m")

func visibleLen(s string) int {
	return len([]rune(ansiPattern.ReplaceAllString(s, "")))
}

func padTo(s string, width int) string {
	if n := width - visibleLen(s); n > 0 {
		return s + strings.Repeat(" ", n)
	}
	return s
}

// SideBySide pairs the king mark with info lines into panel-ready rows.
func SideBySide(info []string) []string {
	king := RenderKing()
	kingWidth := 0
	for _, k := range king {
		if w := visibleLen(k); w > kingWidth {
			kingWidth = w
		}
	}
	rows := max(len(info), len(king))
	out := make([]string, rows)
	for i := range rows {
		left := strings.Repeat(" ", kingWidth)
		if i < len(king) {
			left = king[i]
		}
		right := ""
		if i < len(info) {
			right = info[i]
		}
		out[i] = left + "   " + right
	}
	return out
}

// Panel draws rows inside a rounded SILVER outline with the title set into
// the top border. Width is measured ignoring ANSI codes so the right edge
// never tears.
func Panel(title string, rows []string) string {
	inner := visibleLen(title) + 4
	for _, r := range rows {
		if w := visibleLen(r); w > inner {
			inner = w
		}
	}
	border := Silver
	var b strings.Builder
	b.WriteString(border + "╭─ " + Reset + Bold + White + title + Reset +
		" " + border + strings.Repeat("─", inner-visibleLen(title)-3) + "╮" + Reset + "\n")
	for _, r := range rows {
		b.WriteString(border + "│ " + Reset + padTo(r, inner) + border + " │" + Reset + "\n")
	}
	b.WriteString(border + "╰" + strings.Repeat("─", inner+2) + "╯" + Reset)
	return b.String()
}

// Header formats a bold teal section heading (the accent color).
func Header(text string) string { return Bold + Teal + text + Reset }

// Label formats a "key: value" line — dim-teal key, bold-white value.
func Label(key, value string) string {
	return TealD + key + ": " + Reset + Bold + White + value + Reset
}

// Note formats secondary grey text.
func Note(text string) string { return Grey + text + Reset }
