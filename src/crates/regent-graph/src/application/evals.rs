//! Retrieval evaluation harness — the regression gate for memory recall.
//!
//! Pure metrics (recall@k, MRR) + a golden-set runner that **validates the
//! dataset before scoring** (no empty query/expected — fail loud, never skip),
//! breaks results down **per eval class** so a regression localizes, and gates
//! on explicit thresholds. Callers log the report for reproducibility.
//!
//! MLOps infrastructure (MLflow / Kubeflow / feature stores) is intentionally
//! out of scope: a local agent's retrieval eval is a `cargo test` regression
//! gate over a versioned in-repo dataset, not a training pipeline.

use std::collections::BTreeMap;

/// The recall shape a case exercises — tracked so regressions localize to a
/// retrieval lane (lexical vs semantic vs graph) instead of a single number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalClass {
    Exact,
    Prefix,
    GraphHop,
    Synonym,
    Paraphrase,
    MultiEntity,
}

/// Fraction of `expected` items found within the top `k` of `ranked`.
#[must_use]
pub fn recall_at_k(ranked: &[String], expected: &[String], k: usize) -> f64 {
    if expected.is_empty() {
        return 1.0;
    }
    let top = &ranked[..ranked.len().min(k)];
    let hits = expected.iter().filter(|e| top.contains(e)).count();
    hits as f64 / expected.len() as f64
}

/// Reciprocal rank of the first expected item (0 when none is retrieved).
#[must_use]
pub fn mrr(ranked: &[String], expected: &[String]) -> f64 {
    ranked
        .iter()
        .position(|r| expected.contains(r))
        .map_or(0.0, |rank| 1.0 / (rank as f64 + 1.0))
}

/// One labelled (query → expected identifiers) pair. `expected` holds whatever
/// stable identifier the `retrieve` closure returns (node id or name).
pub struct GoldenCase {
    pub query: String,
    pub expected: Vec<String>,
    pub class: EvalClass,
}

/// Aggregate + per-class metrics over a golden set.
#[derive(Debug)]
pub struct EvalReport {
    pub n: usize,
    /// recall@k for the `k` the run used.
    pub recall: f64,
    pub mrr: f64,
    /// (class, count, recall, mrr) per class present in the dataset.
    pub per_class: Vec<(EvalClass, usize, f64, f64)>,
}

impl EvalReport {
    /// Pass/fail against aggregate thresholds.
    #[must_use]
    pub fn passes(&self, min_recall: f64, min_mrr: f64) -> bool {
        self.recall >= min_recall && self.mrr >= min_mrr
    }
}

/// Runs `retrieve` (returns ranked identifiers) over each validated case and
/// aggregates the metrics. Errors — never panics — on a malformed dataset.
pub fn run_golden(
    cases: &[GoldenCase],
    k: usize,
    retrieve: impl Fn(&str) -> Vec<String>,
) -> Result<EvalReport, String> {
    if cases.is_empty() {
        return Err("eval dataset is empty".into());
    }
    let mut recall_sum = 0.0;
    let mut mrr_sum = 0.0;
    let mut by_class: BTreeMap<u8, (EvalClass, usize, f64, f64)> = BTreeMap::new();

    for case in cases {
        if case.query.trim().is_empty() {
            return Err("eval case has an empty query".into());
        }
        if case.expected.is_empty() {
            return Err(format!(
                "eval case '{}' has no expected identifiers",
                case.query
            ));
        }
        let ranked = retrieve(&case.query);
        let recall = recall_at_k(&ranked, &case.expected, k);
        let reciprocal = mrr(&ranked, &case.expected);
        recall_sum += recall;
        mrr_sum += reciprocal;

        let entry = by_class
            .entry(case.class as u8)
            .or_insert((case.class, 0, 0.0, 0.0));
        entry.1 += 1;
        entry.2 += recall;
        entry.3 += reciprocal;
    }

    let n = cases.len();
    let per_class = by_class
        .values()
        .map(|(class, count, recall, reciprocal)| {
            (
                *class,
                *count,
                recall / *count as f64,
                reciprocal / *count as f64,
            )
        })
        .collect();

    Ok(EvalReport {
        n,
        recall: recall_sum / n as f64,
        mrr: mrr_sum / n as f64,
        per_class,
    })
}

#[cfg(test)]
#[path = "evals_tests.rs"]
mod tests;
