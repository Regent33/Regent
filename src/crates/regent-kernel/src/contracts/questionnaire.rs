//! The structured-question contract: one typed object an agent can put in
//! front of a human, and one typed object that comes back. Lives in the kernel
//! because regent-tools authors it, regent-deacon and regent-gateway ferry it,
//! and every surface renders it — the kernel is the only crate all of them
//! already depend on.
//!
//! Deliberately batch-shaped: a questionnaire carries ALL its questions, and a
//! surface answers them all in one reply. That keeps the existing single-slot
//! pending-approval channel correct as-is — "1 of 3" is a client-side stepper
//! over one request, not three round-trips.

use serde::{Deserialize, Serialize};

/// Caps are tight on purpose. A card with nine options is a menu: it does not
/// fit a Telegram inline keyboard or a terminal window, and it means the model
/// should have asked a different question.
pub const MAX_QUESTIONS: usize = 5;
pub const MAX_OPTIONS: usize = 6;
pub const MIN_OPTIONS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    SingleSelect,
    MultiSelect,
    Text,
    Confirm,
    /// Selection order is the ranking, so `Answer::Selected` covers it too.
    Rank,
}

impl QuestionKind {
    /// True when the kind is answered by picking from `options`.
    #[must_use]
    pub fn needs_options(self) -> bool {
        matches!(self, Self::SingleSelect | Self::MultiSelect | Self::Rank)
    }

