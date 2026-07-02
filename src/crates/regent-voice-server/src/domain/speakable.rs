//! Text → speech-ready text. Two sanitizers, matching the Python server's
//! `_speakable` pair: [`strip_markdown`] (python_server.py — symbols the TTS
//! engine would read aloud: "asterisk", "slash", …) and [`strip_spoken`]
//! (web_call.py — reasoning `<think>` blocks and emoji, which TTS reads as
//! their names).

use regex::Regex;
use std::sync::LazyLock;

static FENCED_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"```[\s\S]*?```").unwrap());
static MD_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap());
static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s{0,3}#{1,6}\s+").unwrap());
static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*[-*+]\s+").unwrap());
static NUMBERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*\d+\.\s+").unwrap());
static STRUCT_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[`*_~#>|]").unwrap());
static SPACE_RUNS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]{2,}").unwrap());
static THINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<think>.*?</think>").unwrap());
// Emoji / pictographs — the main emoji blocks, without touching ASCII.
static EMOJI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "[\u{1f000}-\u{1faff}\u{2600}-\u{27bf}\u{1f1e6}-\u{1f1ff}\
         \u{fe00}-\u{fe0f}\u{2b00}-\u{2bff}\u{2190}-\u{21ff}]+",
    )
    .unwrap()
});

/// Strip markdown/structural symbols before synthesis: fenced code, links →
/// label, headings, bullet/numbered markers, emphasis chars, `/` (never read
/// "slash" aloud), collapsed space runs. Keeps the words, drops the noise.
#[must_use]
pub fn strip_markdown(text: &str) -> String {
    let t = FENCED_CODE.replace_all(text, " ");
    let t = MD_LINK.replace_all(&t, "$1");
    let t = HEADING.replace_all(&t, "");
    let t = BULLET.replace_all(&t, "");
    let t = NUMBERED.replace_all(&t, "");
    let t = STRUCT_CHARS.replace_all(&t, " ");
    let t = t.replace('/', " ");
    let t = SPACE_RUNS.replace_all(&t, " ");
    t.trim().to_owned()
}

/// Text fit to read aloud: no `<think>…</think>`, no emoji, collapsed
/// whitespace.
#[must_use]
pub fn strip_spoken(text: &str) -> String {
    let t = THINK.replace_all(text, "");
    let t = EMOJI.replace_all(&t, "");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_symbols_are_dropped_words_kept() {
        assert_eq!(strip_markdown("**bold** and `code`"), "bold and code");
        assert_eq!(strip_markdown("## Heading\ntext"), "Heading\ntext");
        assert_eq!(
            strip_markdown("- item one\n- item two"),
            "item one\nitem two"
        );
        assert_eq!(strip_markdown("1. first\n2. second"), "first\nsecond");
        assert_eq!(
            strip_markdown("[the docs](https://x.example/a)"),
            "the docs"
        );
        assert_eq!(strip_markdown("a/b"), "a b", "never read 'slash' aloud");
        assert_eq!(strip_markdown("```rust\nlet x = 1;\n```after"), "after");
    }

    #[test]
    fn spoken_text_loses_think_blocks_and_emoji() {
        assert_eq!(strip_spoken("<think>secret plan</think>Hello"), "Hello");
        assert_eq!(
            strip_spoken("<THINK>x\ny</THINK> hi"),
            "hi",
            "case + multiline"
        );
        assert_eq!(strip_spoken("Great job! 🎉🎉"), "Great job!");
        assert_eq!(strip_spoken("a   b\n\n c"), "a b c", "whitespace collapsed");
    }
}
