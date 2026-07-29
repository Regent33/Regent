"""Frozen constants for protocol v5. Nothing here is chosen after a result.

Every list is fixed by the protocol; changing one invalidates the freeze and
must be a visible commit before a run, not an edit after one.
"""

import re

BUILD_SEED = 9001
SEEDS = list(range(101, 113))          # §2.4 — 12 insertion orders
N_GOLD, N_PER_STRATUM, N_FILLER, N_TOTAL = 20, 10, 420, 500
BUDGETS = [150, 300, 600, 1200, 2400]  # §5
PRIMARY_BUDGET = 600
RAISED_CHARS = 200_000                 # §3.2
SHIPPED_MEMORY, SHIPPED_USER = 2200, 1375

MAPS = {
    "M1": {"gold": "ACTIVE", "superseded": "RETIRED", "rejected": "DECLINED"},
    "M2": {"gold": "IN EFFECT", "superseded": "WITHDRAWN", "rejected": "NOT ADOPTED"},
    "M3": {"gold": "STANDING", "superseded": "LAPSED", "rejected": "REFUSED"},
}
L_MARKERS = {"gold": "CURRENT", "superseded": "SUPERSEDED", "rejected": "REJECTED PROPOSAL"}
# The frozen "this record is in force" vocabulary, used by the schema-aware
# baselines (§7 #4, #5). Schema knowledge, not label knowledge: no baseline ever
# reads targets.json. "REJECTED PROPOSAL" contains no in-force marker, and
# "SUPERSEDED" is checked for explicitly so it cannot be read as "STANDING".
CURRENT_MARKERS = ("CURRENT", "ACTIVE", "IN EFFECT", "STANDING")
DATES = {"current": "2026-06", "superseded": "2024-03", "rejected": "2025-11"}

# Assigned by a frozen seed, not by gold index, so map cannot be read off position.
MAP_ASSIGNMENT = ["M2", "M1", "M3", "M1", "M2", "M3", "M3", "M2", "M1", "M2",
                  "M1", "M3", "M2", "M3", "M1", "M2", "M3", "M1", "M2", "M1"]

L_TEMPLATES = [
    "According to the current configuration, what is the {entity}'s {relation}?",
    "In the current setup, what is the {entity}'s {relation}?",
    "Under the current arrangement, what is the {entity}'s {relation}?",
    "Per the current record, what is the {entity}'s {relation}?",
]
D_TEMPLATES = [
    "Presently, what is the {entity}'s {relation}?",
    "What is the {entity}'s {relation} today?",
    "As of now, what is the {entity}'s {relation}?",
    "At this moment, what is the {entity}'s {relation}?",
]

GOLD_TUPLES = [
    ("billing service", "database engine",
     {"current": "postgres 16", "superseded": "postgres 14", "rejected": "mysql 8", "other": "postgres 15"}),
    ("payments gateway", "message broker",
     {"current": "kafka 3", "superseded": "rabbitmq 3", "rejected": "pulsar 2", "other": "kafka 2"}),
    ("search cluster", "index format",
     {"current": "lucene 9", "superseded": "lucene 7", "rejected": "tantivy 1", "other": "lucene 8"}),
    ("auth server", "token format",
     {"current": "paseto v4", "superseded": "jwt hs256", "rejected": "macaroons", "other": "paseto v2"}),
    ("media pipeline", "video codec",
     {"current": "av1", "superseded": "h264", "rejected": "vp9", "other": "h265"}),
    ("ledger service", "consensus protocol",
     {"current": "raft", "superseded": "paxos", "rejected": "pbft", "other": "raft lite"}),
    ("notification worker", "queue backend",
     {"current": "redis streams", "superseded": "sqs", "rejected": "nats", "other": "redis lists"}),
    ("reporting stack", "column store",
     {"current": "clickhouse 24", "superseded": "druid 26", "rejected": "pinot 1", "other": "clickhouse 23"}),
    ("identity broker", "signing curve",
     {"current": "ed25519", "superseded": "rsa 2048", "rejected": "secp256k1", "other": "ed448"}),
    ("edge proxy", "tls library",
     {"current": "rustls 0.23", "superseded": "openssl 1", "rejected": "boringssl", "other": "rustls 0.22"}),
    ("catalog service", "cache layer",
     {"current": "memcached 1.6", "superseded": "ehcache 3", "rejected": "hazelcast 5", "other": "memcached 1.5"}),
    ("ingest daemon", "serialization format",
     {"current": "protobuf 4", "superseded": "thrift 0.16", "rejected": "avro 1", "other": "protobuf 3"}),
    ("scheduler node", "clock source",
     {"current": "ptp grandmaster", "superseded": "ntp pool", "rejected": "gps receiver", "other": "ptp slave"}),
    ("archive tier", "compression codec",
     {"current": "zstd 19", "superseded": "gzip 9", "rejected": "brotli 11", "other": "zstd 12"}),
    ("policy engine", "rule language",
     {"current": "rego", "superseded": "drools", "rejected": "cel", "other": "rego lite"}),
    ("telemetry sink", "trace protocol",
     {"current": "otlp grpc", "superseded": "jaeger thrift", "rejected": "zipkin json", "other": "otlp http"}),
    ("build farm", "artifact store",
     {"current": "oci registry", "superseded": "nexus 3", "rejected": "artifactory 7", "other": "oci mirror"}),
    ("session store", "eviction policy",
     {"current": "lru w-tinylfu", "superseded": "plain lru", "rejected": "random sample", "other": "lfu"}),
    ("routing mesh", "load balancer",
     {"current": "maglev hashing", "superseded": "round robin", "rejected": "least conn", "other": "maglev v2"}),
    ("backup agent", "snapshot mode",
     {"current": "incremental cow", "superseded": "full nightly", "rejected": "log shipping", "other": "incremental dedup"}),
]
OTHER_ENTITIES = [
    "reporting service", "settlement gateway", "analytics cluster", "session server",
    "transcode pipeline", "audit service", "digest worker", "metrics stack",
    "federation broker", "internal proxy", "inventory service", "export daemon",
    "cron node", "cold tier", "consent engine", "profiling sink",
    "release farm", "token store", "overlay mesh", "restore agent",
]
FILLER_ENTITIES = [
    "wiki service", "chat relay", "map tiles", "email gateway", "sms bridge",
    "billing exporter", "doc renderer", "image resizer", "font server", "geo lookup",
    "rate limiter", "webhook fanout", "pdf worker", "ocr queue", "spam filter",
    "feed builder", "diff engine", "lint runner", "shard mapper", "quota keeper",
    "trace sampler", "log shipper", "config mirror", "secret rotator", "key vault",
]
FILLER_RELATIONS = [
    "runtime version", "storage backend", "http framework", "test runner",
    "package manager", "linter profile", "deploy target", "metrics agent",
    "log format", "retry policy", "auth scheme", "cache ttl",
    "thread model", "build tool", "container base", "dns resolver",
    "proxy mode", "shard key", "backup window", "alert channel",
]
FILLER_VALUES = [
    "v3.2", "v1.9", "v7.0", "v4.4", "v2.1", "v6.3", "v5.8", "v9.1", "v0.7", "v8.5",
]

