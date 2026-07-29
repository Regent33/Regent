#!/usr/bin/env python3
"""Builds the v3 corpus, queries and seeded insertion orders.

The whole of v3's difficulty lives here. v1's distractors shared 4.30 content
tokens with gold — too easy, a lexical matcher wins. v2 drove that to 0.30,
which is *lexically* dissimilar and therefore also not hard: a system can score
well on word overlap alone without understanding anything.

v3 requires the opposite: distractors that are semantically ADJACENT and
factually WRONG. Same entity, same topic, different value or opposite polarity.
Word overlap is deliberately left UNCONSTRAINED, because constraining it is what
made v2's negatives easy.

Asserts, before writing anything:
  - >= HARD_PER_GOLD hard negatives per gold entry
  - gold spread across the corpus, not clustered
  - no gold id appears in any indexed text

  build_corpus.py <out-dir>
"""

import json
import pathlib
import random
import sys

OUT = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".")
OUT.mkdir(parents=True, exist_ok=True)

N_CORPUS = 500
N_GOLD = 20
HARD_PER_GOLD = 3
SEEDS = (11, 22, 33)

# Each gold fact, then the semantically adjacent wrong ones. The wrong ones
# share the entity and the topic; only the VALUE or the POLARITY differs. That
# is what makes them hard — a system that matches on topic alone cannot tell
# them apart, which is exactly the failure v1 and v2 could not detect.
FACTS = [
    ("deploy target", "production deploys go to the eu-west-2 region", [
        "production deploys go to the us-east-1 region",
        "staging deploys go to the eu-west-2 region",
        "production deploys no longer go to the eu-west-2 region",
    ]),
    ("db engine", "the billing service runs on postgres 16", [
        "the billing service runs on postgres 14",
        "the reporting service runs on postgres 16",
        "the billing service has migrated off postgres entirely",
    ]),
    ("oncall", "alerts page the platform team between 09:00 and 21:00", [
        "alerts page the platform team between 21:00 and 09:00",
        "alerts page the security team between 09:00 and 21:00",
        "alerts stopped paging the platform team entirely",
    ]),
    ("editor", "the user prefers tabs over spaces in go files", [
        "the user prefers spaces over tabs in go files",
        "the user prefers tabs over spaces in python files",
        "the user has no preference between tabs and spaces",
    ]),
    ("release cadence", "releases ship every second thursday", [
        "releases ship every second tuesday",
        "hotfixes ship every second thursday",
        "releases no longer follow a fixed cadence",
    ]),
    ("auth", "the api authenticates with mutual tls", [
        "the api authenticates with bearer tokens",
        "the admin console authenticates with mutual tls",
        "the api dropped mutual tls last quarter",
    ]),
    ("cache ttl", "the session cache expires after 45 minutes", [
        "the session cache expires after 15 minutes",
        "the asset cache expires after 45 minutes",
        "the session cache no longer expires on a timer",
    ]),
    ("language", "the ingest pipeline is written in rust", [
        "the ingest pipeline is written in go",
        "the export pipeline is written in rust",
        "the ingest pipeline was rewritten away from rust",
    ]),
    ("meeting", "the weekly sync happens on wednesday mornings", [
        "the weekly sync happens on wednesday afternoons",
        "the monthly review happens on wednesday mornings",
        "the weekly sync was cancelled permanently",
    ]),
    ("budget owner", "cloud spend is approved by the finance lead", [
        "cloud spend is approved by the engineering lead",
        "tooling spend is approved by the finance lead",
        "cloud spend no longer requires approval",
    ]),
    ("test runner", "integration tests run under pytest with xdist", [
        "integration tests run under pytest without xdist",
        "unit tests run under pytest with xdist",
        "integration tests were moved off pytest",
    ]),
    ("queue", "background jobs use redis streams", [
        "background jobs use rabbitmq",
        "scheduled jobs use redis streams",
        "background jobs stopped using redis streams",
    ]),
    ("doc format", "runbooks are written in asciidoc", [
        "runbooks are written in markdown",
        "design docs are written in asciidoc",
        "runbooks are no longer written down",
    ]),
    ("retention", "audit logs are retained for 400 days", [
        "audit logs are retained for 90 days",
        "access logs are retained for 400 days",
        "audit logs are retained indefinitely",
    ]),
    ("mobile", "the android build targets api level 34", [
        "the android build targets api level 31",
        "the ios build targets api level 34",
        "the android build dropped a fixed api target",
    ]),
    ("vendor", "observability is bought from a third party, not built", [
        "observability is built in house, not bought",
        "incident response is bought from a third party",
        "observability was moved back in house",
    ]),
    ("naming", "feature branches are prefixed feat/", [
        "feature branches are prefixed feature/",
        "release branches are prefixed feat/",
        "feature branches use no prefix",
    ]),
    ("review rule", "two approvals are required to merge to main", [
        "one approval is required to merge to main",
        "two approvals are required to merge to develop",
        "merges to main require no approvals",
    ]),
    ("timezone", "the team reports timestamps in utc", [
        "the team reports timestamps in local time",
        "the billing exports report timestamps in utc",
        "the team stopped normalising timestamps",
    ]),
    ("licence", "the project ships under the mit licence", [
        "the project ships under the apache 2.0 licence",
        "the sdk ships under the mit licence",
        "the project relicensed away from mit",
    ]),
]

