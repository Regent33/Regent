# Guide — the CLI: using it, and checking it works

Everything here was run against the compiled binary before it was written down.
Where a number appears, it is measured, not estimated; where something is not
finished, it says so rather than reading as though it were.

Companion pages: [`commands.md`](commands.md) for the full command list,
[`env-vars.md`](env-vars.md) for the environment.

---

## 1. The two ways to talk to Regent

```bash
regent                       # interactive chat (the default)
regent ask "what is this repo"   # one question, one answer, exit
```

`regent` opens the full-screen chat. `regent ask` is for scripts, pipes and
anything where you want an answer rather than a session.

Piping into bare `regent` used to hang forever. It now refuses immediately:

```bash
$ echo hi | regent
✗ regent chat needs an interactive terminal — stdin is not a terminal.

  Piping into `regent` is not supported: there is no one-shot mode yet.
  For a scripted task use:  regent code "<task>"
  Misdetected terminal? Set REGENT_FORCE_TTY=1.
$ echo $?
2
```

If your terminal is misdetected (some Git Bash, mintty and multiplexer setups
report no TTY when there is one), `REGENT_FORCE_TTY=1 regent` overrides it.

---

## 2. `regent ask` — headless

### The whole front door

```bash
regent ask "what changed in this repo today"
echo "summarise this file" | regent ask
```

Four flags, and most uses need none:

| Flag | Does |
|---|---|
| `-c`, `--continue` | continue the last session instead of starting fresh |
| `--json` | one JSON object instead of prose — for scripts |
| `--yes` | allow gated actions instead of denying them |
| `--timeout S` | give up after S seconds |

Automation extras, in `regent ask --help`: `--events` (NDJSON stream),
`--session <id>`.

### Output modes

**Default** — the answer streams to stdout, progress to stderr:

```bash
$ regent ask "Reply with exactly: HEADLESS OK"
session sess_a2294d9a50a04b46ae2247fa729a3ab4     # ← stderr
HEADLESS OK                                        # ← stdout
```

The session id goes to **stderr**, never into the answer, so this is safe:

```bash
answer=$(regent ask "one line summary" 2>/dev/null)
```

**`--json`** — one document, buffered, atomic by construction:

```json
{
  "type": "run.completed",
  "schema_version": 1,
  "status": "completed",
  "answer_complete": true,
  "denied_approvals": 0,
  "tool_calls": 0,
  "run_id": "run_c4a8b82a76374f4f",
  "session_id": "sess_55cbea96d2bf47abbfe2c40c46133b97",
  "answer": "JSON OK"
}
```

**`--events`** — NDJSON, one object per line, stdout only:

```
{"type":"run.started","schema_version":1,"session_id":"sess_11e8…"}
{"type":"turn.started","schema_version":1,"session_id":"sess_11e8…"}
{"type":"turn.usage","schema_version":1,"input_tokens":11595,"output_tokens":34,…}
{"type":"message.complete","schema_version":1,"reply":"EVENTS OK",…}
{"type":"turn.complete","schema_version":1,…}
{"type":"run.completed","schema_version":1,"status":"completed","answer_complete":true,…}
```

`--json` and `--events` are separate flags on purpose: `--json` yields one valid
JSON document, `--events` yields a stream of them. Asking for both is a usage
error rather than a guess.

### Exit codes

| Code | Means |
|---|---|
| `0` | completed per policy |
| `1` | execution or runtime failure |
| `2` | usage or local validation (bad flag, no prompt, both prompt and stdin) |
| `3` | policy or budget prevented completion (includes `--timeout` firing) |
| `4` | required resource unavailable (no deacon, session could not be opened) |
| `130` | interrupted |

A run that never reaches a terminal event exits `1`, even if it printed part of
an answer — a dead deacon or a dropped stream is not a success.

**A denied approval is not a failure.** A run refused a write that still answers
exits `0`; what actually happened is in the terminal event
(`denied_approvals`, `answer_complete`). Scripts should read that, not infer
from the exit code.

### Approvals in a headless run

