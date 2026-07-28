#!/usr/bin/env python3
"""v2 corpus + queries. Deterministic; no clock, no unseeded randomness.

Two constraints v1 lacked, both enforced here rather than hoped for:

* gold entries are SPREAD across the corpus, so a seeded shuffle plus the
  ~2,200-char cap drops gold and distractors alike (v1 clustered gold in the
  first 20, so the cap only ever refused distractors and thereby protected
  recall);
* distractor-class queries share **at most 2** content tokens with their gold
  entry. v1's averaged 4.3, which made the "hard" class the easy one. The
  builder asserts this and fails loudly rather than emitting a soft corpus.
"""

import json
import pathlib
import re

STOP = {
    "the", "a", "an", "is", "are", "was", "were", "to", "of", "in", "on", "at",
    "for", "and", "or", "but", "not", "no", "it", "its", "he", "she", "they",
    "his", "her", "their", "we", "i", "you", "do", "does", "did", "so", "that",
    "this", "these", "those", "with", "from", "by", "as", "be", "been", "has",
    "have", "had", "what", "which", "when", "where", "who", "how", "why", "if",
    "can", "will", "would", "should", "my", "me", "am", "any", "all", "each",
}


def toks(text):
    return {w for w in re.findall(r"[a-z0-9]+", text.lower()) if w not in STOP}


# 15 gold entries — same facts as v1 so the two runs stay comparable.
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

NEAR = [
    ("d01", "The staging cluster is called lighthouse; deploys to it are banned on Mondays."),
    ("d02", "Platform team incident hotline is extension 4471."),
    ("d03", "The nightly backup job writes to the Dublin bucket every weekend."),
    ("d04", "Postgres runs at version 15 in development; CI containers use 16."),
    ("d05", "API keys for the payments vendor rotate on the last Friday of each month."),
    ("d06", "The archive cluster is called harbourside; it accepts writes any weekday."),
    ("d07", "Design drafts use Inter for body text and Archivo for small captions."),
    ("d08", "Ralph's son's birthday is 14 September; that day is a normal working day."),
]

FILLER_TEMPLATES = [
    "Sprint {i} retrospective notes are filed in the team drive under planning.",
    "Onboarding checklist item {i} is to request VPN access from IT support.",
    "Meeting room {i} on the fourth floor has the working video conference unit.",
    "Vendor invoice batch {i} was approved by finance without any exceptions.",
    "The archive bucket keeps release {i} artifacts for exactly ninety days.",
    "Runbook section {i} covers restarting the ingestion workers safely.",
    "Ticket queue {i} is triaged every morning by the on-call engineer.",
    "Dashboard panel {i} tracks p95 latency for the public API surface.",
    "Config flag {i} was retired after the platform migration completed.",
    "Weekly digest {i} summarises open pull requests for the whole team.",
]

# Queries. Distractor class deliberately avoids the gold entry's own wording.
QUERIES = [
    ("q01", "lexical", "harbormaster", ["g02"]),
    ("q02", "lexical", "extension 4417", ["g03"]),
    ("q03", "lexical", "Frankfurt bucket", ["g07"]),
    ("q04", "lexical", "Archivo", ["g12"]),
    ("q05", "lexical", "shellfish", ["g08"]),
    ("q06", "lexical", "Blameless Reviews", ["g15"]),
    ("q07", "lexical", "rustfmt", ["g01"]),
    ("q08", "lexical", "Maya Okonkwo", ["g06"]),
    ("q09", "lexical", "decaf", ["g14"]),
    ("q10", "lexical", "Q3 migration", ["g09"]),
    ("q11", "paraphrase", "when am I not allowed to ship to the test environment", ["g02"]),
    ("q12", "paraphrase", "who do I contact if the checkout system breaks", ["g03"]),
    ("q13", "paraphrase", "where does the overnight copy of our data end up", ["g07"]),
    ("q14", "paraphrase", "which typeface should large titles use", ["g12"]),
    ("q15", "paraphrase", "dietary restrictions when reserving a table", ["g08"]),
    ("q16", "paraphrase", "which date is he always away for family reasons", ["g11"]),
    ("q17", "paraphrase", "what release of the database is live right now", ["g13"]),
    ("q18", "paraphrase", "how often do the reporting credentials change", ["g10"]),
    ("q19", "paraphrase", "who wants written notes instead of a scheduled call", ["g06"]),
    ("q20", "paraphrase", "what code is frozen until the move finishes", ["g09"]),
    # Distractor class: a near-duplicate differs in one detail, and the query
    # deliberately does NOT reuse the gold entry's phrasing.
    ("q21", "distractor", "name of the environment we test releases on", ["g02"]),
    ("q22", "distractor", "which weekday is shipping to test blocked", ["g02"]),
    ("q23", "distractor", "four digit number to reach billing support", ["g03"]),
    ("q24", "distractor", "which european city stores our overnight copies", ["g07"]),
    ("q25", "distractor", "database release number serving live traffic", ["g13"]),
    ("q26", "distractor", "schedule for changing reporting credentials", ["g10"]),
    ("q27", "distractor", "team owning the four digit support line", ["g03"]),
    ("q28", "distractor", "is our irish region used for overnight copies", ["g07"]),
    ("q29", "distractor", "database release number used before going live", ["g13"]),
    ("q30", "distractor", "vendor whose credentials change at month start", ["g10"]),
]


