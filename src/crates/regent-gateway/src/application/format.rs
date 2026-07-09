//! Markdown → readable plain text for chat platforms. The CLI renders markdown
//! richly (Ink), but Telegram/Discord/… show raw `**bold**`, pipe tables, and
//! `[text](url)` literally, so the gateway flattens replies before sending.
//! Conservative: drops syntax that reads as noise, keeps all the actual text
//! (and code-fence contents verbatim).

/// Flatten common markdown to plain text suitable for any chat platform.
#[must_use]
pub fn flatten_markdown(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in input.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence; // drop the fence marker line itself
            continue;
        }
        if in_fence {
            out.push(line.to_owned()); // code body stays verbatim
            continue;
        }
        if is_table_separator(trimmed) {
            continue; // |---|:--:|--- rows carry no content
        }
        let mut s = strip_heading(line);
        s = flatten_table_row(&s);
        s = normalize_bullet(&s);
        s = strip_inline(&s);
        out.push(s);
    }
    out.join("\n")
}

/// `## Title` / `### Title` → `Title` (only a real ATX heading: #s then space).
fn strip_heading(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        format!("{indent}{}", rest[hashes..].trim_start())
    } else {
        line.to_owned()
    }
}

/// `|---|:--:|` separator row (only pipes, dashes, colons, spaces).
fn is_table_separator(trimmed: &str) -> bool {
    trimmed.starts_with('|')
        && trimmed.contains('-')
        && trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// `| a | b | c |` → `a  b  c` (drop the pipes, join cells with two spaces).
fn flatten_table_row(line: &str) -> String {
    let t = line.trim();
    if !(t.starts_with('|') && t.ends_with('|') && t.matches('|').count() >= 2) {
        return line.to_owned();
    }
    let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
    cells.join("  ")
}

/// Leading `-`/`*`/`+` list marker → `• ` (keeps indentation).
fn normalize_bullet(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    for m in ["- ", "* ", "+ "] {
        if let Some(item) = rest.strip_prefix(m) {
            return format!("{indent}• {item}");
        }
    }
    line.to_owned()
}

/// Inline spans: `[text](url)` → `text (url)`; strip `**`/`` ` ``; remove `*`
/// emphasis markers (but keep a literal `*` that stands alone between spaces).
fn strip_inline(line: &str) -> String {
    let no_links = flatten_links(line);
    let mut out = String::with_capacity(no_links.len());
    let bytes: Vec<char> = no_links.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '`' {
            i += 1; // drop inline-code backticks
            continue;
        }
        if c == '*' {
            // Keep a standalone `*` surrounded by spaces (e.g. "2 * 3"); drop
            // emphasis markers (** or *word*).
            let prev_space = i == 0 || bytes[i - 1].is_whitespace();
            let next_space = i + 1 >= bytes.len() || bytes[i + 1].is_whitespace();
            if prev_space && next_space {
                out.push(c);
            }
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Replace `[text](url)` with `text (url)`, leaving everything else untouched.
fn flatten_links(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some((text, url, end)) = parse_link(&chars, i)
        {
            if url.is_empty() {
                out.push_str(&text);
            } else {
                out.push_str(&format!("{text} ({url})"));
            }
            i = end;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Parse `[text](url)` starting at `[`; returns (text, url, index after `)`).
fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let close_br = (start + 1..chars.len()).find(|&j| chars[j] == ']')?;
    if close_br + 1 >= chars.len() || chars[close_br + 1] != '(' {
        return None;
    }
    let close_paren = (close_br + 2..chars.len()).find(|&j| chars[j] == ')')?;
    let text: String = chars[start + 1..close_br].iter().collect();
    let url: String = chars[close_br + 2..close_paren].iter().collect();
    Some((text, url, close_paren + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bold_code_and_headings() {
        assert_eq!(flatten_markdown("## Key specs"), "Key specs");
        assert_eq!(
            flatten_markdown("a **bold** and `code` x"),
            "a bold and code x"
        );
    }

    #[test]
    fn links_become_text_with_url() {
        assert_eq!(
            flatten_markdown("see [the docs](https://x.com/a)"),
            "see the docs (https://x.com/a)"
        );
    }

    #[test]
    fn bullets_and_tables_and_fences() {
        assert_eq!(flatten_markdown("- item"), "• item");
        assert_eq!(
            flatten_markdown("| a | b |\n|---|---|\n| 1 | 2 |"),
            "a  b\n1  2"
        );
        assert_eq!(flatten_markdown("```rust\nlet x = 1;\n```"), "let x = 1;");
    }

    #[test]
    fn keeps_literal_text_and_standalone_asterisk() {
        assert_eq!(flatten_markdown("2 * 3 = 6"), "2 * 3 = 6");
        assert_eq!(flatten_markdown("plain line"), "plain line");
    }
}
