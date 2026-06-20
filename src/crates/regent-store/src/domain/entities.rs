//! Persistence entities (rows as the domain sees them). Pure data — all
//! SQL lives in `infra/`.

use regent_kernel::ChatMessage;

/// A message row with storage metadata (superset of `ChatMessage`).
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub id: i64,
    pub message: ChatMessage,
    pub timestamp: f64,
    pub finish_reason: Option<String>,
}

/// One full-text search hit across past conversations.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub message_id: i64,
    pub session_id: String,
    pub role: String,
    pub snippet: String,
    pub timestamp: f64,
}

/// Session header row (lineage, lifecycle, accounting).
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub source: String,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub parent_session_id: Option<String>,
    pub started_at: f64,
    pub ended_at: Option<f64>,
    pub end_reason: Option<String>,
    pub message_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub api_call_count: i64,
}

/// One recorded turn (reproducibility ledger).
#[derive(Debug, Clone)]
pub struct TurnRecord {
    pub model: Option<String>,
    pub api_calls: u32,
    pub outcome: String,
    pub error: Option<String>,
    pub started_at: f64,
    pub ended_at: f64,
}

/// Aggregate usage rollup across every session — the `insights` surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InsightsRollup {
    pub sessions: i64,
    pub turns: i64,
    pub turns_ok: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub api_calls: i64,
    pub messages: i64,
}

/// Graph memory node row.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub content: String,
    pub provenance: String,
    pub trust: f64,
    pub session_id: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
    pub ttl_expires_at: Option<f64>,
    pub access_count: i64,
    pub content_hash: String,
}

/// A neighbor reached over one edge (either direction).
#[derive(Debug, Clone)]
pub struct NeighborRow {
    pub relation: String,
    pub weight: f64,
    pub node: NodeRow,
}

/// A kanban task on the multi-agent work board.
#[derive(Debug, Clone, PartialEq)]
pub struct KanbanTaskRow {
    pub id: String,
    pub board: String,
    pub title: String,
    pub description: String,
    /// `todo` | `in_progress` | `in_review` | `done` | `blocked`.
    pub status: String,
    /// Worker profile that claimed it (None while unclaimed).
    pub assignee: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
}

/// How finished work on a board reaches `done`. Boards with no config row
/// default to [`ReviewPolicy::Human`], so a review is never silently skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewPolicy {
    /// A person approves `in_review` tasks via the kanban tool.
    #[default]
    Human,
    /// A reviewer agent judges the work and approves/rejects it.
    Agent,
    /// Self-approve — submitted work goes straight to `done` (no gate).
    Auto,
}

impl ReviewPolicy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Auto => "auto",
        }
    }

    /// Parses a stored policy string, defaulting to [`Self::Human`] for any
    /// unknown value — the fail-safe (never skip review on a typo).
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "agent" => Self::Agent,
            "auto" => Self::Auto,
            _ => Self::Human,
        }
    }
}

/// A board's configuration row (its review policy, and the reviewer profile
/// used when the policy is `agent`).
#[derive(Debug, Clone, PartialEq)]
pub struct BoardRow {
    pub board: String,
    pub review_policy: ReviewPolicy,
    pub reviewer_agent: Option<String>,
    pub created_at: f64,
}

/// A long-term memory write awaiting human approval — holds everything
/// `add_node` needs to commit it once approved.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingWriteRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub content: String,
    pub provenance: String,
    pub trust: f64,
    pub session_id: Option<String>,
    pub ttl_secs: Option<f64>,
    pub created_at: f64,
}
