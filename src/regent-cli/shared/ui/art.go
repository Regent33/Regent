// Dotted braille art for the Regent identity: the "REGENT" wordmark and the
// kneeling-king mark, both rasterised onto a 1-bit canvas and packed into
// braille (2×4 dots per glyph) so they read as a fine pixel grid — the Hermes
// aesthetic in Regent's silver/teal palette.
package ui

import (
	"math"
	"strings"
)

// ── 1-bit canvas + braille packing ──────────────────────────────────────────

type canvas struct {
	w, h int
	px   [][]bool
}

func newCanvas(w, h int) *canvas {
	px := make([][]bool, h)
	for i := range px {
		px[i] = make([]bool, w)
	}
	return &canvas{w, h, px}
}

func (c *canvas) set(x, y int) {
	if x >= 0 && x < c.w && y >= 0 && y < c.h {
		c.px[y][x] = true
	}
}

func (c *canvas) rect(x0, y0, x1, y1 int) {
	for y := y0; y <= y1; y++ {
		for x := x0; x <= x1; x++ {
			c.set(x, y)
		}
	}
}

func (c *canvas) disc(cx, cy, r int) {
	for y := cy - r; y <= cy+r; y++ {
		for x := cx - r; x <= cx+r; x++ {
			dx, dy := float64(x-cx), float64(y-cy)
			if dx*dx+dy*dy <= float64(r*r) {
				c.set(x, y)
			}
		}
	}
}

// stroke fills a thick line segment (capsule of diameter th).
func (c *canvas) stroke(x0, y0, x1, y1 int, th float64) {
	fx0, fy0, fx1, fy1 := float64(x0), float64(y0), float64(x1), float64(y1)
	r := th / 2
	for y := 0; y < c.h; y++ {
		for x := 0; x < c.w; x++ {
			if segDist(float64(x), float64(y), fx0, fy0, fx1, fy1) <= r {
				c.set(x, y)
			}
		}
	}
}

func segDist(px, py, x0, y0, x1, y1 float64) float64 {
	dx, dy := x1-x0, y1-y0
	if dx == 0 && dy == 0 {
		return math.Hypot(px-x0, py-y0)
	}
	t := ((px-x0)*dx + (py-y0)*dy) / (dx*dx + dy*dy)
	t = math.Max(0, math.Min(1, t))
	return math.Hypot(px-(x0+t*dx), py-(y0+t*dy))
}

// dotBits maps a 2×4 block to braille dot bit values (U+2800 base).
var dotBits = [4][2]rune{
	{0x01, 0x08}, {0x02, 0x10}, {0x04, 0x20}, {0x40, 0x80},
}

func packBraille(c *canvas) []string {
	var rows []string
	for cy := 0; cy < c.h; cy += 4 {
		var sb strings.Builder
		for cx := 0; cx < c.w; cx += 2 {
			var bits rune
			for dy := 0; dy < 4; dy++ {
				for dx := 0; dx < 2; dx++ {
					y, x := cy+dy, cx+dx
					if y < c.h && x < c.w && c.px[y][x] {
						bits |= dotBits[dy][dx]
					}
				}
			}
			sb.WriteRune(0x2800 + bits)
		}
		rows = append(rows, sb.String())
	}
	return rows
}

// ── "REGENT" wordmark ────────────────────────────────────────────────────────
// A 5×7 pixel font, scaled and stamped onto the canvas, then braille-packed
// and rendered as a vertical silver gradient — matching the dotted king.

var glyphs = map[rune][]string{
	'R': {"####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"},
	'E': {"#####", "#....", "#....", "####.", "#....", "#....", "#####"},
	'G': {".####", "#....", "#....", "#.###", "#...#", "#...#", ".####"},
	'N': {"#...#", "##..#", "#.#.#", "#.#.#", "#..##", "#...#", "#...#"},
	'T': {"#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."},
}

