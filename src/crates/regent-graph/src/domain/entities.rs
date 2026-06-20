use crate::domain::errors::GraphError;
use regent_store::NodeRow;

/// Where external content came from — stored with every node and mapped to
/// a trust prior. Untrusted provenance is re-rendered as quoted data on
/// every retrieval, never laundered into instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    UserStated,
    AgentInferred,
    ToolOutput,
    WebContent,
}

impl Provenance {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserStated => "user_stated",
            Self::AgentInferred => "agent_inferred",
            Self::ToolOutput => "tool_output",
            Self::WebContent => "web_content",
        }
    }

    #[must_use]
    pub fn trust(self) -> f64 {
        match self {
            Self::UserStated => 1.0,
            Self::AgentInferred => 0.7,
            Self::ToolOutput => 0.4,
            Self::WebContent => 0.3,
        }
    }

    /// Parses the stored string form; unknown values fall back to the most
    /// conservative trusted tier (`AgentInferred`) rather than over-trusting.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "user_stated" => Self::UserStated,
            "tool_output" => Self::ToolOutput,
            "web_content" => Self::WebContent,
            _ => Self::AgentInferred,
        }
    }
}

/// The two bounded prompt stores (Hermes MEMORY.md / USER.md semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTarget {
    Memory,
    User,
}

impl MemoryTarget {
    #[must_use]
    pub fn kind(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::User => "user",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, GraphError> {
        match raw {
            "memory" => Ok(Self::Memory),
            "user" => Ok(Self::User),
            other => Err(GraphError::Rejected(format!(
                "unknown memory target '{other}' (expected 'memory' or 'user')"
            ))),
        }
    }
}

/// Outcome of a bounded-store add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddOutcome {
    Added,
    /// Identical entry already stored — success, nothing written.
    Duplicate,
}

/// One retrieval result with its fused score.
#[derive(Debug, Clone)]
pub struct Recalled {
    pub node: NodeRow,
    pub score: f64,
    /// Relation that pulled this node in via expansion (None for seeds).
    pub via: Option<String>,
}
