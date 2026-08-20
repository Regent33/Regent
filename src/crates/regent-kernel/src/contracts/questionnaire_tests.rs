//! Unit tests for the questionnaire contract (a sibling file pulled into the
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn option(id: &str) -> QuestionOption {
    QuestionOption {
        id: id.to_owned(),
        label: format!("Option {id}"),
        description: None,
    }
}

fn question(kind: QuestionKind, options: usize) -> Question {
    Question {
        id: "q1".to_owned(),
        prompt: "Pick one".to_owned(),
        header: Some("Scope".to_owned()),
        kind,
        options: ('a'..)
            .take(options)
            .map(|c| option(&c.to_string()))
            .collect(),
        allow_custom: true,
        required: false,
    }
}

fn sheet(questions: Vec<Question>) -> Questionnaire {
    Questionnaire {
        id: "q_1".to_owned(),
        questions,
    }
}

#[test]
fn accepts_one_of_each_kind() {
    for kind in [
        QuestionKind::SingleSelect,
        QuestionKind::MultiSelect,
        QuestionKind::Rank,
    ] {
        assert!(
            validate(&sheet(vec![question(kind, 2)])).is_ok(),
            "{kind:?}"
        );
    }
    for kind in [QuestionKind::Text, QuestionKind::Confirm] {
        assert!(
            validate(&sheet(vec![question(kind, 0)])).is_ok(),
            "{kind:?}"
        );
    }
}

#[test]
fn rejects_every_violation() {
    let empty_id = Questionnaire {
        id: "  ".to_owned(),
        questions: vec![question(QuestionKind::Text, 0)],
    };
    let cases: Vec<(&str, Questionnaire)> = vec![
        ("blank questionnaire id", empty_id),
        ("no questions", sheet(vec![])),
        (
            "too many questions",
            sheet(
                (0..=MAX_QUESTIONS)
                    .map(|n| Question {
                        id: format!("q{n}"),
                        ..question(QuestionKind::Text, 0)
                    })
                    .collect(),
            ),
        ),
        (
            "one option",
            sheet(vec![question(QuestionKind::SingleSelect, 1)]),
        ),
        (
            "too many options",
            sheet(vec![question(QuestionKind::SingleSelect, MAX_OPTIONS + 1)]),
        ),
        (
            "options on a text question",
            sheet(vec![question(QuestionKind::Text, 2)]),
        ),
        (
            "duplicate question ids",
            sheet(vec![
                question(QuestionKind::Text, 0),
                question(QuestionKind::Text, 0),
            ]),
        ),
        (
            "duplicate option ids",
            sheet(vec![Question {
                options: vec![option("a"), option("a")],
                ..question(QuestionKind::SingleSelect, 0)
            }]),
        ),
        (
            "blank prompt",
            sheet(vec![Question {
                prompt: " ".to_owned(),
                ..question(QuestionKind::Text, 0)
            }]),
        ),
    ];
    for (name, bad) in cases {
        assert!(validate(&bad).is_err(), "should reject: {name}");
    }
}

#[test]
fn answers_round_trip_through_json() {
    let answer = QuestionnaireAnswer {
        questionnaire_id: "q_1".to_owned(),
        answers: vec![
            (
                "a".to_owned(),
                Answer::Selected {
                    option_ids: vec!["x".to_owned(), "y".to_owned()],
                },
            ),
            (
                "b".to_owned(),
                Answer::Text {
                    text: "hi".to_owned(),
                },
            ),
            ("c".to_owned(), Answer::Confirmed { yes: true }),
            ("d".to_owned(), Answer::Skipped),
        ],
        cancelled: false,
    };
    let json = serde_json::to_string(&answer).unwrap();
    assert!(json.contains(r#""kind":"selected""#), "{json}");
    assert!(json.contains(r#""kind":"skipped""#), "{json}");
    assert_eq!(
        serde_json::from_str::<QuestionnaireAnswer>(&json).unwrap(),
        answer
    );
    assert!(matches!(
        answer.get("c"),
        Some(Answer::Confirmed { yes: true })
    ));
    assert!(answer.get("nope").is_none());
}

#[test]
fn questions_round_trip_and_default_allow_custom_to_true() {
    let sheet = sheet(vec![question(QuestionKind::MultiSelect, 3)]);
    let json = serde_json::to_string(&sheet).unwrap();
    assert_eq!(serde_json::from_str::<Questionnaire>(&json).unwrap(), sheet);

    // The minimal shape a model might emit: no header, no allow_custom.
    let lean: Questionnaire = serde_json::from_str(
        r#"{"id":"q","questions":[{"id":"a","prompt":"?","kind":"confirm"}]}"#,
    )
    .unwrap();
    assert!(
        lean.questions[0].allow_custom,
        "allow_custom defaults to true"
    );
    assert!(!lean.questions[0].required);
    assert!(lean.questions[0].options.is_empty());
}

#[test]
fn text_rendering_numbers_the_options() {
    let text = render_text(&sheet(vec![
        question(QuestionKind::SingleSelect, 2),
        Question {
            id: "q2".to_owned(),
            ..question(QuestionKind::Confirm, 0)
        },
    ]));
    assert!(text.contains("(1 of 2)"), "{text}");
    assert!(text.contains("  1. Option a"), "{text}");
    assert!(text.contains("reply yes or no"), "{text}");
}

#[test]
fn text_replies_map_onto_typed_answers() {
    let single = question(QuestionKind::SingleSelect, 3);
    let multi = question(QuestionKind::MultiSelect, 3);
    let confirm = question(QuestionKind::Confirm, 0);

    assert_eq!(
        parse_text_answer(&single, "2"),
        Answer::Selected {
            option_ids: vec!["b".to_owned()]
        }
    );
    assert_eq!(
        parse_text_answer(&multi, "1, 3"),
        Answer::Selected {
            option_ids: vec!["a".to_owned(), "c".to_owned()]
        }
    );
    // Repeated picks collapse; order is preserved (it is the ranking).
    assert_eq!(
        parse_text_answer(&multi, "3 1 3"),
        Answer::Selected {
            option_ids: vec!["c".to_owned(), "a".to_owned()]
        }
    );
    assert_eq!(
        parse_text_answer(&confirm, "Yep"),
        Answer::Confirmed { yes: true }
    );
    assert_eq!(
        parse_text_answer(&confirm, "nope"),
        Answer::Confirmed { yes: false }
    );
    assert_eq!(parse_text_answer(&single, "   "), Answer::Skipped);

    // Ambiguity stays free text rather than silently meaning an option.
    for reply in ["3 or 4, whichever", "9", "0", "the second one"] {
        assert!(
            matches!(parse_text_answer(&single, reply), Answer::Text { .. }),
            "{reply:?} should stay free text"
        );
    }
    // Two numbers for a single-select is not a selection.
    assert!(matches!(
        parse_text_answer(&single, "1,2"),
        Answer::Text { .. }
    ));
}

#[test]
fn answers_describe_themselves_with_labels() {
    let q = question(QuestionKind::MultiSelect, 3);
    assert_eq!(
        describe_answer(
            &q,
            &Answer::Selected {
                option_ids: vec!["a".to_owned(), "c".to_owned()]
            }
        ),
        "Option a, Option c"
    );
    // An id with no matching option degrades to the id, never a panic.
    assert_eq!(
        describe_answer(
            &q,
            &Answer::Selected {
                option_ids: vec!["zz".to_owned()]
            }
        ),
        "zz"
    );
    assert_eq!(describe_answer(&q, &Answer::Skipped), "(skipped)");
    assert_eq!(describe_answer(&q, &Answer::Confirmed { yes: false }), "no");
}