_WORD = re.compile(r"[a-z0-9]+")


def toks(text):
    """The frozen lexical tokenization (§7). No stop words, no stemming."""
    return set(_WORD.findall(text.lower()))


def entry(eid, stratum, mapping, kind, entity, relation, value, date):
    """One rendered record. Both strata share the body; only the marker differs."""
    # The other-entity negative is a *current* fact about a different entity,
    # so it carries the gold marker — that is what makes it confusable.
    marker = (L_MARKERS if stratum == "L" else mapping)[
        "gold" if kind == "other_entity" else kind]
    body = f"the {entity}'s {relation} is {value}"
    text = (f"{marker} ({date}): {body}" if stratum == "L"
            else f"Status: {marker} ({date}) - {body}")
    return {"id": eid, "text": text, "stratum": stratum, "kind": kind,
            "entity": entity, "relation": relation, "value": value,
            "marker": marker, "date": date}


def derangement(rng, n):
    """A permutation with no fixed point, so no negative keeps its own gold entity."""
    while True:
        p = list(range(n))
        rng.shuffle(p)
        if all(p[i] != i for i in range(n)):
            return p


def report_marginals(base, other):
    """§2.5 — what the slot permutation preserved, and what it did not."""
    def unigrams(rows):
        c = {}
        for e in rows:
            for t in toks(e["text"]):
                c[t] = c.get(t, 0) + 1
        return c

    def lengths(rows):
        return {e["id"]: len(e["text"]) for e in rows}

    import tiktoken  # local: only the marginals report needs a tokenizer
    enc = tiktoken.get_encoding("cl100k_base")
    tb = {e["id"]: len(enc.encode(e["text"])) for e in base}
    to = {e["id"]: len(enc.encode(e["text"])) for e in other}
    tok_drift = {k: to[k] - tb[k] for k in tb if to[k] != tb[k]}

    ub, uo = unigrams(base), unigrams(other)
    lb, lo = lengths(base), lengths(other)
    drift = {k: (lo[k] - lb[k]) for k in lb if lo[k] != lb[k]}
    return {
        "entity_unigrams_preserved": ub == uo,
        "token_length_drift_entries": len(tok_drift),
        "token_length_drift_max": max((abs(v) for v in tok_drift.values()), default=0),
        "token_length_within_2": max((abs(v) for v in tok_drift.values()), default=0) <= 2,
        "unigram_diff": {k: uo.get(k, 0) - ub.get(k, 0)
                         for k in set(ub) | set(uo) if ub.get(k, 0) != uo.get(k, 0)},
        "char_length_drift_entries": len(drift),
        "char_length_drift_max": max((abs(v) for v in drift.values()), default=0),
        "status_prevalence_base": {m: sum(1 for e in base if e["marker"] == m)
                                   for m in sorted({e["marker"] for e in base})},
        "status_prevalence_other": {m: sum(1 for e in other if e["marker"] == m)
                                    for m in sorted({e["marker"] for e in other})},
    }