A headless run never prompts. Without `--yes`, any action that reaches the
approval gate is **denied** and the run continues:

```bash
$ regent ask "delete the build directory"
denied: terminal (rm -rf build) — pass --yes to allow it     # ← stderr
```

> **Read this before treating the default as read-only.** The approval gate does
> not currently see `write_file`, `file_edit`, `apply_patch` or
> `create_document` — those tools have no approval call, and no permission rules
> are configured by default. So `--yes` widens what is *gated*; it is not the
> boundary between "reads" and "writes". The full map, with file and line
> references, is in
> [`../audits/approval-sandbox-boundary-2026-07-31.md`](../audits/approval-sandbox-boundary-2026-07-31.md).

---

## 3. Configuration

There is one implementation of "change a config key" — the Rust one that
validates the whole file against the real schema before writing, under a lock,
atomically. The CLI reaches it two ways: over RPC when a deacon is running (so
open sessions pick the change up live), and by running the deacon binary as a
one-shot when it is not.

Every command that changes config.yaml goes through it, not just `config set`:
`regent setup`, `regent providers add|remove`, `regent agents mom
create|remove`, and `regent voice setup|enable|disable`. Each of those used to
carry its own YAML writer with the same flaw — on a parse error it "started
fresh", so one bad line plus a re-run silently replaced the whole file. Because
the write is now one transaction, a command that sets several related keys
applies all of them or none.

```bash
regent config list            # keys you have changed
regent config list --all      # every key the schema defines
regent config list --json     # for scripts; secrets are already redacted
regent config set model.default claude-opus-5
regent config set tools.deferred '["calc","weather"]'   # JSON arrays stay arrays
regent config unset model.defalut
regent config validate
```

### A typo cannot brick your install any more

```bash
$ regent config set moddel.default x
✗ rejected — this would break config.yaml: unknown field `moddel`,
  expected one of `_config_version`, `model`, `context`, `limits`, `memory`,
  `cron`, `board`, `http`, `tools`, `speech`, `providers`, `agents_defaults`,
  `mom`, `constitution` at line 4 column 1
$ echo $?
2
```

The file is byte-identical afterwards. This works **with the deacon stopped**
too — validation does not need a daemon.

### Repairing a config that stops the deacon

```bash
$ regent config validate
✗ config.yaml did not validate: model: unknown field `defalut`,
  expected one of `default`, `provider`, `base_url`, `review` at line 3 column 3
  `regent config unset <key>` removes an offending key

$ regent config unset model.defalut
unset model.defalut
$ regent config validate
✓ config.yaml loads and validates
```

Two failures are kept apart because the fix differs:

* **schema-invalid** — parses as YAML but is not a config. `config unset` repairs it.
* **malformed** — not YAML at all. Nothing will rewrite it; you edit it by hand.
  A file that cannot be parsed is **never** overwritten.

### The deacon's own offline surface

The CLI is a thin client over these; use them directly when scripting:

```bash
regent-deacon config describe   # every key: type, default, value, origin
regent-deacon config validate
regent-deacon config set model.default '"claude-opus-5"'   # value is JSON
regent-deacon config unset model.defalut

# Several keys, ONE transaction — all of them or none:
regent-deacon config set model.provider '"ollama"' model.default '"llama3.2"'
```

`set` takes any number of `<key> <json-value>` pairs. Passing them together is
not just faster (one process, one lock): a refusal on the last pair leaves the
earlier ones unapplied, which is what keeps `regent setup` from producing a
half-configured install.

`describe` output is versioned (`descriptor_version`), so a script can check the
shape before trusting the field names.

---

## 4. Health and posture

```bash
regent doctor              # is the install working
regent doctor --strict     # …and fail on warnings too (for CI)
regent doctor --json       # one document
regent security            # what is this session allowed to do
regent security --json
```

`doctor` reports three severities — passed, worth warning about, actually
broken. **Its default exit code is unchanged**: warnings do not fail it, because
a health command that cries wolf is one people stop running. `--strict` is the
opt-in for CI.

