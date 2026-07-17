use crate::domain::entities::SearchHit;
use crate::domain::errors::StoreError;
use crate::infra::db::Store;
use rusqlite::params;

impl Store {
    /// FTS5 search across all messages. Explicit FTS syntax (AND/OR/NOT,
    /// phrases, trailing `*`) passes through sanitized; a plain
    /// natural-language query becomes OR-of-prefixes (`natural_fts_query`) —
    /// FTS5's implicit AND made "past conversations today" require every
    /// word in one message, so model-typed recall queries returned nothing.
    pub fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, StoreError> {
        let sanitized = natural_fts_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }
        self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT m.id, m.session_id, m.role,
                        snippet(messages_fts, 0, '>>>', '<<<', '…', 16),
                        m.timestamp
                 FROM messages_fts
                 JOIN messages m ON m.id = messages_fts.rowid
                 WHERE messages_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![sanitized, limit], |row| {
                Ok(SearchHit {
                    message_id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    snippet: row.get(3)?,
                    timestamp: row.get(4)?,
                })
            })?;
            rows.collect()
        })
    }
}

/// Reduces arbitrary user input to a safe FTS5 query: bare alphanumeric
/// tokens (with optional trailing `*`) and interior AND/OR/NOT operators
/// pass through; everything else is phrase-quoted. Dangling operators are
/// dropped.
pub fn sanitize_fts5_query(raw: &str) -> String {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let last_index = tokens.len().saturating_sub(1);
    for (i, token) in tokens.iter().enumerate() {
        let is_operator = matches!(*token, "AND" | "OR" | "NOT");
        if is_operator {
            // Operators are only legal between terms.
            if i == 0 || i == last_index || out.last().is_some_and(|t| is_op(t)) {
                continue;
            }
            out.push((*token).to_owned());
            continue;
        }
        let (body, prefix_star) = match token.strip_suffix('*') {
            Some(rest) => (rest, true),
            None => (*token, false),
        };
        if !body.is_empty() && body.chars().all(char::is_alphanumeric) {
            out.push(if prefix_star {
                format!("{body}*")
            } else {
                body.to_owned()
            });
        } else {
            let cleaned: String = token.chars().filter(|c| *c != '"').collect();
            if !cleaned.is_empty() {
                out.push(format!("\"{cleaned}\""));
            }
        }
    }
    // A trailing operator can appear if later tokens were dropped.
    while out.last().is_some_and(|t| is_op(t)) {
        out.pop();
    }
    out.join(" ")
}

fn is_op(token: &str) -> bool {
    matches!(token, "AND" | "OR" | "NOT")
}

/// Recall-biased query builder shared by message search and graph retrieval
/// (ADR-013's lexical lane). A query using explicit FTS syntax — operators,
/// quotes, or trailing `*` — keeps its exact (sanitized) semantics. A plain
/// natural-language query becomes OR-of-prefixes over its content words:
/// FTS5's implicit AND requires EVERY word to hit, so "what did we do today"
/// matched nothing; OR still ranks multi-term matches first via BM25.
/// Wildcard/garbage-only input ("*", punctuation) reduces to "" — callers
/// treat that as "not searchable", never as a real empty result.
pub fn natural_fts_query(raw: &str) -> String {
    const STOPWORDS: &[&str] = &[
        "a", "an", "the", "is", "are", "was", "were", "be", "do", "does", "did", "how", "what",
        "where", "who", "why", "when", "which", "i", "my", "me", "we", "our", "you", "your", "it",
        "its", "to", "for", "of", "in", "on", "at", "by", "with", "and", "or", "not", "get", "now",
        "use", "about",
    ];
    let explicit = raw.contains('"')
        || raw
            .split_whitespace()
            .any(|t| is_op(t) || (t.len() > 1 && t.ends_with('*')));
    if explicit {
        return sanitize_fts5_query(raw);
    }
    let mut terms: Vec<String> = Vec::new();
    for token in raw.split_whitespace() {
        // Edge punctuation is chatter ("today?"); interior punctuation is
        // structure ("chat-send") and keeps phrase semantics.
        let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
        if trimmed.chars().any(|c| !c.is_alphanumeric()) {
            terms.push(format!("\"{}\"", trimmed.replace('"', "")));
            continue;
        }
        let cleaned = trimmed.to_lowercase();
        if cleaned.len() < 2
            || STOPWORDS.contains(&cleaned.as_str())
            || terms.iter().any(|t| t.trim_end_matches('*') == cleaned)
        {
            continue;
        }
        // Prefix form so "conversation" finds "conversations".
        terms.push(if cleaned.len() >= 3 {
            format!("{cleaned}*")
        } else {
            cleaned
        });
    }
    terms.join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::sanitize_fts5_query;

    #[test]
    fn keeps_plain_terms_and_operators() {
        assert_eq!(
            sanitize_fts5_query("docker deployment"),
            "docker deployment"
        );
        assert_eq!(
            sanitize_fts5_query("docker OR kubernetes"),
            "docker OR kubernetes"
        );
        assert_eq!(sanitize_fts5_query("deploy*"), "deploy*");
    }

    #[test]
    fn quotes_special_tokens_and_drops_dangling_operators() {
        assert_eq!(sanitize_fts5_query("chat-send"), "\"chat-send\"");
        assert_eq!(sanitize_fts5_query("hello AND"), "hello");
        assert_eq!(sanitize_fts5_query("AND hello"), "hello");
        assert_eq!(sanitize_fts5_query("say \"hi\" there"), "say \"hi\" there");
    }

    #[test]
    fn empty_and_garbage_inputs() {
        assert_eq!(sanitize_fts5_query(""), "");
        assert_eq!(sanitize_fts5_query("   "), "");
        assert_eq!(sanitize_fts5_query("\""), "");
    }

    #[test]
    fn natural_queries_become_or_of_prefixes() {
        use super::natural_fts_query;
        // The exact failure shape: stopwords out, content words OR'd + prefixed.
        assert_eq!(
            natural_fts_query("what did we do today"),
            "today*",
            "only the content word survives"
        );
        assert_eq!(
            natural_fts_query("past conversations"),
            "past* OR conversations*"
        );
        // Explicit syntax keeps exact semantics.
        assert_eq!(
            natural_fts_query("docker AND deployment"),
            "docker AND deployment"
        );
        assert_eq!(natural_fts_query("deploy*"), "deploy*");
        assert_eq!(natural_fts_query("say \"hi\" there"), "say \"hi\" there");
        // Hyphenated identifiers keep phrase semantics; edge punctuation drops.
        assert_eq!(
            natural_fts_query("ran chat-send today?"),
            "ran* OR \"chat-send\" OR today*"
        );
        // Wildcard/garbage reduces to "" — callers say "not searchable",
        // never "zero results".
        assert_eq!(natural_fts_query("*"), "");
        assert_eq!(natural_fts_query("?? !!"), "");
        assert_eq!(natural_fts_query(""), "");
    }
}
