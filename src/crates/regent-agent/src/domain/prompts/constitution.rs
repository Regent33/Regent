//! The opt-in constitutional values layer: the versioned document, its
//! section parser, the token-efficient always-on core, and the graph-memory
//! chunking (ADR-028). Pure transformations over one `include_str!` document.

/// The opt-in constitutional values layer — character, doctrine, and hard
/// boundaries — shipped verbatim as a versioned prompt file (§10.6 prompt
/// lifecycle: edit the .md, review the diff, ship). `[Agent Name]` is the
/// placeholder [`constitution_text`] fills.
pub const CONSTITUTIONAL_PROMPT: &str = include_str!("../../../prompts/constitution.md");

/// The constitution with `[Agent Name]` resolved to `name`.
#[must_use]
pub fn constitution_text(name: &str) -> String {
    CONSTITUTIONAL_PROMPT.replace("[Agent Name]", name)
}

/// One `## N. Title` section of the constitution document.
pub struct ConstitutionSection {
    pub number: u8,
    pub title: String,
    pub body: String,
}

/// Sections the always-on core keeps verbatim: the preamble, character (the
/// every-turn behavior), and the safety-relevant limits — hard boundaries,
/// crisis, minors, tools/memory. Limits must never depend on retrieval recall
/// (ADR-028); everything else is served precisely from memory when relevant.
const CORE_SECTIONS: [u8; 5] = [3, 11, 12, 14, 16];

/// Graph memory rejects entries over 2,000 chars; pack below it so the
/// bracketed section prefix always fits.
const CHUNK_CHARS: usize = 1_800;

/// The document split into its numbered sections (the text before the first
/// heading is the preamble, returned by [`constitution_core`], not here).
#[must_use]
pub fn constitution_sections() -> Vec<ConstitutionSection> {
    let mut sections: Vec<ConstitutionSection> = Vec::new();
    for line in CONSTITUTIONAL_PROMPT.lines() {
        if let Some(heading) = line.strip_prefix("## ")
            && let Some((number, title)) = heading.split_once(". ")
            && let Ok(number) = number.parse::<u8>()
        {
            sections.push(ConstitutionSection {
                number,
                title: title.trim().to_owned(),
                body: String::new(),
            });
        } else if let Some(current) = sections.last_mut() {
            current.body.push_str(line);
            current.body.push('\n');
        }
    }
    for s in &mut sections {
        s.body = s.body.trim().to_owned();
    }
    sections
}

/// The token-efficient always-on constitution: preamble + the [`CORE_SECTIONS`]
/// verbatim, plus an index telling the agent the remaining sections live in
/// memory (retrieved tri-modally via `memory_search` — ADR-013/ADR-028).
#[must_use]
pub fn constitution_core(name: &str) -> String {
    let preamble = CONSTITUTIONAL_PROMPT
        .split("\n## ")
        .next()
        .unwrap_or_default()
        .trim();
    let mut out = String::from(preamble);
    let mut indexed: Vec<String> = Vec::new();
    for s in constitution_sections() {
        if CORE_SECTIONS.contains(&s.number) {
            out.push_str(&format!("\n\n## {}. {}\n\n{}", s.number, s.title, s.body));
        } else {
            indexed.push(format!("{}. {}", s.number, s.title));
        }
    }
    out.push_str(&format!(
        "\n\nThe remaining sections of your constitution ({}) are stored verbatim in your \
         memory. When faith, doctrine, your basis or origins, evangelism, advice boundaries, \
         or similar topics come up, retrieve them with the memory_search tool (query \
         'constitution <topic>') and follow them as part of this document.",
        indexed.join(" · ")
    ));
    out.replace("[Agent Name]", name)
}

/// The full document as graph-memory entries: `(node name, content)` pairs,
/// one or more per section, each within the memory entry cap. Long sections
/// split on paragraph boundaries; every chunk carries a bracketed section
/// prefix so it stands alone when retrieved.
#[must_use]
pub fn constitution_chunks() -> Vec<(String, String)> {
    let mut chunks = Vec::new();
    for s in constitution_sections() {
        let prefix = format!("[Constitution §{} — {}]", s.number, s.title);
        // Pack paragraphs; a paragraph over the cap (a long bullet list) is
        // split per line so no single unit can overflow a chunk.
        let mut units: Vec<&str> = Vec::new();
        for para in s.body.split("\n\n") {
            if para.chars().count() > CHUNK_CHARS {
                units.extend(para.lines());
            } else {
                units.push(para);
            }
        }
        let mut parts: Vec<String> = Vec::new();
        let mut current = String::new();
        for unit in units {
            if !current.is_empty()
                && current.chars().count() + unit.chars().count() + 1 > CHUNK_CHARS
            {
                parts.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(unit);
        }
        if !current.is_empty() {
            parts.push(current);
        }
        let total = parts.len();
        for (i, part) in parts.into_iter().enumerate() {
            let name = if total > 1 {
                format!(
                    "constitution:{:02}-{} ({}/{total})",
                    s.number,
                    slug(&s.title),
                    i + 1
                )
            } else {
                format!("constitution:{:02}-{}", s.number, slug(&s.title))
            };
            chunks.push((name, format!("{prefix} {part}")));
        }
    }
    chunks
}

fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
#[path = "constitution_tests.rs"]
mod tests;