`security` reports each control's value, where it came from, and whether it
deviates from the default. "Safe" is contextual: a loopback listener with a
token reads differently from an unauthenticated one bound to `0.0.0.0`.

```
$ regent security
regent security
  ✗ approvals                  AUTO-APPROVED
    every tool runs without asking (ask_user still reaches you) · from config.yaml
  ✓ REGENT_UNSAFE_NO_SANDBOX   off
    filesystem jail in force · from default
  ✓ REGENT_SANDBOX             off
    shell runs on the host · from default
  ✗ http listener              on · bind=0.0.0.0:8080 · token=MISSING
    listener is ON with NO token — anything that can reach it can drive the agent · from config.yaml
  ✓ tool hooks                 none
    no shell runs around tool dispatch · from default
  …
$ echo $?
1
```

Exit `1` when anything is `✗`, `0` otherwise — `!` (worth reviewing) does not
fail it, for the same reason warnings do not fail `doctor`.

When a session is less guarded than default, the **status line says so** while
you are using it — `⚠ auto-approve · sandbox off` — because a control nobody can
see is a control nobody can rely on.

---

## 5. The chat composer

| Key | Does |
|---|---|
| `enter` | send |
| `alt+enter` / `shift+enter` | new line |
| `↑ ↓` | move a line; recall history when the input is single-line |
| `← →` | move by character |
| `ctrl+← →` | move by word |
| `home` / `ctrl+a` | start of line |
| `end` / `ctrl+e` | end of line |
| `ctrl+w` | delete the word before the cursor |
| `ctrl+u` | delete to start of line |
| `ctrl+k` | delete to end of line |
| `/` | command picker — `↑↓` select, `⇥` complete, `↵` run |
| `?` | show this list (on an empty line) |
| `ctrl+c` | interrupt · twice to quit |

Pasting a stack trace, a diff or a log excerpt now arrives as **one message**.
Bracketed paste is enabled while the composer is active, so newlines inside a
paste are content, not a send.

`shift+enter` only works in terminals that send it (kitty protocol / CSI-u);
`alt+enter` works essentially everywhere. Both are wired, so you can use
whichever your terminal delivers.

---

## 6. Shell completions

```bash
regent completions bash > /etc/bash_completion.d/regent
regent completions zsh  > ~/.zsh/completions/_regent
regent completions fish > ~/.config/fish/completions/regent.fish
regent completions powershell | Out-String | Invoke-Expression   # PowerShell
```

They are generated from the same table `regent help` reads, so they cannot
offer a command that does not exist. They are deliberately **static** — a
completion that starts a daemon or blocks on RPC is worse than no completion.

---

## 7. Testing it yourself

### The automated suites

```bash
# TypeScript CLI — 184 tests
cd src/regent-cli
bun install
bunx tsc --noEmit          # types
bunx biome check src       # lint + format
bun run compile            # MUST run before `bun test`: the *.int.test.ts
bun test                   #   suites black-box the compiled binary and skip
                           #   themselves when it is missing

# Rust — config, approval boundary, everything else
cd ../..
cargo fmt --all -- --check
cargo clippy --workspace --exclude regent-voice-server --all-targets
cargo test --workspace --exclude regent-voice-server
```

CI runs the CLI job on **Ubuntu and Windows**, and compiles before testing for
the reason above.

### The suites worth knowing about

| Suite | What it protects |
|---|---|
| `src/regent-cli/src/app/cli/cli.int.test.ts` | the shell contract of the **compiled** binary — exit codes, stdout/stderr separation, the non-TTY guard |
| `src/regent-cli/src/app/config/commandSpec.test.ts` | reads the router's own source: a command in one and not the other fails CI |
| `src/regent-cli/src/features/chat/domain/composer.test.ts` | paste, readline keys, cursor — no terminal needed |
| `src/regent-cli/src/features/ask/domain/askRun.test.ts` | the headless machine contract: exit codes and the terminal event |
| `src/crates/regent-deacon/src/infra/tests/config_offline.rs` | offline validation, locking, atomic writes, secret redaction |
| `src/crates/regent-tools/tests/approval_coverage.rs` | every registered tool has a recorded approval posture; a denied command does not execute |

