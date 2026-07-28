#!/usr/bin/env python3
"""Emits the frozen corpus + query set for the paired memory-recall pilot.

Deterministic: no randomness, no clock. Re-running reproduces byte-identical
artifacts, which is the protocol's reproducibility condition.

Design note. The gold entries all sit in the first 20 so that the SAME gold set
is present at every N; growing N adds distractors only. That is what makes the
three points a curve about precision under load rather than three unrelated
measurements.
"""

import json
import pathlib

# ── The 15 gold entries. Present at every N. ────────────────────────────────
GOLD = [
    ("g01", "Ralph prefers tabs over spaces in Rust, and rustfmt runs on every save."),
    ("g02", "The staging cluster is called harbormaster; deploys to it are banned on Fridays."),
    ("g03", "Payments team incident hotline is extension 4417."),
    ("g04", "Ralph's preferred terminal font is Chorus at 13pt with ligatures off."),
    ("g05", "Quarterly board review is the second Tuesday of March, June, September, December."),
    ("g06", "Maya Okonkwo leads the platform team and prefers async updates over meetings."),
    ("g07", "The nightly backup job writes to the Frankfurt bucket, not the Dublin one."),
    ("g08", "Ralph is allergic to shellfish; avoid seafood restaurants when booking dinners."),
    ("g09", "The legacy billing service must not be touched before the Q3 migration finishes."),
    ("g10", "API keys for the analytics vendor rotate on the first Monday of each month."),
    ("g11", "Ralph's daughter's birthday is 14 November; he takes that day off every year."),
    ("g12", "The design system uses Archivo for body text and Chorus for display headings."),
    ("g13", "Postgres runs at version 16 in production; staging is still on 15."),
    ("g14", "Ralph drinks decaf after 2pm and declines coffee meetings later than that."),
    ("g15", "The incident postmortem template lives in the ops wiki under Blameless Reviews."),
]

# ── Near-duplicate distractors: one detail differs from a gold entry. ───────
NEAR = [
    ("d01", "The staging cluster is called lighthouse; deploys to it are banned on Mondays."),
    ("d02", "Platform team incident hotline is extension 4471."),
    ("d03", "The nightly backup job writes to the Dublin bucket every weekend."),
    ("d04", "Postgres runs at version 15 in development; CI containers use 16."),
    ("d05", "API keys for the payments vendor rotate on the last Friday of each month."),
]

# ── Filler: plausible, never gold. Deterministically expanded to reach N. ───
FILLER_TEMPLATES = [
    "Sprint {i} retrospective notes are filed in the team drive under planning.",
    "The {i}th onboarding checklist item is to request VPN access from IT.",
    "Meeting room {i} on the fourth floor has the working video conference unit.",
    "Vendor invoice batch {i} was approved by finance without exceptions.",
    "The archive bucket keeps release {i} artifacts for ninety days.",
    "Runbook section {i} covers restarting the ingestion workers safely.",
    "Ticket queue {i} is triaged every morning by the on-call engineer.",
    "Dashboard panel {i} tracks p95 latency for the public API.",
    "Config flag {i} was retired after the platform migration completed.",
    "Weekly digest {i} summarises open pull requests for the team.",
]


def filler(n):
    out = []
    for i in range(n):
        template = FILLER_TEMPLATES[i % len(FILLER_TEMPLATES)]
        out.append((f"f{i:03d}", template.format(i=i + 1)))
    return out


def build():
    # First 20 = 15 gold + 5 near-duplicates, so N=20 already exercises the
    # distractor-heavy queries.
    head = GOLD + NEAR
    assert len(head) == 20
    corpus = head + filler(180)
    assert len(corpus) == 200
    for _, text in corpus:
        assert len(text) <= 120, text
        assert "\n" not in text and "§" not in text, text
    return corpus


# ── 30 frozen queries with hand-labelled gold, written against the corpus. ──
QUERIES = [
    # 10 lexical — share a rare word with the gold entry.
    ("q01", "lexical", "harbormaster", ["g02"]),
    ("q02", "lexical", "extension 4417", ["g03"]),
    ("q03", "lexical", "Frankfurt bucket", ["g07"]),
    ("q04", "lexical", "Archivo", ["g12"]),
    ("q05", "lexical", "shellfish", ["g08"]),
    ("q06", "lexical", "Blameless Reviews", ["g15"]),
    ("q07", "lexical", "Chorus", ["g04", "g12"]),
    ("q08", "lexical", "Maya Okonkwo", ["g06"]),
    ("q09", "lexical", "rustfmt", ["g01"]),
    ("q10", "lexical", "Q3 migration", ["g09"]),
    # 10 paraphrase — no content word shared with the gold entry.
    ("q11", "paraphrase", "when am I not allowed to ship to the test environment", ["g02"]),
    ("q12", "paraphrase", "who do I contact if the checkout system breaks", ["g03"]),
    ("q13", "paraphrase", "where does the overnight copy of our data end up", ["g07"]),
    ("q14", "paraphrase", "which typeface should large titles use", ["g12"]),
    ("q15", "paraphrase", "any dietary restrictions when reserving a table", ["g08"]),
    ("q16", "paraphrase", "which date is he always away for family reasons", ["g11"]),
    ("q17", "paraphrase", "what release of the database is live right now", ["g13"]),
    ("q18", "paraphrase", "how often do the reporting credentials change", ["g10"]),
    ("q19", "paraphrase", "who should I send written updates to instead of scheduling a call", ["g06"]),
    ("q20", "paraphrase", "what part of the codebase is frozen until the move is done", ["g09"]),
    # 10 distractor-heavy — a near-duplicate differs in exactly one detail.
    ("q21", "distractor", "what is the staging cluster called", ["g02"]),
    ("q22", "distractor", "which day are staging deploys banned", ["g02"]),
    ("q23", "distractor", "payments incident hotline number", ["g03"]),
    ("q24", "distractor", "which bucket does the nightly backup write to", ["g07"]),
    ("q25", "distractor", "what postgres version is production on", ["g13"]),
    ("q26", "distractor", "when do the analytics vendor keys rotate", ["g10"]),
    ("q27", "distractor", "which team does extension 4417 belong to", ["g03"]),
    ("q28", "distractor", "is the Dublin bucket used for nightly backups", ["g07"]),
    ("q29", "distractor", "what postgres version does staging run", ["g13"]),
    ("q30", "distractor", "which vendor rotates keys on the first Monday", ["g10"]),
]


def main():
    here = pathlib.Path(__file__).parent
    corpus = build()
    (here / "corpus.json").write_text(
        json.dumps([{"id": i, "text": t} for i, t in corpus], indent=1) + "\n",
        encoding="utf-8",
    )
    (here / "queries.json").write_text(
        json.dumps(
            [{"id": q, "kind": k, "text": t, "gold": g} for q, k, t, g in QUERIES], indent=1
        )
        + "\n",
        encoding="utf-8",
    )
    gold_ids = {g for _, _, _, gs in QUERIES for g in gs}
    known = {i for i, _ in corpus[:20]}
    assert gold_ids <= known, gold_ids - known
    for n in (20, 60, 200):
        chars = sum(len(t) for _, t in corpus[:n])
        print(f"N={n:>3}  {chars:>6} chars  (Hermes MEMORY.md cap is 2200)")
    print(f"{len(QUERIES)} queries, {len(gold_ids)} distinct gold entries")


if __name__ == "__main__":
    main()
