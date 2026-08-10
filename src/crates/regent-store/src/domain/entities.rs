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

/// One conversational turn beside a search hit (W11).
///
/// An FTS snippet is ~16 tokens around the match, which is often the entire
/// message and still unreadable: *"yes, do that"* answers a question the hit
/// does not contain. `offset` is the signed distance in message order — `-1` is
/// the turn before the hit, `+1` the turn after — so a reader can reconstruct
/// the exchange without a second round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowMessage {
    pub role: String,
    pub content: String,
    pub offset: i64,
}

/// One data source's slice of a session-mix report (ADR-038 P0(c)): turn and
/// token aggregates over the report's lookback window, feeding the
/// light/full billed-token comparison in `profile.report`.
#[derive(Debug, Clone)]
pub struct SourceMix {
    pub source: String,
    pub session_count: i64,
    pub total_turns: i64,
    pub avg_turns_per_session: f64,
    pub total_input_tokens: i64,
    pub avg_input_tokens_per_call: f64,
}

/// Session-mix report over the last `days` days (ADR-038 P0(c)): the measured
/// inputs to the profile A-vs-B analytic comparison, derived entirely from
/// existing telemetry — read-only, no new instrumentation.
#[derive(Debug, Clone)]
pub struct SessionMixReport {
    pub days: f64,
    pub total_sessions: i64,
    /// Sessions with at least one assistant tool call to `code_task`,
    /// `delegate_task`, or `load_tools` — the escalation-trigger tools.
    pub escalating_sessions: i64,
    /// `escalating_sessions / total_sessions` (0.0 when there are no
    /// sessions in the window, never NaN).
    pub escalation_share: f64,
    pub by_source: Vec<SourceMix>,
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
    /// Most recent persisted message, or `started_at` before the first turn.
    pub last_activity_at: f64,
    pub ended_at: Option<f64>,
    pub end_reason: Option<String>,
    /// Human-set session title (None until renamed).
    pub title: Option<String>,
    /// Organization flags — surfaced additively on `session.list`.
    pub pinned: bool,
    pub archived: bool,
    pub message_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub api_call_count: i64,
}

/// Surfaces that are the agent talking to itself, not a conversation anyone had.
///
/// A denylist rather than an allowlist on purpose: a new user-facing surface
/// should appear by default, not vanish until someone remembers to list it.
///
/// Lives here, beside [`SessionMeta`], because THREE call sites need the same
/// answer — the `session_list` tool, the `session.list` RPC, and the desktop
/// rail — and this repo has already been bitten once by a hand-copied list
/// drifting between two composition roots.
pub const INTERNAL_SESSION_SOURCES: &[&str] = &["review", "background", "delegate"];

impl SessionMeta {
    /// Whether this session belongs in a human-facing history listing.
    ///
    /// Measured on a real store 2026-07-30: of the 1,000 newest rows, **833
    /// were internal**, so a listing that filters client-side ships six times
    /// the payload it renders — and a model asked "what did we do this week?"
    /// had to summarise a list that was mostly the learning loop.
    #[must_use]
    pub fn is_user_facing(&self) -> bool {
        // A session row is created the moment one is built and plenty never get
        // a turn (a health probe, an abandoned "New session", a cancelled folder
        // pick). There is nothing to resume in a conversation with no messages.
        self.message_count > 0 && !INTERNAL_SESSION_SOURCES.contains(&self.source.as_str())
    }
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
    /// Calls whose provider response omitted input/output token usage.
    pub unreported_usage_calls: i64,
    /// True for an upgraded database whose pre-v11 coverage cannot be proven.
    pub legacy_usage_unverified: bool,
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

/// A graph edge row (endpoints, relation, strength) — surfaced by the
/// full-graph dump for the visualization page.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeRow {
    pub src: String,
    pub dst: String,
    pub relation: String,
    pub weight: f64,
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

/// A persistent, reusable named agent definition. A kanban task assigned to
/// `name` is worked by this agent (its prompt/model/tools).
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    /// Model override; None = inherit the session/deacon model.
    pub model: Option<String>,
    /// CSV tool allow-list; None = the full catalog.
    pub tools: Option<String>,
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

/// One process's durable ownership of a parent-session transcript range.
/// The opaque token fences completion if an expired lease is reclaimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewLease {
    pub token: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewClaimOutcome {
    Acquired(ReviewLease),
    /// Another process currently owns an unexpired range on this session.
    Busy,
    /// The durable cursor already covers the requested target.
    Covered { reviewed_message_count: usize },
}
