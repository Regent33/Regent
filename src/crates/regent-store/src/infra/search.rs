use crate::domain::entities::SearchHit;
use crate::domain::errors::StoreError;
use crate::infra::db::Store;
use rusqlite::params;

impl Store {
    /// FTS5 search across all messages. User input is sanitized into a safe
    /// FTS5 query (ports the `_sanitize_fts5_query` behavior).
    pub fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, StoreError> {
        let sanitized = sanitize_fts5_query(query);
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

#[cfg(test)]
mod tests {
    use super::sanitize_fts5_query;

    #[test]
    fn keeps_plain_terms_and_operators() {
        assert_eq!(sanitize_fts5_query("docker deployment"), "docker deployment");
        assert_eq!(sanitize_fts5_query("docker OR kubernetes"), "docker OR kubernetes");
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
}
