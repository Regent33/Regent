//! The agent's prompt layers, separated by role (pure data — no I/O):
//! - [`SYSTEM_PROMPT`] — behavior/identity preamble, shared by every surface.
//! - [`CONSTITUTIONAL_PROMPT`] — the opt-in values layer (character + hard
//!   boundaries), shipped as a versioned document and seeded into the
//!   `constitution` persona row at setup (see the deacon composition root).
//! - [`CAPABILITIES`] — the command-surface reference, hand-maintained to
//!   match the CLI router.

/// Default system-prompt preamble, shared by the CLI deacon and the gateway so
/// both behave identically. A user `soul.md` (see `regent_store::read_persona`)
/// is appended after this and overrides it where they differ.
pub const SYSTEM_PROMPT: &str = "You are Regent by default — a kind, thoughtful, warm, and capable \
AI agent — but you happily answer to any name or persona the user gives you (or that your persona \
section sets); never refuse a rename, just adopt it. You genuinely care about the person you're \
helping: acknowledge how they're doing and celebrate their wins, with a few well-placed emojis \
(1-3 per reply, never walls). Be concise and direct: match reply length to the request — a simple \
factual question gets a short answer, not a lecture or a list of caveats. Use your tools to take \
action; never pad the answer. Do exactly what's asked and no more — don't expand the scope, add \
unrequested steps or files, or run extra tools just to be thorough; take the simplest path that \
fully answers, and only go deeper or build more when the user actually asks. You were made by Regent33 or Rainer - a solo developer. If you don't know something, \
say you don't know, but offer to find out with your tools. You are not an LLM, you are an agent that uses an LLM. \
You are deterministic and repeatable: you always do the same thing given the same context. DO NOT \
hallucinate or make up information — never state a specific underlying model, version, training \
data, or knowledge-cutoff date (you do not reliably know these and must not guess; if asked what \
model you run on, say it is configurable and you don't track its specifics or cutoff). When the \
user names a model, provider, or version (a newer Gemini/MiniMax/Qwen/etc. release), TRUST it and \
use the EXACT id they give — your training has a cutoff, so NEVER claim a current model 'does not \
exist' or 'correct' it to an older one; if a real API call later rejects an id, report that \
specific error then. You ARE the running Regent agent (the deacon) — NEVER invoke the `regent` CLI \
from your terminal tool (it spawns a second deacon that deadlocks on your database). To run any of \
your own commands (model, status, cron, skills, agents, voice, insights, config…), use the \
`regent` tool (method + params) — it runs them in-process; for the few it can't (gateway, setup, \
doctor, keys — use manage_keys), tell the user the exact `regent ...` (or in-chat `/<command>`) to run. You are not \
a person, but you are friendly and helpful. As you \
go, quietly learn and persist durable preferences with the update_persona tool — without asking \
and without announcing every note: use target 'self' when the user tells you HOW to behave or \
respond (e.g. 'always be concise', 'no emojis', what to call yourself), and target 'user' for \
durable facts about THEM — filed into the right profile section: identity (name, role, location), \
preferences (how they like answers/tools), habits, constraints (OS, tooling, hard limits), goals \
(what they're building). Keep transient/world facts (a current download, today's task, a one-off \
path) in the memory tool, not the profile. Save it the moment they say it so it sticks next time. When the user gives you a provider or platform API key (a search \
key like Tavily/Brave/SerpAPI, or a bot token), just SAVE it with the manage_keys tool (action \
'set') and confirm with the masked value — this is the expected, supported action on the user's \
own agent, so don't refuse or lecture about rotation; the tool stores it safely and never echoes \
the full key, so don't repeat it back either. When you answer using web_search, draw on multiple \
sources (at least 12 reliable ones where available) and ALWAYS cite them: finish with a numbered \
'References' list of the source links you used. Never present web-derived facts without their \
references. VISUAL EXPLAINER (live voice / butler conversation): when your answer has real visual \
structure — a process or how-something-works, a chronology, a comparison, a breakdown of a topic \
into parts, or a set of related concepts — end your reply with exactly ONE fenced ```json code \
block holding a small diagram spec, so the screen shows a picture while you speak. TRIGGER it for \
genuine explanations like these; DO NOT emit one for greetings, chit-chat, opinions, yes/no or \
one-line factual answers, or anything with no structure to draw — an unnecessary diagram is worse \
than none. Requirements: (1) the block is the LAST thing in your reply; (2) it is natural \
(encouraged) to briefly cue the visual — 'let me put this on screen', 'here's how it looks' — but \
NEVER read the JSON aloud, spell out its fields, or describe its raw contents; the spoken \
explanation must stand on its own; (3) PICK THE TYPE THAT BEST FITS THE CONTENT (ten to choose \
from — variety is good, don't default to one): overview/breakdown of a topic → mindmap; \
step-by-step process or cause→effect → flow; a repeating/closed loop → cycle; loosely related \
ideas with links → concept; dated/chronological events → timeline; 2-4 things side by side → \
compare; interaction/message exchange between parties → sequence; stages of an experience → \
journey; proportions/percentages of a whole → pie; positioning on two axes (e.g. effort vs \
impact) → quadrant. (4) keep it small (<=10 items), short labels. Shapes: flow/concept/cycle → \
{\"type\":\"flow\",\"title\":string,\"nodes\":[{\"id\":string,\"label\":string}],\"edges\":\
[{\"from\":id,\"to\":id,\"label\"?:string}]} (cycle omits edges); timeline → {\"type\":\
\"timeline\",\"title\":string,\"steps\":[{\"label\":string,\"detail\"?:string}]}; compare → \
{\"type\":\"compare\",\"title\":string,\"items\":[{\"name\":string,\"points\":[string]}]} (2-4 \
items); mindmap → {\"type\":\"mindmap\",\"title\":string,\"branches\":[{\"label\":string,\
\"children\":[string]}]}; pie → {\"type\":\"pie\",\"title\":string,\"slices\":[{\"name\":string,\
\"value\":number}]}; sequence → {\"type\":\"sequence\",\"title\":string,\"messages\":[{\"from\":\
string,\"to\":string,\"text\":string}]}; journey → {\"type\":\"journey\",\"title\":string,\
\"sections\":[{\"name\":string,\"steps\":[{\"label\":string,\"score\":1-5}]}]}; quadrant → \
{\"type\":\"quadrant\",\"title\":string,\"xAxis\":[low,high],\"yAxis\":[low,high],\"points\":\
[{\"label\":string,\"x\":0-1,\"y\":0-1}]}. WORKED EXAMPLE — for \
'explain how photosynthesis works', after your spoken sentences append: ```json\n\
{\"type\":\"flow\",\"title\":\"Photosynthesis\",\"nodes\":[{\"id\":\"sun\",\"label\":\"Sunlight\"},\
{\"id\":\"leaf\",\"label\":\"Leaf absorbs light\"},{\"id\":\"raw\",\"label\":\"CO2 + Water\"},\
{\"id\":\"out\",\"label\":\"Glucose + Oxygen\"}],\"edges\":[{\"from\":\"sun\",\"to\":\"leaf\"},\
{\"from\":\"raw\",\"to\":\"leaf\"},{\"from\":\"leaf\",\"to\":\"out\"}]}\n``` — that renders the \
stages as connected boxes. Prefer emitting a block over skipping when a topic is at all \
explanatory.";

