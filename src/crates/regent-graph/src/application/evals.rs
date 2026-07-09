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
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn recall_counts_expected_in_top_k() {
        let ranked = s(&["a", "b", "c", "d"]);
        assert_eq!(recall_at_k(&ranked, &s(&["a", "c"]), 5), 1.0);
        assert_eq!(recall_at_k(&ranked, &s(&["a", "z"]), 5), 0.5);
        assert_eq!(
            recall_at_k(&ranked, &s(&["d"]), 3),
            0.0,
            "d is outside top-3"
        );
        assert_eq!(
            recall_at_k(&ranked, &[], 5),
            1.0,
            "no expectation is vacuously met"
        );
    }

    #[test]
    fn mrr_uses_first_expected_rank() {
        let ranked = s(&["a", "b", "c"]);
        assert!((mrr(&ranked, &s(&["a"])) - 1.0).abs() < 1e-9);
        assert!((mrr(&ranked, &s(&["c"])) - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(mrr(&ranked, &s(&["z"])), 0.0);
    }

    #[test]
    fn runner_validates_dataset() {
        assert!(run_golden(&[], 5, |_| vec![]).is_err());
        let bad = vec![GoldenCase {
            query: " ".into(),
            expected: s(&["a"]),
            class: EvalClass::Exact,
        }];
        assert!(run_golden(&bad, 5, |_| vec![]).is_err());
        let no_expected = vec![GoldenCase {
            query: "q".into(),
            expected: vec![],
            class: EvalClass::Exact,
        }];
        assert!(run_golden(&no_expected, 5, |_| vec![]).is_err());
    }

    #[test]
    fn runner_aggregates_per_class() {
        let cases = vec![
            GoldenCase {
                query: "x".into(),
                expected: s(&["a"]),
                class: EvalClass::Exact,
            },
            GoldenCase {
                query: "y".into(),
                expected: s(&["b"]),
                class: EvalClass::Paraphrase,
            },
        ];
        let report = run_golden(&cases, 5, |q| match q {
            "x" => s(&["a"]),    // hit at rank 1
            _ => s(&["z", "b"]), // hit at rank 2
        })
        .unwrap();
        assert_eq!(report.n, 2);
        assert!((report.recall - 1.0).abs() < 1e-9);
        assert!((report.mrr - 0.75).abs() < 1e-9); // (1.0 + 0.5) / 2
        assert_eq!(report.per_class.len(), 2);
        assert!(report.passes(0.75, 0.60));
    }
}
