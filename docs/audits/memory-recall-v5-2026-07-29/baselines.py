"""Frozen baselines for protocol v5 §7.

Every baseline ranks the SAME population — the entries a system actually stored
— and is rendered through ONE shared renderer, so `target_delivery@600` compares
ranking and not renderer verbosity. Parameters are frozen in the protocol;
"BM25" and "word TF-IDF" do not name algorithms on their own.

Nothing here reads `targets.json`. Baselines 4 and 5 read the status marker out
of the rendered text, using the frozen schema vocabulary — that is what makes
them *schema-aware* rather than *label-aware*.
"""

import numpy as np
from rank_bm25 import BM25Okapi
from sklearn.feature_extraction.text import TfidfVectorizer

import spec

SHARED_RENDERER = {"prefix": "", "separator": "\n", "suffix": ""}
RRF_K = 60
STATUS_BOOST = 1.5


def _tok(text):
    return spec._WORD.findall(text.lower())


def _order(ids, scores):
    """Rank descending by score, ties broken by ascending corpus id (frozen)."""
    return [i for i, _ in sorted(zip(ids, scores), key=lambda p: (-p[1], p[0]))]


def _is_current(text):
    """Schema-aware, not label-aware: the frozen 'this is in force' vocabulary.

    Parses the marker field rather than substring-matching the whole record, so
    a body that happened to contain a marker word could not fake currency.
    """
    head = text.split("(", 1)[0].upper()
    if head.startswith("STATUS:"):
        head = head[len("STATUS:"):]
    return head.strip() in spec.CURRENT_MARKERS


def build(ids, texts, queries, embed_fn, rng):
    """Return {baseline_name: {query_id: [ranked ids]}} over one stored population."""
    corpus_tok = [_tok(t) for t in texts]
    bm25 = BM25Okapi(corpus_tok, k1=1.5, b=0.75)

    word = TfidfVectorizer(sublinear_tf=True, norm="l2", ngram_range=(1, 1),
                           lowercase=True, token_pattern=r"[a-z0-9]+")
    word_m = word.fit_transform(texts)
    char = TfidfVectorizer(analyzer="char_wb", ngram_range=(3, 5), sublinear_tf=True,
                           norm="l2", lowercase=True)
    char_m = char.fit_transform(texts)

    vecs = embed_fn(texts)                       # (n, d), L2-normalised
    current = np.array([_is_current(t) for t in texts])

    out = {name: {} for name in (
        "random", "insertion", "bm25", "bm25_status_filtered", "bm25_status_boost",
        "tfidf_word", "tfidf_char", "minilm_cosine", "rrf_bm25_minilm")}

    shuffled = list(ids)
    rng.shuffle(shuffled)

    for q in queries:
        qid, qtext = q["id"], q["text"]
        qtok = _tok(qtext)
        bm = np.asarray(bm25.get_scores(qtok), dtype=float)
        qv = embed_fn([qtext])[0]
        cos = vecs @ qv

        out["random"][qid] = shuffled
        out["insertion"][qid] = list(ids)
        out["bm25"][qid] = _order(ids, bm)

        # 4 — filter to in-force records, then BM25. The obvious method given
        # the corpus is generated from an explicit schema.
        filtered = bm.copy()
        filtered[~current] = -np.inf
        out["bm25_status_filtered"][qid] = _order(ids, filtered)

        # 5 — soft version of 4: a frozen additive bonus, on min-max normalised
        # BM25 so the constant means the same thing at every query.
        span = bm.max() - bm.min()
        norm = (bm - bm.min()) / span if span > 0 else np.zeros_like(bm)
        out["bm25_status_boost"][qid] = _order(ids, norm + STATUS_BOOST * current)

        out["tfidf_word"][qid] = _order(ids, (word_m @ word.transform([qtext]).T).toarray().ravel())
        out["tfidf_char"][qid] = _order(ids, (char_m @ char.transform([qtext]).T).toarray().ravel())
        out["minilm_cosine"][qid] = _order(ids, cos)

        # 10 — frozen RRF over ranks, equal weights, no score normalisation.
        rank_bm = {i: r for r, i in enumerate(_order(ids, bm))}
        rank_cos = {i: r for r, i in enumerate(_order(ids, cos))}
        rrf = [1.0 / (RRF_K + rank_bm[i]) + 1.0 / (RRF_K + rank_cos[i]) for i in ids]
        out["rrf_bm25_minilm"][qid] = _order(ids, rrf)

    return out


def oracles(ids, texts, queries, targets, cost_fn, budget):
    """§7 baselines 11 and 12 — the two ceilings, computed exactly.

    `cost_fn(list_of_ids) -> tokens` is the scorer's own prefix tokenizer, so the
    oracles are bound by the same admission rule as the systems.
    """
    text_of = dict(zip(ids, texts))
    stored = set(ids)

    # 11 — the best SINGLE fixed context for the whole 20-query workload. Each
    # query has exactly one target and targets are distinct, so value is uniform
    # and the optimum is the cheapest-first packing of the targets.
    wanted = [targets[q["id"]] for q in queries if targets[q["id"]] in stored]
    by_cost = sorted(set(wanted), key=lambda i: (cost_fn([i]), i))
    fixed, admitted = [], []
    for i in by_cost:
        if cost_fn(fixed + [i]) > budget:
            break
        fixed.append(i)
        admitted.append(i)
    static = set(admitted)

    # 12 — per query, the target first. Delivered iff the target was stored and
    # its own complete rendered text fits the budget.
    conditioned = {}
    for q in queries:
        t = targets[q["id"]]
        conditioned[q["id"]] = [t] if t in stored and cost_fn([t]) <= budget else []

    return {
        "oracle_static": {q["id"]: list(fixed) for q in queries},
        "oracle_conditioned": conditioned,
        "_static_covers": sum(1 for q in queries if targets[q["id"]] in static),
        "_text_of": text_of,
    }
