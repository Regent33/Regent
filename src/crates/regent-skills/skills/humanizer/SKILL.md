---
name: humanizer
description: "Humanize text: strip AI-isms and add real voice."
version: 1.0.0
created_by: bundled
pinned: true
tags: [writing, editing, humanize, voice, prose]
---

Strip AI-writing tells and put a real voice back into the text. Based on
Wikipedia's "Signs of AI Writing" guide (WikiProject AI Cleanup).

**Key insight:** LLMs guess the statistically likely next token, which is how
these tells get baked in — puffed-up significance, hedged claims, tidy
rule-of-threes, uniform rhythm.

## When to use
User asks to "humanize", "de-AI", or "de-slop" text; rewrite a draft (post,
PR description, docs, email) to sound natural; match their voice from a
sample; or review text for AI tells before publishing. Also apply to your
own user-facing prose — release notes, PR descriptions, long explanations.

## How text arrives
1. **Inline** — pasted into the message. Rewrite in place.
2. **File** — `read_file` it, then `file_edit` (targeted section) or
   `write_file` (full rewrite). Always show the user a diff or the changed
   section — never silently overwrite.
3. **Voice sample** — user gives their own past writing to match. Read it
   first: sentence length, word-choice level, how paragraphs open,
   punctuation habits, recurring tics. Match those patterns, not the
   default voice below.

## Process
1. Scan for the patterns in the table below.
2. Rewrite problem sections; preserve meaning; match intended tone (or the
   voice sample, if given).
3. Add soul — see below. Removing bad patterns isn't enough.
4. Ask yourself: "What makes this so obviously AI generated?" List remaining
   tells briefly, revise once more, then present the final version.

## Add soul, not just remove tells
Soulless writing is as obvious as slop: uniform sentence length, no
opinions, no first person, no acknowledged uncertainty, reads like a press
release.
- **Have opinions.** "I genuinely don't know how to feel about this" beats a
  neutral pros/cons list.
- **Vary rhythm.** Short punchy sentences. Then longer ones that take their
  time getting where they're going.
- **Acknowledge complexity.** "Impressive but also kind of unsettling" beats
  "impressive."
- **Use "I" when it fits.** Signals a real person thinking.
- **Let some mess in.** Tangents and asides are human; perfect structure
  reads algorithmic.
- **Be specific about feelings**, not generic ("concerning" → say what's
  actually unsettling about it).

## The tells

| # | Pattern | Watch for | Fix |
|---|---------|-----------|-----|
| 1 | Inflated significance | stands/serves as, testament, pivotal, evolving landscape | State the plain fact, drop the framing |
| 2 | Notability padding | "featured in", "active social media presence" | Cite one specific claim with a source |
| 3 | -ing tacked-on depth | highlighting, underscoring, reflecting, fostering | Cut the clause or make it a real sentence |
| 4 | Promotional language | boasts, vibrant, nestled, breathtaking, must-visit | Plain description, no adjectives selling it |
| 5 | Vague attribution | "experts say", "observers note" (no source) | Name the source or drop the claim |
| 6 | Formulaic "Challenges" section | "Despite its... faces challenges... continues to thrive" | Specific facts, not a template arc |
| 7 | Overused AI vocabulary | delve, crucial, intricate, tapestry, underscore, pivotal | Plainer synonym or cut |
| 8 | Copula avoidance | serves as, stands as, boasts, features (for "is/has") | Use is/are/has |
| 9 | Negative parallelism | "not just X, it's Y"; tailing negations ("no guessing") | Say the positive claim directly |
| 10 | Rule-of-three overuse | forced triads for "comprehensiveness" | Cut to what's actually true, any count |
| 11 | Elegant variation | protagonist/main character/hero cycling for one entity | Reuse the same word |
| 12 | False ranges | "from X to Y" where X, Y aren't a real scale | List the actual items |
| 13 | Passive/subjectless fragments | "No configuration needed." | Name the actor: "You don't need..." |
| 14 | Em dash overuse | — used as punchy connector | Comma, period, or parens |
| 15 | Boldface mechanically | bolding every key term in prose | Bold only what's actually being flagged |
| 16 | Inline-header bullet lists | **Label:** sentence restating the label | Merge into normal prose |
| 17 | Title Case Headings | every word capitalized | Sentence case |
| 18 | Emoji decoration | 🚀 before headings/bullets | Remove |
| 19 | Curly quotes | "smart quotes" | Straight quotes |
| 20 | Chatbot artifacts | "I hope this helps!", "Here is a...", "Let me know" | Delete — this isn't a chat reply |
| 21 | Knowledge-cutoff hedges | "as of my last update", "details are limited" | State what's actually known, sourced |
| 22 | Sycophantic tone | "Great question!", "You're absolutely right" | Cut the flattery, answer directly |
| 23 | Filler phrases | "in order to", "due to the fact that", "at this point in time" | "to", "because", "now" |
| 24 | Excessive hedging | "could potentially possibly be argued" | "may" |
| 25 | Generic upbeat closer | "the future looks bright", "exciting times ahead" | A concrete next fact, or nothing |
| 26 | Uniform hyphenation | third-party, high-quality, real-time hyphenated every time | Hyphenate inconsistently, like a human would |
| 27 | Persuasive-authority tropes | "the real question is", "at its core", "what really matters" | State the point plainly |
| 28 | Signposting | "let's dive in", "here's what you need to know" | Just say the thing |
| 29 | Fragmented headers | heading followed by a one-line restatement before real content | Cut the restatement |

## Output
1. Draft rewrite.
2. Brief self-audit of remaining tells.
3. Final rewrite.
4. If from a file, apply via `file_edit`/`write_file` and show what changed.

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