### Checking it by hand

Use a scratch home so your real one is never touched:

```bash
export REGENT_HOME=$(mktemp -d)
touch "$REGENT_HOME/.setup-done"     # skip the onboarding wizard
```

```bash
# 1. Shell contract
regent --version;            echo "expect 0: $?"
regent nosuchcmd;            echo "expect 2: $?"     # diagnostic on stderr
regent --nosuchopt;          echo "expect 2: $?"     # "unknown option", not command
regent --profile --help;     echo "expect 2: $?"     # not a profile called --help
regent nosuchcmd 2>/dev/null # stdout must be EMPTY

# 2. Non-TTY guard (must not hang)
echo hi | regent;            echo "expect 2: $?"
regent < /dev/null;          echo "expect 2: $?"

# 3. Config safety
printf '_config_version: 2\nmodel:\n  default: claude-sonnet-4-6\n' > "$REGENT_HOME/config.yaml"
regent config set moddel.default x;  echo "expect 2: $?"
cat "$REGENT_HOME/config.yaml"       # unchanged

printf '_config_version: 2\nmodel:\n  defalut: typo\n' > "$REGENT_HOME/config.yaml"
regent config validate;              echo "expect 1: $?"
regent config unset model.defalut;   echo "expect 0: $?"
regent config validate;              echo "expect 0: $?"

# 4. Malformed YAML is never rewritten
printf 'model:\n  default: "unclosed\n  bad: [\n' > "$REGENT_HOME/config.yaml"
before=$(md5sum "$REGENT_HOME/config.yaml")
regent config set model.default x;   echo "expect 1: $?"
[ "$before" = "$(md5sum "$REGENT_HOME/config.yaml")" ] && echo "file untouched ✓"

# 5. Help, completions and posture
regent code --help                   # usage, flags, examples — not one line
regent completions bash | bash -n - && echo "bash syntax ✓"
regent config list --all | wc -l     # every key the schema defines (52 today)
regent security;                     echo "0 clean / 1 has an ✗: $?"

# 6. Headless (needs a working provider key)
unset REGENT_HOME
regent ask "Reply with exactly: OK"
regent ask "Reply with exactly: OK" --json
echo "Reply with exactly: OK" | regent ask 2>/dev/null
regent ask "Reply with exactly: OK" --events
```

On Windows PowerShell the same checks work with `$LASTEXITCODE` in place of
`$?`, and `Get-FileHash` in place of `md5sum`.

---

## 8. Measured numbers

Windows 11, Bun 1.3.14, Windows Defender active, 20 runs each, warm:

| | |
|---|---|
| `regent --version` | ~270 ms |
| `regent help` | ~274 ms |
| `regent code --help` | ~271 ms |
| `regent completions bash` | ~270 ms |
| compiled artefact | 96 MB |

These are **deacon-free** paths — no daemon is spawned. Treat them as a trend
baseline on one machine, not a performance gate: a 96 MB binary is sensitive to
antivirus and signing state, and shared CI runners are not a stable environment.

---

## 9. What is not done

Stated plainly so nobody discovers it the hard way.

* **`ask --max-turns` / `--max-tool-calls` / `--policy`** are not implemented.
  They need "turn" and "tool call" defined first, and a scoped budget model.
* **A true read-only headless mode** needs permission rules, because the
  approval gate does not see file writes (§2 above).
* **Descendant processes** are not tree-killed when a run is force-stopped; the
  deacon is killed, but a container or compiler it spawned may outlive it. Both
  platforms need process-tree ownership (Job Objects / process groups).
* **`run_id` identifies the invocation, not a turn.** The deacon has no per-turn
  id, so `--session`/`-c` runs against the same session are not isolated from
  one another: cancelling one cancels that session's current turn, whoever
  started it. Each invocation does get its own deacon child, so the transport
  and its notifications are isolated — but persistent session state is shared.
* **`config edit`**, `config restore` and self-replacing `regent update` are
  deliberately not built; see the plan's refusal list.
