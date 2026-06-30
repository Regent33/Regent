//! Pure parser for the V4A patch envelope (Claude Code / Hermes `patch_parser`):
//!
//! ```text
//! *** Begin Patch
//! *** Add File: path/new.txt
//! +line one
//! +line two
//! *** Update File: path/edit.txt
//! @@ optional context hint
//!  unchanged context
//! -removed line
//! +added line
//! *** Delete File: path/old.txt
//! *** End Patch
//! ```
//!
//! No I/O — `mod.rs` applies the parsed ops through the path jail.

#[derive(Debug, PartialEq, Eq)]
pub enum Op {
    Add { path: String, content: String },
    Delete { path: String },
    Update { path: String, hunks: Vec<Hunk> },
}

/// One hunk: an ordered run of context / removed / added lines.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Hunk {
    pub lines: Vec<HLine>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HLine {
    Ctx(String),
    Add(String),
    Del(String),
}

/// Parse a V4A patch into ordered operations. Errors describe the offending line.
pub fn parse(patch: &str) -> Result<Vec<Op>, String> {
    let mut lines = patch.lines();
    // Skip blanks to the Begin marker.
    let begin = lines
        .by_ref()
        .find(|l| !l.trim().is_empty())
        .ok_or("empty patch")?;
    if !begin.trim_start().starts_with("*** Begin Patch") {
        return Err("patch must start with '*** Begin Patch'".into());
    }

    let mut ops: Vec<Op> = Vec::new();
    // The in-progress op + (for Update) the current hunk.
    let mut add: Option<(String, Vec<String>)> = None;
    let mut update: Option<(String, Vec<Hunk>)> = None;
    let mut cur_hunk: Option<Hunk> = None;

    // Flush whatever op is open into `ops`.
    macro_rules! flush {
        () => {{
            if let Some((path, body)) = add.take() {
                ops.push(Op::Add {
                    path,
                    content: body.join("\n"),
                });
            }
            if let Some((path, mut hunks)) = update.take() {
                if let Some(h) = cur_hunk.take()
                    && !h.lines.is_empty()
                {
                    hunks.push(h);
                }
                ops.push(Op::Update { path, hunks });
            }
            // `cur_hunk` is already None here: an Update flush `.take()`s it, and
            // outside an Update it was never set.
        }};
    }

    for line in lines {
        if let Some(rest) = marker(line, "*** Add File:") {
            flush!();
            add = Some((rest, Vec::new()));
        } else if let Some(rest) = marker(line, "*** Delete File:") {
            flush!();
            ops.push(Op::Delete { path: rest });
        } else if let Some(rest) = marker(line, "*** Update File:") {
            flush!();
            update = Some((rest, Vec::new()));
        } else if line.trim_start().starts_with("*** End Patch") {
            flush!();
            return Ok(ops);
        } else if line.starts_with("@@") {
            // Hunk boundary inside an Update.
            if let Some((_, hunks)) = update.as_mut()
                && let Some(h) = cur_hunk.take()
                && !h.lines.is_empty()
            {
                hunks.push(h);
            }
            cur_hunk = Some(Hunk::default());
        } else if let Some((_, body)) = add.as_mut() {
            // Add-file body: every line is `+`-prefixed content.
            body.push(line.strip_prefix('+').unwrap_or(line).to_owned());
        } else if update.is_some() {
            let h = cur_hunk.get_or_insert_with(Hunk::default);
            let hline = match line.chars().next() {
                Some('+') => HLine::Add(line[1..].to_owned()),
                Some('-') => HLine::Del(line[1..].to_owned()),
                Some(' ') => HLine::Ctx(line[1..].to_owned()),
                None => HLine::Ctx(String::new()),
                _ => HLine::Ctx(line.to_owned()),
            };
            h.lines.push(hline);
        }
        // Lines before any file marker (other than Begin) are ignored.
    }
    Err("patch missing '*** End Patch'".into())
}

fn marker(line: &str, prefix: &str) -> Option<String> {
    line.trim_start()
        .strip_prefix(prefix)
        .map(|rest| rest.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_add_update_delete() {
        let patch = "*** Begin Patch\n\
                     *** Add File: a.txt\n\
                     +hello\n\
                     +world\n\
                     *** Update File: b.txt\n\
                     @@\n\
                      keep\n\
                     -old\n\
                     +new\n\
                     *** Delete File: c.txt\n\
                     *** End Patch";
        let ops = parse(patch).unwrap();
        assert_eq!(ops.len(), 3);
        assert_eq!(
            ops[0],
            Op::Add {
                path: "a.txt".into(),
                content: "hello\nworld".into()
            }
        );
        match &ops[1] {
            Op::Update { path, hunks } => {
                assert_eq!(path, "b.txt");
                assert_eq!(hunks.len(), 1);
                assert_eq!(
                    hunks[0].lines,
                    vec![
                        HLine::Ctx("keep".into()),
                        HLine::Del("old".into()),
                        HLine::Add("new".into())
                    ]
                );
            }
            other => panic!("expected Update, got {other:?}"),
        }
        assert_eq!(
            ops[2],
            Op::Delete {
                path: "c.txt".into()
            }
        );
    }

    #[test]
    fn rejects_missing_begin_and_end() {
        assert!(parse("nope").is_err());
        assert!(
            parse("*** Begin Patch\n*** Add File: a\n+x").is_err(),
            "no End"
        );
    }
}
