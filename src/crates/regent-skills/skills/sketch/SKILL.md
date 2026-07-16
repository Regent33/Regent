---
name: sketch
description: "Throwaway HTML mockups: 2-3 design variants to compare."
version: 1.0.0
created_by: bundled
pinned: true
tags: [sketch, mockup, design, ui, prototype]
---

Use when the user wants to **see a design direction before committing** —
exploring a UI idea as disposable HTML mockups. Goal: 2–3 interactive
variants to compare side by side, not shippable code.

Load when the user says "sketch this screen", "show me what X could look
like", "compare layout A vs B", "give me 2–3 takes on this UI", "mockup this
before I build".

**Don't use when:** they want a production component (build it properly);
they want one polished artifact (just build it); they want a diagram (this
isn't that); the design's already locked (just build it).

## Core method
```
intake → variants → head-to-head → pick winner (or iterate)
```

### 1. Intake (skip if already answered)
One question at a time:
1. **Feel.** "What should this feel like?" — adjectives/vibe beat "minimal".
2. **References.** Actual apps/sites that capture the feel.
3. **Core action.** The single most important thing a user does here — every
   variant must serve this, or it's just decoration.

### 2. Variants — 2–3, never 1, rarely 4+
Each is a complete, standalone HTML file. Build, don't describe. Each
variant takes a **different design stance**, not different pixel values —
two variants differing only in accent color are wasted effort. Pick one axis
and pull apart:
- Density: compact / airy / ultra-dense
- Emphasis: content-first / action-first / tool-first
- Aesthetic: editorial / utilitarian / playful
- Layout: single-column / sidebar / split-pane
- Grounding: card-based / bare-content / document-style

Name variants by stance, not number:
```
sketches/001-calm-editorial/{index.html,README.md}
sketches/001-utilitarian-dense/{index.html,README.md}
sketches/001-playful-split/{index.html,README.md}
```

### 3. Make them real HTML
- Inline `<style>`, no build step, no external CSS file
- System fonts or one Google Font via `<link>`
- Realistic fake content — real sentences and names, not lorem ipsum
- **Interactive**: clickable links, real hovers, at least one state
  transition (open/close, filter, toggle). A frozen static mockup is worse
  than a sloppy animated one.

Default reset + system font stack:
```html
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
                 "Helvetica Neue", Arial, sans-serif;
    color: #1a1a1a; background: #fafafa; line-height: 1.5;
  }
</style>
```

**Verify before showing the user.** If a browser or screenshot tool is
available, open the file and check for broken layout, overlapping text, or
unstyled elements. If not, at minimum re-read the HTML for obvious mistakes
(unclosed tags, missing asset paths) before handing it over.

### 4. Variant README
```markdown
## Variant: {stance name}
### Design stance
One sentence — the principle driving this variant.
### Key choices
Layout / Typography / Color / Interaction
### Trade-offs
Strong at: ... / Weak at: ...
### Best for
The user or use case this variant actually serves
```

### 5. Head-to-head
Opinionate, don't just list:
```markdown
| Dimension | Calm editorial | Utilitarian dense | Playful split |
|---|---|---|---|
| Density | Low | High | Medium |
| Primary action visibility | Low | High | Medium |
| Scan-ability | High | Medium | Low |

**My take:** Utilitarian dense for power users, calm editorial for
content-forward audiences. Playful split is weakest — tries to do both.
```
Let the user pick a winner, combine two into a hybrid, or ask for another
round.

## Theming
If the project has an existing visual identity, put shared tokens in
`sketches/themes/tokens.css` and `@import` in each variant. Don't
over-tokenize a throwaway — three colors and one font is usually enough.

## Interactivity bar
Enough when the user can click a primary action and see something happen,
see one real state transition, and hover recognizable affordances. More is
over-engineering a throwaway; less is a screenshot.

## Frontier mode
If sketches exist and the user asks "what next?": consistency gaps (winning
variants from different sketches never composed together); unsketched
screens; missing states (empty/loading/error, not just happy path);
responsive gaps; unsketched interaction patterns. Propose 2–4 named
candidates, let the user pick.

## Output
- `sketches/NNN-stance-name/index.html` + `README.md` per variant
- Tell the user how to open them (`start` on Windows, `open` on macOS,
  `xdg-open` on Linux)
- Keep variants disposable — one worth keeping gets promoted into real
  project code, not curated as an asset

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
