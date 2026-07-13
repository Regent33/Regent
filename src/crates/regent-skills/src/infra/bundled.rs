//! Skills compiled into the binary (`include_str!`) — always available, no
//! install step. They ride the normal SKILL.md pipeline (same frontmatter
//! codec) and the library merges them UNDER the disk repository: a user skill
//! directory with the same name overrides the bundled copy entirely.

use crate::domain::entities::SkillRecord;
use crate::infra::frontmatter;

const BUNDLED_RAW: &[(&str, &str)] = &[
    ("ponytail", include_str!("../../skills/ponytail/SKILL.md")),
    (
        "code-reviewer",
        include_str!("../../skills/code-reviewer/SKILL.md"),
    ),
    (
        "secure-code-guardian",
        include_str!("../../skills/secure-code-guardian/SKILL.md"),
    ),
    ("documents", include_str!("../../skills/documents/SKILL.md")),
    ("research", include_str!("../../skills/research/SKILL.md")),
];

/// The bundled skills, parsed. A malformed asset is skipped with an error log
/// rather than failing the caller — the test below keeps that path dead.
#[must_use]
pub fn bundled() -> Vec<SkillRecord> {
    BUNDLED_RAW
        .iter()
        .filter_map(|(name, raw)| match frontmatter::parse(raw) {
            Ok((mut meta, body)) => {
                meta.name = (*name).to_owned();
                Some(SkillRecord {
                    meta,
                    body,
                    files: Vec::new(),
                })
            }
            Err(error) => {
                tracing::error!(skill = name, %error, "bundled skill failed to parse");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_skills_parse_with_bundled_provenance() {
        let records = bundled();
        let names: Vec<&str> = records.iter().map(|r| r.meta.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "ponytail",
                "code-reviewer",
                "secure-code-guardian",
                "documents",
                "research"
            ]
        );
        for record in &records {
            assert_eq!(record.meta.created_by, "bundled", "{}", record.meta.name);
            assert!(record.meta.pinned, "{} must be pinned", record.meta.name);
            assert!(!record.body.is_empty());
            let desc = &record.meta.description;
            assert!(
                desc.chars().count() <= 60 && desc.ends_with('.'),
                "{}: description breaks the hardline standard",
                record.meta.name
            );
        }
    }
}