// stampGlyph draws one glyph at (ox, oy), each source pixel a scale×scale block.
func (c *canvas) stampGlyph(glyph []string, ox, oy, scale int) {
	for gy, row := range glyph {
		for gx, ch := range row {
			if ch != '#' {
				continue
			}
			for dy := 0; dy < scale; dy++ {
				for dx := 0; dx < scale; dx++ {
					c.set(ox+gx*scale+dx, oy+gy*scale+dy)
				}
			}
		}
	}
}

// packHalfBlocks renders the canvas with vertical half-block glyphs (▀ ▄ █),
// pairing two pixel rows per text row. Unlike braille, each source pixel maps
// to exactly one character cell, so pixel-font letters stay crisp and legible.
func packHalfBlocks(c *canvas) []string {
	var rows []string
	for cy := 0; cy < c.h; cy += 2 {
		var sb strings.Builder
		for x := 0; x < c.w; x++ {
			top := c.px[cy][x]
			bot := cy+1 < c.h && c.px[cy+1][x]
			switch {
			case top && bot:
				sb.WriteRune('█')
			case top:
				sb.WriteRune('▀')
			case bot:
				sb.WriteRune('▄')
			default:
				sb.WriteByte(' ')
			}
		}
		rows = append(rows, sb.String())
	}
	return rows
}

// RenderBanner returns the "REGENT" wordmark as a crisp half-block pixel font
// with a top-to-bottom silver gradient.
func RenderBanner() string {
	const (
		scale  = 2
		glyphW = 5 * scale
		glyphH = 7 * scale
		gap    = scale
	)
	word := []rune("REGENT")
	width := len(word)*glyphW + (len(word)-1)*gap
	c := newCanvas(width, glyphH)
	x := 0
	for _, letter := range word {
		c.stampGlyph(glyphs[letter], x, 0, scale)
		x += glyphW + gap
	}
	rows := packHalfBlocks(c)
	var b strings.Builder
	b.WriteString("\n")
	for i, line := range rows {
		b.WriteString(Bold + shade(i, len(rows)) + line + Reset + "\n")
	}
	return b.String()
}

// ── Kneeling-king mark ───────────────────────────────────────────────────────
// A crowned figure in profile, kneeling on one knee, head bowed, facing right:
// crown + bowed head upper-left, a thick diagonal back, a prominent horizontal
// thigh, the front foot planted lower-right, the kneeling leg lower-left, with
// a triangular negative space in the middle. Teal crown, silver body.

const (
	kingW, kingH  = 34, 52
	kingCrownRows = 2 // leading braille rows (4 px each) that are crown → teal
)

func buildKing() *canvas {
	c := newCanvas(kingW, kingH)

	// Crown: band + four crenellations, upper-left.
	c.rect(5, 7, 15, 9)
	c.rect(5, 3, 6, 7)
	c.rect(8, 3, 9, 7)
	c.rect(11, 3, 12, 7)
	c.rect(14, 3, 15, 7)

	// Bowed head (rounded) just below the crown.
	c.rect(7, 10, 14, 16)
	c.disc(10, 13, 4)
	c.rect(11, 16, 14, 19) // neck

	// Back/torso: thick diagonal sweeping down to the hips.
	c.stroke(13, 17, 22, 29, 8)
	c.disc(21, 30, 5) // hip joint

	// Front leg: a prominent horizontal thigh, then the shin dropping to a
	// foot planted at the lower right.
	c.stroke(20, 30, 30, 31, 8)
	c.stroke(29, 30, 29, 45, 7)
	c.rect(27, 44, 33, 47)

	// Back leg: thigh down-left to the knee, then shin/foot laid on the
	// ground pointing left (the kneeling leg).
	c.stroke(19, 31, 10, 41, 7)
	c.stroke(10, 41, 4, 45, 6)
	c.rect(2, 44, 13, 47)

	return c
}

// RenderKing returns the dotted mark: teal crown, uniform bright-silver body
// (the banner carries the gradient; a flat body reads cleaner as a sprite).
func RenderKing() []string {
	raw := packBraille(buildKing())
	out := make([]string, len(raw))
	for i, line := range raw {
		if i < kingCrownRows {
			out[i] = Teal + line + Reset
		} else {
			out[i] = White + line + Reset
		}
	}
	return out
}
