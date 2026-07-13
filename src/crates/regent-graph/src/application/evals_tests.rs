//! Unit tests for `evals` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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
