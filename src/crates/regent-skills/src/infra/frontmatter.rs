//! Minimal SKILL.md frontmatter codec — the flat `key: value` subset the
//! skill standard actually uses (plus inline `[a, b]` lists). A YAML crate
//! is deliberately avoided (supply-chain footprint for 5 keys).

use crate::domain::entities::SkillMeta;
use crate::domain::errors::SkillError;

/// Parses `---` delimited frontmatter + body. The `name` field is forced to
/// the directory name by the caller (directory is the identity).
pub fn parse(raw: &str) -> Result<(SkillMeta, String), SkillError> {
    let rest = raw.strip_prefix("---").ok_or(SkillError::Invalid {
        field: "frontmatter",
        reason: "SKILL.md must start with ---".into(),
    })?;
    let (header, body) = rest.split_once("\n---").ok_or(SkillError::Invalid {
        field: "frontmatter",
        reason: "unterminated frontmatter block".into(),
    })?;

    let mut meta = SkillMeta::new("", "", "user");
    for line in header.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "name" => meta.name = value.to_owned(),
            "description" => meta.description = value.to_owned(),
            "version" => meta.version = value.to_owned(),
            "created_by" => meta.created_by = value.to_owned(),
            "pinned" => meta.pinned = value == "true",
            "tags" => meta.tags = parse_inline_list(value),
            _ => {} // unknown keys tolerated (agentskills.io superset)
        }
    }
    Ok((meta, body.trim_start_matches(['\r', '\n']).to_owned()))
}

pub fn render(meta: &SkillMeta, body: &str) -> String {
    let tags = if meta.tags.is_empty() {
        String::new()
    } else {
        format!("\ntags: [{}]", meta.tags.join(", "))
    };
    format!(
        "---\nname: {}\ndescription: {}\nversion: {}\ncreated_by: {}\npinned: {}{}\n---\n\n{}",
        meta.name, meta.description, meta.version, meta.created_by, meta.pinned, tags, body
    )
}

fn parse_inline_list(value: &str) -> Vec<String> {
    value
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_meta_and_body() {
        let meta = SkillMeta {
            name: "code-review".into(),
            description: "Structured code review workflow.".into(),
            version: "1.2.0".into(),
            created_by: "agent".into(),
            pinned: true,
            tags: vec!["review".into(), "quality".into()],
        };
        let rendered = render(&meta, "# Body\n\nSteps here.");
        let (parsed, body) = parse(&rendered).unwrap();
        assert_eq!(parsed, meta);
        assert_eq!(body, "# Body\n\nSteps here.");
    }

    #[test]
    fn tolerates_unknown_keys_and_rejects_missing_block() {
        let raw = "---\nname: x\ndescription: D.\nauthor: Someone\nlicense: MIT\n---\nbody";
        let (meta, body) = parse(raw).unwrap();
        assert_eq!(meta.name, "x");
        assert_eq!(body, "body");
        assert!(parse("no frontmatter here").is_err());
        assert!(parse("---\nname: x\nnever terminated").is_err());
    }
}