assert len(FACTS) == N_GOLD, f"expected {N_GOLD} gold facts, got {len(FACTS)}"

rng = random.Random(20260729)

corpus = []
queries = []

# Gold + its hard negatives first, so ids are stable; positions are assigned by
# the shuffle below, not by construction order.
for i, (topic, gold_text, hard) in enumerate(FACTS):
    gid = f"g{i:02d}"
    corpus.append({"id": gid, "text": gold_text, "role": "gold", "topic": topic})
    assert len(hard) >= HARD_PER_GOLD, f"{gid}: only {len(hard)} hard negatives"
    for j, wrong in enumerate(hard):
        corpus.append(
            {"id": f"{gid}h{j}", "text": wrong, "role": "hard_negative", "topic": topic}
        )
    queries.append({"id": f"q{i:02d}", "text": f"what is the {topic}?", "gold": [gid]})

# Filler to reach N_CORPUS. Unrelated facts — these are the EASY negatives, and
# they are here to make the store realistic, not to be the difficulty.
FILLER_SUBJECTS = [
    "the lobby printer", "the office kettle", "the parking gate", "the guest wifi",
    "the fire drill", "the coffee order", "the plant rota", "the door badge",
    "the mail room", "the standing desk", "the window blinds", "the bike rack",
]
FILLER_PREDICATES = [
    "was serviced in march", "is on the second floor", "needs a new fuse",
    "is shared with the neighbours", "has a spare key", "was replaced last year",
    "runs on a timer", "is checked weekly", "belongs to facilities",
    "is being decommissioned", "arrived in a blue box", "has no manual",
]
n = 0
while len(corpus) < N_CORPUS:
    text = f"{rng.choice(FILLER_SUBJECTS)} {rng.choice(FILLER_PREDICATES)} (note {n})"
    corpus.append({"id": f"f{n:03d}", "text": text, "role": "filler", "topic": "misc"})
    n += 1

assert len(corpus) == N_CORPUS, len(corpus)

# --- discharge conditions from protocol §7, asserted before anything is written

gold_ids = {e["id"] for e in corpus if e["role"] == "gold"}
for e in corpus:
    for gid in gold_ids:
        assert gid not in e["text"], f"gold id {gid} leaked into indexed text: {e}"

hard_count = {}
for e in corpus:
    if e["role"] == "hard_negative":
        hard_count[e["topic"]] = hard_count.get(e["topic"], 0) + 1
for topic in {e["topic"] for e in corpus if e["role"] == "gold"}:
    assert hard_count.get(topic, 0) >= HARD_PER_GOLD, f"{topic}: {hard_count.get(topic, 0)}"

# Seeded insertion orders — emitted ONCE and read by both harnesses, so the two
# systems see a byte-identical sequence.
for seed in SEEDS:
    order = [e["id"] for e in corpus]
    random.Random(seed).shuffle(order)
    pos = {cid: i for i, cid in enumerate(order)}
    spread = sorted(pos[g] for g in gold_ids)
    # Gold must not cluster: v1 put all of it in the first 20, so saturation
    # only ever refused distractors and the cap protected recall.
    assert spread[0] < N_CORPUS * 0.2, f"seed {seed}: earliest gold at {spread[0]}"
    assert spread[-1] > N_CORPUS * 0.6, f"seed {seed}: latest gold at {spread[-1]}"
    (OUT / f"order-seed{seed}.json").write_text(json.dumps(order), encoding="utf-8")
    print(f"seed {seed}: gold positions {spread[0]}..{spread[-1]} of {N_CORPUS}")

(OUT / "corpus.json").write_text(json.dumps(corpus, indent=1), encoding="utf-8")
(OUT / "queries.json").write_text(json.dumps(queries, indent=1), encoding="utf-8")

chars = sum(len(e["text"]) for e in corpus)
print(f"\ncorpus {len(corpus)} entries, {chars} chars")
print(f"gold {len(gold_ids)}, hard negatives {sum(hard_count.values())}, "
      f"filler {sum(1 for e in corpus if e['role'] == 'filler')}")
print(f"queries {len(queries)}")
print(f"\nshipped-arm cap 2200 holds ~{2200 * len(corpus) // chars} entries")
print(f"raised arm needs >= {chars} chars to hold everything")