def build():
    filler = [
        (f"f{i:03d}", FILLER_TEMPLATES[i % len(FILLER_TEMPLATES)].format(i=i + 1))
        for i in range(177)
    ]
    # Interleave so gold is spread through the corpus rather than clustered.
    # Deterministic: gold every 13th slot, near-duplicates every 17th.
    corpus, gi, di, fi = [], 0, 0, 0
    for slot in range(200):
        if slot % 13 == 0 and gi < len(GOLD):
            corpus.append(GOLD[gi]); gi += 1
        elif slot % 17 == 0 and di < len(NEAR):
            corpus.append(NEAR[di]); di += 1
        elif fi < len(filler):
            corpus.append(filler[fi]); fi += 1
        elif gi < len(GOLD):
            corpus.append(GOLD[gi]); gi += 1
        elif di < len(NEAR):
            corpus.append(NEAR[di]); di += 1
    assert gi == len(GOLD) and di == len(NEAR), (gi, di)
    assert len(corpus) == 200, len(corpus)
    for _, text in corpus:
        assert len(text) <= 120 and "\n" not in text and "§" not in text, text
    return corpus


def main():
    here = pathlib.Path(__file__).parent
    corpus = build()
    text_of = dict(corpus)

    # The constraint v1 lacked. Fail loudly, do not emit a soft corpus.
    violations = []
    for qid, kind, text, gold in QUERIES:
        if kind != "distractor":
            continue
        for g in gold:
            shared = toks(text) & toks(text_of[g])
            if len(shared) > 2:
                violations.append((qid, g, sorted(shared)))
    assert not violations, f"distractor queries too lexically easy: {violations}"

    (here / "corpus.json").write_text(
        json.dumps([{"id": i, "text": t} for i, t in corpus], indent=1) + "\n", encoding="utf-8"
    )
    (here / "queries.json").write_text(
        json.dumps(
            [{"id": q, "kind": k, "text": t, "gold": g} for q, k, t, g in QUERIES], indent=1
        )
        + "\n",
        encoding="utf-8",
    )

    for kind in ("lexical", "paraphrase", "distractor"):
        overlaps = [
            len(toks(t) & toks(text_of[g]))
            for _, k, t, gs in QUERIES if k == kind for g in gs
        ]
        print(f"{kind:>11}: mean query<->gold content-token overlap {sum(overlaps)/len(overlaps):.2f}")
    # Seeded insertion orders, emitted ONCE and read by both harnesses, so
    # the two systems get the identical sequence rather than each shuffling
    # independently.
    import random

    for seed in (11, 22, 33):
        ids = [cid for cid, _ in corpus]
        random.Random(seed).shuffle(ids)
        (here / f"order-seed{seed}.json").write_text(
            json.dumps(ids, indent=1) + chr(10), encoding="utf-8"
        )
    print("wrote insertion orders for seeds 11, 22, 33")

    positions = [i for i, (cid, _) in enumerate(corpus) if cid.startswith("g")]
    print(f"gold at corpus positions {positions[0]}..{positions[-1]} (spread, not clustered)")
    for n in (10, 20, 30, 200):
        print(f"N={n:>3}  {sum(len(t) for _, t in corpus[:n]):>6} chars")


if __name__ == "__main__":
    main()
