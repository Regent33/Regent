/// One normalized inbound message from any platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEvent {
    pub platform: String,
    pub chat_id: String,
    pub user_id: String,
    pub text: String,
}

impl MessageEvent {
    /// `platform:chat` — the key approvals and guards route on.
    #[must_use]
    pub fn chat_key(&self) -> String {
        format!("{}:{}", self.platform, self.chat_id)
    }

    /// `platform:user` — the key the auth policy evaluates.
    #[must_use]
    pub fn user_key(&self) -> String {
        format!("{}:{}", self.platform, self.user_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundMessage {
    pub chat_id: String,
    pub text: String,
}

/// The session-key convention. Never construct keys by hand —
/// always through this function.
#[must_use]
pub fn build_session_key(platform: &str, chat_id: &str) -> String {
    format!("agent:main:{platform}:{chat_id}")
}

/// One slash command — a single registry feeds dispatch, help text, and
/// (later) CLI autocomplete, so surfaces can never drift (the
/// `COMMAND_REGISTRY` pattern).
#[derive(Debug, Clone, Copy)]
pub struct CommandDef {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    /// May reach the runner while an agent is busy (the two-level guard
    /// bypass: stop/approve/deny must never be queued behind a turn).
    pub bypass_when_running: bool,
}

pub const COMMAND_REGISTRY: &[CommandDef] = &[
    CommandDef {
        name: "help",
        description: "List available commands.",
        aliases: &[],
        bypass_when_running: false,
    },
    CommandDef {
        name: "new",
        description: "Start a fresh conversation.",
        aliases: &["reset"],
        bypass_when_running: false,
    },
    CommandDef {
        name: "stop",
        description: "Interrupt the running turn.",
        aliases: &[],
        bypass_when_running: true,
    },
    CommandDef {
        name: "approve",
        description: "Approve the pending dangerous action.",
        aliases: &[],
        bypass_when_running: true,
    },
    CommandDef {
        name: "deny",
        description: "Deny the pending dangerous action.",
        aliases: &[],
        bypass_when_running: true,
    },
    CommandDef {
        name: "pair",
        description: "Generate a pairing code for a new user.",
        aliases: &[],
        bypass_when_running: false,
    },
];

/// Resolves `/name args` (or an alias) to its definition + argument rest.
#[must_use]
pub fn resolve_command(text: &str) -> Option<(&'static CommandDef, &str)> {
    let rest = text.strip_prefix('/')?;
    let (name, args) = rest.split_once(' ').unwrap_or((rest, ""));
    COMMAND_REGISTRY
        .iter()
        .find(|def| def.name == name || def.aliases.contains(&name))
        .map(|def| (def, args.trim()))
}

#[must_use]
pub fn render_help() -> String {
    let mut out = String::from("Commands:\n");
    for def in COMMAND_REGISTRY {
        out.push_str(&format!("/{} — {}\n", def.name, def.description));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_resolution_handles_aliases_args_and_unknowns() {
        let (def, args) = resolve_command("/stop").unwrap();
        assert_eq!(def.name, "stop");
        assert!(def.bypass_when_running);
        assert_eq!(args, "");

        let (def, args) = resolve_command("/reset please").unwrap();
        assert_eq!(def.name, "new");
        assert_eq!(args, "please");

        assert!(resolve_command("/unknown").is_none());
        assert!(resolve_command("plain text").is_none());
    }

    #[test]
    fn help_is_generated_from_the_registry() {
        let help = render_help();
        for def in COMMAND_REGISTRY {
            assert!(help.contains(def.name), "help must list /{}", def.name);
        }
    }

    #[test]
    fn session_key_format_is_stable() {
        assert_eq!(
            build_session_key("telegram", "42"),
            "agent:main:telegram:42"
        );
    }
}