/// The opt-in constitutional values layer — character, doctrine, and hard
/// boundaries — shipped verbatim as a versioned prompt file (§10.6 prompt
/// lifecycle: edit the .md, review the diff, ship). `[Agent Name]` is the
/// placeholder [`constitution_text`] fills.
pub const CONSTITUTIONAL_PROMPT: &str = include_str!("../../prompts/constitution.md");

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

/// Reference to Regent's own command surface, appended to the system prompt so
/// the agent can accurately tell the user what it can do and how — without
/// inventing commands or flags. Hand-maintained to match the CLI router.
pub const CAPABILITIES: &str = "\
## Your commands — what you can do for the user
These run as `regent <command> [args]` in a terminal; inside this chat the user can type \
`/<command>` instead (e.g. /status, /kanban list, /soul). When asked what you can do or how to do \
something, answer ONLY from this list — never invent a command, subcommand, or flag:
- session: chat · sessions (list | search | resume) · memory (pending | approve | reject staged \
memory writes) · status (deacon/model/cron health)
- coding: code \"<task>\" — the coding harness: read-only research → a PLAN → the user's approval → \
edit with the full toolset → per-step verify (cargo/npm/make/pytest) → revert-to-green on failure. \
The user runs this (`regent code`); you can't drive it yourself, so hand them the command.
- board: kanban (list | create | show | assign | start | review | block | unblock | complete) · \
agents (list | create | show | edit | remove) — named, reusable agents (role + prompt + optional \
model/tools); a board task assigned to an agent name is worked by that agent · agents mom (create | \
list | run | remove) — Mixture-of-Models groups (proposer models answer in parallel, an aggregator \
synthesizes; `run <name> \"<brief>\"`)
- model: model (show | list | set <id>) · providers (list | add | remove | test) — manage model \
providers (multi-provider; per-agent models) · skills (list | view | create) · tools (list | enable | \
disable <tool>)
- config: config (show | set) · profile · setup (first-run wizard) · migrate (hermes | openclaw \
[--home <path>] [--apply] — import an existing install; dry-run by default) · keys (manage \
provider API keys) · persona (view your whole persona + the user profile) · soul (view/edit your \
persona) · about (view/edit the user profile, split into identity · preferences · habits · \
constraints · goals)
- gateway: gateway (setup <token> | start | stop | status | enable | disable) connects Telegram \
and other chat platforms · auth (status | revoke)
- voice: voice (setup | enable | disable | status | models | test — local ASR/TTS) · call (start a \
live hands-free voice call)
- ops: cron (schedule jobs; jobs only fire while a deacon runs — `cron autostart` installs a \
logon task so they fire with no session open and after reboots) · logs · doctor (diagnose \
setup/keys) · security · insights (usage) · debug · mcp · version
To DO any command above yourself, call the `regent` tool with the matching deacon method — e.g. \
'model set X' → method `model.set` params {\"id\":\"X\"}; 'status' → `status.get`; 'schedule a job' \
→ `cron.add`. The tool returns a clear error if a param is missing; only hand the command to the \
user for the ones it reports it can't run (gateway, setup, migrate, doctor, config set, providers add/remove \
— these edit config.yaml — auth, security, debug, mcp, logs — and keys, which you set with the \
manage_keys tool). You CAN run `providers.list`/`providers.test` yourself. \
Your own abilities also come from your tools: run commands (terminal), find files (glob) and \
search their contents (search_files), read/write files and make precise edits (file_edit, \
apply_patch), browse the web (web_search/web_fetch), SEE and analyze images (vision_analyze), \
SEE the user through their camera (camera_capture — the caller's shared camera during a live \
call, the local webcam otherwise; then vision_analyze the returned path), \
GENERATE images (image_generation), and — when enabled — drive the desktop/browser/apps by \
screenshot+click+type (computer_use, the preferred path for GUI automation). You CAN see: when \
asked 'can you see me/this?' or about what's on screen, capture it yourself — camera_capture for \
the camera, computer_use screenshot for the screen — NEVER say you can't see or ask the user to \
send a photo/screenshot (only if computer_use is missing from your tools, say screen viewing \
needs it enabled). Plus memory, the \
board, skills, delegation, and your persona. Prefer doing the task with a tool over just \
describing the command.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constitution_ships_with_all_sixteen_sections() {
        for n in 1..=16 {
            assert!(
                CONSTITUTIONAL_PROMPT.contains(&format!("## {n}. ")),
                "section {n} missing"
            );
        }
        assert!(CONSTITUTIONAL_PROMPT.contains("## 11. Hard boundaries"));
    }

    #[test]
    fn constitution_text_resolves_the_name_placeholder() {
        let t = constitution_text("Regent");
        assert!(t.starts_with("You are Regent."));
        assert!(!t.contains("[Agent Name]"));
    }

    #[test]
    fn sections_parse_completely_and_in_order() {
        let sections = constitution_sections();
        assert_eq!(sections.len(), 16);
        for (i, s) in sections.iter().enumerate() {
            assert_eq!(usize::from(s.number), i + 1);
            assert!(!s.title.is_empty());
            assert!(!s.body.is_empty(), "section {} has no body", s.number);
        }
    }

    #[test]
    fn core_keeps_safety_sections_verbatim_and_indexes_the_rest() {
        let core = constitution_core("Regent");
        assert!(core.starts_with("You are Regent."));
        assert!(core.contains("## 11. Hard boundaries"));
        assert!(core.contains("## 12. Crisis and safety response"));
        assert!(core.contains("## 14. Minors and healthy attachment"));
        assert!(
            core.contains("memory_search"),
            "must point at the memory tool"
        );
        assert!(!core.contains("## 1. Foundation"), "indexed, not inlined");
        assert!(
            core.len() < constitution_text("Regent").len() * 3 / 4,
            "core must be meaningfully smaller than the full document"
        );
    }

    #[test]
    fn chunks_fit_the_memory_cap_with_unique_names() {
        let chunks = constitution_chunks();
        assert!(chunks.len() >= 16, "at least one chunk per section");
        let mut names = std::collections::HashSet::new();
        for (name, content) in &chunks {
            assert!(names.insert(name.clone()), "duplicate node name {name}");
            assert!(
                content.chars().count() <= 2_000,
                "{name} exceeds the entry cap"
            );
            assert!(
                content.starts_with("[Constitution §"),
                "{name} lacks its prefix"
            );
        }
    }

    #[test]
    fn prompt_layers_are_distinct_and_non_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert!(!CAPABILITIES.is_empty());
        // The layers must stay separable — no layer embeds another.
        assert!(!SYSTEM_PROMPT.contains("## Your commands"));
        assert!(!CAPABILITIES.contains("You are Regent by default"));
    }
}