    /// True when more than one option may be chosen.
    #[must_use]
    pub fn is_multi(self) -> bool {
        matches!(self, Self::MultiSelect | Self::Rank)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
    /// The line under the label that carries the actual decision content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub prompt: String,
    /// Very short chip label beside the question ("Auth method", "Scope") —
    /// what makes the card read as a form field rather than a paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub kind: QuestionKind,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    /// Offer a "Something else" free-text row beside the options. Defaults to
    /// TRUE and the tool tells the model never to author its own "Other", so a
    /// model never burns one of its few option slots re-inventing the escape
    /// hatch.
    #[serde(default = "yes")]
    pub allow_custom: bool,
    /// A skippable question resolves to `Skipped`; `required` marks the ones a
    /// surface should not let the user step past.
    #[serde(default)]
    pub required: bool,
}

const fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Questionnaire {
    pub id: String,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    /// Option ids in the order the user chose them — which is also the ranking
    /// for `Rank`, so one variant covers select and rank.
    Selected {
        option_ids: Vec<String>,
    },
    Text {
        text: String,
    },
    Confirmed {
        yes: bool,
    },
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionnaireAnswer {
    pub questionnaire_id: String,
    /// One entry per answered question, keyed by `Question.id`.
    pub answers: Vec<(String, Answer)>,
    /// True when the user dismissed the whole card rather than answering.
    #[serde(default)]
    pub cancelled: bool,
}

impl QuestionnaireAnswer {
    /// The answer for `question_id`, if the user gave one.
    #[must_use]
    pub fn get(&self, question_id: &str) -> Option<&Answer> {
        self.answers
            .iter()
            .find(|(id, _)| id == question_id)
            .map(|(_, answer)| answer)
    }
}

/// Rejects a questionnaire the surfaces could not render, before it leaves the
/// tool — the model gets the violation back and corrects itself rather than a
/// human getting a broken card.
///
/// # Errors
/// Returns the first violation found, phrased for the model.
pub fn validate(q: &Questionnaire) -> Result<(), String> {
    if q.id.trim().is_empty() {
        return Err("questionnaire needs a non-empty id".to_owned());
    }
    if q.questions.is_empty() || q.questions.len() > MAX_QUESTIONS {
        return Err(format!(
            "a questionnaire needs 1..={MAX_QUESTIONS} questions, got {}",
            q.questions.len()
        ));
    }
    let mut seen = Vec::with_capacity(q.questions.len());
    for question in &q.questions {
        validate_question(question)?;
        if seen.contains(&question.id.as_str()) {
            return Err(format!("duplicate question id {:?}", question.id));
        }
        seen.push(question.id.as_str());
    }
    Ok(())
}

fn validate_question(question: &Question) -> Result<(), String> {
    if question.id.trim().is_empty() {
        return Err("every question needs a non-empty id".to_owned());
    }
    if question.prompt.trim().is_empty() {
        return Err(format!("question {:?} needs a prompt", question.id));
    }
    let count = question.options.len();
    if question.kind.needs_options() {
        if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&count) {
            return Err(format!(
                "question {:?} is a {:?} and needs {MIN_OPTIONS}..={MAX_OPTIONS} options, got {count}",
                question.id, question.kind
            ));
        }
    } else if count > 0 {
        return Err(format!(
            "question {:?} is a {:?} and must not carry options",
            question.id, question.kind
        ));
    }
    let mut seen = Vec::with_capacity(count);
    for option in &question.options {
        if option.id.trim().is_empty() || option.label.trim().is_empty() {
            return Err(format!(
                "every option of question {:?} needs an id and a label",
                question.id
            ));
        }
        if seen.contains(&option.id.as_str()) {
            return Err(format!(
                "duplicate option id {:?} in question {:?}",
                option.id, question.id
            ));
        }
        seen.push(option.id.as_str());
    }
    Ok(())
}

/// Renders a questionnaire as numbered plain text — the universal fallback for
/// a surface that cannot draw a card (an old client, WeChat, a piped stdin).
/// Whatever renders this must also be able to parse [`parse_text_answer`].
#[must_use]
pub fn render_text(q: &Questionnaire) -> String {
    let mut out = String::new();
    let many = q.questions.len() > 1;
    for (index, question) in q.questions.iter().enumerate() {
        if many {
            out.push_str(&format!("({} of {}) ", index + 1, q.questions.len()));
        }
        out.push_str(&question.prompt);
        out.push('\n');
        match question.kind {
            QuestionKind::Confirm => out.push_str("  reply yes or no\n"),
            QuestionKind::Text => out.push_str("  reply with your answer\n"),
            kind => {
                for (n, option) in question.options.iter().enumerate() {
                    out.push_str(&format!("  {}. {}", n + 1, option.label));
                    if let Some(description) = &option.description {
                        out.push_str(&format!(" — {description}"));
                    }
                    out.push('\n');
                }
                out.push_str(if kind.is_multi() {
                    "  reply with numbers (e.g. 1,3) or your own answer\n"
                } else {
                    "  reply with a number or your own answer\n"
                });
            }
        }
    }
    out
}

/// Maps one whole text reply onto the questionnaire: one line per question, in
/// the order they were rendered. Fewer lines than questions leaves the rest
/// `Skipped` — which is honest, rather than guessing that one sentence answered
/// three questions. This is the text surfaces' inverse of [`render_text`].
#[must_use]
pub fn parse_text_reply(q: &Questionnaire, reply: &str) -> Vec<(String, Answer)> {
    let mut lines = reply.lines().map(str::trim).filter(|l| !l.is_empty());
    q.questions
        .iter()
        .map(|question| {
            let answer = lines
                .next()
                .map_or(Answer::Skipped, |line| parse_text_answer(question, line));
            (question.id.clone(), answer)
        })
        .collect()
}

/// Maps one free-text reply onto a typed answer for `question`: `2`, `1,3`,
/// `yes`/`no`, or anything else as custom text. The inverse of the numbered
/// rendering above, and the single parser every text surface shares.
#[must_use]
pub fn parse_text_answer(question: &Question, reply: &str) -> Answer {
    let text = reply.trim();
    if text.is_empty() {
        return Answer::Skipped;
    }
    if question.kind == QuestionKind::Confirm {
        return match affirmative(text) {
            Some(yes) => Answer::Confirmed { yes },
            None => Answer::Text {
                text: text.to_owned(),
            },
        };
    }
    if question.kind.needs_options()
        && let Some(ids) = parse_indices(question, text)
    {
        return Answer::Selected { option_ids: ids };
    }
    Answer::Text {
        text: text.to_owned(),
    }
}

/// `1` / `1,3` / `2 4` → option ids, in the order given (which is the ranking).
/// `None` when any token is not a valid 1-based option number, so a reply like
/// "3 or 4, whichever" stays free text instead of silently meaning "3".
fn parse_indices(question: &Question, text: &str) -> Option<Vec<String>> {
    let tokens: Vec<&str> = text
        .split([',', ' ', ';'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() || (tokens.len() > 1 && !question.kind.is_multi()) {
        return None;
    }
    let mut ids = Vec::with_capacity(tokens.len());
    for token in tokens {
        let index: usize = token.parse().ok()?;
        let option = question.options.get(index.checked_sub(1)?)?;
        if !ids.contains(&option.id) {
            ids.push(option.id.clone());
        }
    }
    Some(ids)
}

/// yes/no in the words people actually type. `None` = neither.
fn affirmative(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" | "yeah" | "yep" | "ok" | "okay" | "sure" | "1" | "true" => Some(true),
        "n" | "no" | "nope" | "nah" | "2" | "false" => Some(false),
        _ => None,
    }
}

/// Human-readable summary of one answer — what the tool hands back to the
/// model and what a transcript shows after the card is answered.
#[must_use]
pub fn describe_answer(question: &Question, answer: &Answer) -> String {
    match answer {
        Answer::Selected { option_ids } => option_ids
            .iter()
            .map(|id| label_of(question, id))
            .collect::<Vec<_>>()
            .join(", "),
        Answer::Text { text } => text.clone(),
        Answer::Confirmed { yes } => if *yes { "yes" } else { "no" }.to_owned(),
        Answer::Skipped => "(skipped)".to_owned(),
    }
}

fn label_of(question: &Question, option_id: &str) -> String {
    question
        .options
        .iter()
        .find(|option| option.id == option_id)
        .map_or_else(|| option_id.to_owned(), |option| option.label.clone())
}

#[cfg(test)]
#[path = "questionnaire_tests.rs"]
mod tests;
