# Using Regent from the command line

This is the everyday guide: what to type, what you get back, and how to tell
whether something is wrong. The first half assumes nothing. The second half is
for scripts, automation and people building Regent — clearly marked, so you can
stop reading when it stops being useful to you.

Everything below was run against the real program before it was written down.
Where there is a number, it was measured.

**Also here:** [`commands.md`](commands.md) lists every command ·
[`env-vars.md`](env-vars.md) lists every environment variable.

---

## One thing to know first

Regent is two programs that work together:

* **`regent`** — what you type. The chat window, the commands.
* **`regent-deacon`** — the part that does the actual work: talking to the AI,
  running tools, remembering your sessions. It starts and stops on its own; you
  do not launch it.

You will see the word **deacon** in error messages. That is the second one. If a
message says the deacon is missing, the install is incomplete.

---

## 1. The two ways to use it

```bash
regent                            # open the chat window
regent ask "what is this repo"    # ask one question, get one answer, done
```

Use `regent` when you want a conversation. Use `regent ask` when you want an
answer and nothing else — it prints the reply and exits, so it works in scripts.

### Feeding text in from another command

This does **not** work, and Regent tells you so straight away instead of
freezing:

```bash
$ echo hi | regent
✗ regent chat needs an interactive terminal — stdin is not a terminal.

  Piping into `regent` is not supported: there is no one-shot mode yet.
  For a scripted task use:  regent code "<task>"
  Misdetected terminal? Set REGENT_FORCE_TTY=1.
```

The chat window needs a real terminal — somewhere it can draw, and where you can
press keys. When text is piped in there is nobody to press keys, so it stops.

Use `regent ask` instead:

```bash
echo "summarise this" | regent ask
```

**If you get that message when you *are* in a real terminal:** some setups (Git
Bash, mintty, tmux and friends) report the wrong thing. Put `REGENT_FORCE_TTY=1`
in front to override it:

```bash
REGENT_FORCE_TTY=1 regent
```

---

## 2. Asking one question

```bash
regent ask "what changed in this repo today"
echo "summarise this file" | regent ask
```

That is the whole thing. There are four options, and most of the time you need
none of them:

| Option | What it does |
|---|---|
| `-c` | keep going in the last conversation instead of starting a new one |
| `--yes` | let Regent do things it would otherwise refuse (see below) |
| `--timeout 60` | give up after 60 seconds |
| `--json` | print a machine-readable block instead of prose |

What you see:

```bash
$ regent ask "Reply with exactly: HEADLESS OK"
session sess_a2294d9a50a04b46ae2247fa729a3ab4
HEADLESS OK
```

The `session …` line is a progress note, not part of the answer. Regent keeps
those separate on purpose, so if you capture the output you get the answer
alone:

```bash
answer=$(regent ask "one line summary" 2>/dev/null)
```

### ⚠ What `--yes` really means

Without `--yes`, some risky actions get refused and Regent carries on without
them:

```bash
$ regent ask "delete the build directory"
denied: terminal (rm -rf build) — pass --yes to allow it
```

**Do not read that as "safe mode".** Right now the permission check covers
shell commands, but it does **not** cover writing or editing files. So leaving
`--yes` off narrows what Regent will do — it does not stop it changing files on
disk.

If you need a genuine look-but-don't-touch mode, it does not exist yet. Run it
somewhere disposable. The exact list of what is and is not checked is in
[the approval boundary audit](../audits/approval-sandbox-boundary-2026-07-31.md).

---

## 3. Changing settings

Regent keeps your settings in one file, `config.yaml`, inside its home folder.
**You should not need to open it.** Change things with commands instead:

```bash
regent config list                              # what you have changed
regent config list --all                        # everything you could change
regent config set model.default claude-opus-5   # change one thing
regent config unset model.default               # put one thing back to default
regent config validate                          # is the settings file OK?
```

`regent config list` shows only what you have actually changed, because the full
list is 52 entries and "what did I change?" is the usual question:

```bash
$ regent config list
  model.default   "llama3.2"
  model.provider  "ollama"

47 more at their defaults — --all
```

### Why you cannot break it with a typo

Every settings change is checked against the list of settings Regent actually
understands, *before* anything is saved. A typo is refused and nothing is
written:

```bash
$ regent config set moddel.default x
✗ rejected — this would break config.yaml: unknown field `moddel`,
  expected one of `_config_version`, `model`, `context`, `limits`, `memory`,
  `cron`, `board`, `http`, `tools`, `speech`, `providers`, `agents_defaults`,
  `mom`, `constitution` at line 4 column 1
```

Your file is left exactly as it was — not rewritten, not "fixed", not touched.

Three promises hold for every settings change:

* **It is checked first.** If the result would not work, it is refused.
* **It happens all at once.** A change is never left half-written, even if the
  power goes out mid-save.
* **Two things changing settings at the same time take turns.** Neither one
  quietly wipes out the other's change.

Those promises used to cover only `regent config set`. They now cover every
command that changes settings — `regent setup`, `regent providers`, `regent
tools`, `regent agents mom`, `regent voice`, and the Settings screen in the
desktop app.

### Fixing a settings file that stops Regent starting

Two different things can go wrong, and they need different fixes. Regent tells
you which one you have.

**Something Regent does not recognise** — usually a typo. `unset` removes it:

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

**The file is scrambled** — a missing quote, a bad indent, something that is not
readable as a settings file at all. Nothing can fix that for you, so nothing
tries:

```
✗ config.yaml is not valid YAML and was left untouched: ...
  it has to be fixed by hand first
```

Open it in an editor and fix the line it names. Regent will never overwrite a
file it cannot read — which means a broken settings file is always still there
to be repaired, never silently replaced with a blank one.

All of this works **with Regent stopped**. That is the point: the moment you
need to fix settings is usually the moment Regent will not start.

---

## 4. Is it working?

```bash
regent doctor      # is the install healthy?
regent security    # what is this session allowed to do?
```

`doctor` checks the install and sorts what it finds into three buckets: fine,
worth knowing about, and actually broken. Only the last kind counts as a
failure — a health check that panics about everything is one people stop
running. For CI, `regent doctor --strict` fails on the middle bucket too, and
`--json` gives a machine-readable version. `regent security --json` likewise.

`security` lists what Regent can currently do and where each answer came from:

```bash
$ regent security
regent security
  ✓ approvals                  prompted
    sensitive actions ask first · from default
  ✓ REGENT_SANDBOX             off
    shell runs on the host · from default
  ✓ http listener              off
    no HTTP agent listener · from default
  …

audit
  ✓ REGENT_HOME          /home/you/.regent
  ! provider key         not set — prompt.submit will fail until exported
  ✓ config secrets       none in config.yaml (secrets stay in .env)

no issues found
```

`✓` fine · `!` worth a look · `✗` you have opened something up.

It judges things in context rather than by a checklist. A web listener that only
your own machine can reach, with a password on it, is not the same finding as
one open to the whole network with no password — and it says so in those terms:

```
✗ http listener              on · bind=0.0.0.0:8080 · token=MISSING
  listener is ON with NO token — anything that can reach it can drive the agent
```

**You do not have to remember to run this.** When a session is less guarded than
normal, the status bar says so the whole time you are using it —
`⚠ auto-approve · sandbox off` — because a safety setting nobody can see is a
safety setting nobody can rely on.

---

## 5. Typing in the chat window

| Key | What it does |
|---|---|
| `enter` | send |
| `alt+enter` or `shift+enter` | start a new line instead of sending |
| `↑ ↓` | move between lines; on a single line, brings back what you typed before |
| `← →` | move one character |
| `ctrl+← →` | move one word |
| `home` / `end` | start / end of the line |
| `ctrl+w` | delete the word behind the cursor |
| `ctrl+u` / `ctrl+k` | delete to the start / end of the line |
| `/` | pick a command from a list — `↑↓` to choose, `⇥` to fill in, `↵` to run |
| `?` | show this list (press it on an empty line) |
| `ctrl+c` | stop what Regent is doing · press twice to quit |

**Pasting works properly.** A stack trace, a diff, a chunk of log — paste it and
it arrives as one message. The line breaks inside it stay line breaks instead of
each one sending the message early.

`alt+enter` works essentially everywhere. `shift+enter` only works in terminals
that bother to send it. Both are wired up, so use whichever your terminal gives
you.

---

## 6. Tab completion

Let your shell finish command names for you:

```bash
regent completions bash > /etc/bash_completion.d/regent
regent completions zsh  > ~/.zsh/completions/_regent
regent completions fish > ~/.config/fish/completions/regent.fish
regent completions powershell | Out-String | Invoke-Expression
```

These are built from the same list `regent help` prints, so they can never
suggest a command that does not exist. They are deliberately simple and do not
contact Regent at all — a tab-completion that pauses while a background program
starts up is worse than none.

---

# For scripts and automation

Everything past this point is for people wiring Regent into something else. If
you are just using it, you are done.

## 7. Making `regent ask` machine-readable

Three output shapes, one flag each:

**Default** — the answer streams to *stdout*, progress notes to *stderr*. That
split is the contract: capturing stdout gives you the answer with nothing mixed
in.

**`--json`** — one JSON document, printed only when the run is over, so you can
never parse half of it:

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

**`--events`** — a live stream, one JSON object per line, stdout only:

```
{"type":"run.started","schema_version":1,"session_id":"sess_11e8…"}
{"type":"turn.started","schema_version":1,"session_id":"sess_11e8…"}
{"type":"turn.usage","schema_version":1,"input_tokens":11595,"output_tokens":34,…}
{"type":"message.complete","schema_version":1,"reply":"EVENTS OK",…}
{"type":"turn.complete","schema_version":1,…}
{"type":"run.completed","schema_version":1,"status":"completed","answer_complete":true,…}
```

Asking for both is an error rather than a guess: `--json` is one document,
`--events` is a stream of them, and they cannot both own stdout.

`--session <id>` targets a specific conversation, the scripted counterpart of
`-c`.

## 8. Exit codes

| Code | Means |
|---|---|
| `0` | finished |
| `1` | something failed while running |
| `2` | you called it wrong — bad flag, no question, question given twice |
| `3` | a limit stopped it, including `--timeout` |
| `4` | something it needs is not there (no deacon, session would not open) |
| `130` | interrupted |

Two rules worth building on:

**A run that never properly finishes exits `1`**, even if it printed part of an
answer. A dropped connection is not a success.

**A refused action is not a failure.** A run that declines to delete something
and still answers your question exits `0`. If you need to know whether anything
was refused, read `denied_approvals` and `answer_complete` in the final event —
do not try to infer it from the exit code.

## 9. Driving settings from a script

The background program has its own settings commands that need nothing else
running:

```bash
regent-deacon config describe   # every setting: type, default, current value, where it came from
regent-deacon config validate
regent-deacon config set model.default '"claude-opus-5"'   # values are JSON
regent-deacon config unset model.defalut

# Several at once — all of them or none:
regent-deacon config set model.provider '"ollama"' model.default '"llama3.2"'
```

`set` takes any number of key/value pairs. Passing them together is not just
faster — if the last one is refused, none of the earlier ones are applied
either. That is what stops a half-finished `regent setup`.

`describe` reports a `file` field: `ok`, `missing`, or `malformed`. Check it.
When the settings file cannot be read, `describe` still lists every setting so
you can see what is available — but those are the *defaults*, not what the user
configured, and treating them as current values is how a script overwrites
someone's settings with blanks.

`describe` also carries `descriptor_version`, so a script can confirm the shape
before trusting the field names.

---

# For developers

## 10. Testing a build

```bash
# The command-line program — 188 tests
cd src/regent-cli
bun install
bunx tsc --noEmit          # types
bunx biome check src       # lint + format
bun run compile            # MUST come before `bun test` — see below
bun test

# The engine
cd ../..
cargo fmt --all -- --check
cargo clippy --workspace --exclude regent-voice-server --all-targets
cargo test --workspace --exclude regent-voice-server
```

**Compile before testing.** The `*.int.test.ts` suites run the compiled program
as a black box and skip themselves when it is not there. Compiling afterwards
meant they had never actually run in CI.

Some suites also need the engine built (`cargo build -p regent-deacon`), because
they check that settings are written through it. CI builds it on the Linux run
and sets `REGENT_CI_EXPECT_DEACON=1`, which makes those tests **fail** instead of
skipping when the binary is missing. CI runs the CLI job on both Ubuntu and
Windows.

### The suites worth knowing about

| Suite | What it protects |
|---|---|
| `src/regent-cli/src/app/cli/cli.int.test.ts` | the shell contract of the **compiled** program — exit codes, keeping the answer separate from progress notes, the no-terminal guard |
| `src/regent-cli/src/app/config/commandSpec.test.ts` | reads the router's own source: a command in one list and not the other fails CI |
| `src/regent-cli/src/features/chat/domain/composer.test.ts` | pasting, key handling, cursor movement — no terminal needed |
| `src/regent-cli/src/features/ask/domain/askRun.test.ts` | the headless contract: exit codes and the final event |
| `src/regent-cli/src/features/setup/cli/setupCommand.int.test.ts` | onboarding against a throwaway home, including that a scrambled settings file is reported rather than replaced |
| `src/crates/regent-deacon/src/infra/tests/config_offline.rs` | offline validation, locking, all-at-once writes, keeping secrets out of output |
| `src/crates/regent-tools/tests/approval_coverage.rs` | every tool has a recorded permission stance; a refused command really does not run |

### Checking it by hand

Work in a throwaway home so your real one is never touched:

```bash
export REGENT_HOME=$(mktemp -d)
touch "$REGENT_HOME/.setup-done"     # skip the onboarding wizard
```

```bash
# 1. Shell contract
regent --version;            echo "expect 0: $?"
regent nosuchcmd;            echo "expect 2: $?"     # error goes to stderr
regent --nosuchopt;          echo "expect 2: $?"     # "unknown option", not "unknown command"
regent --profile --help;     echo "expect 2: $?"     # not a profile named --help
regent nosuchcmd 2>/dev/null # stdout must be EMPTY

# 2. No-terminal guard (must not hang)
echo hi | regent;            echo "expect 2: $?"
regent < /dev/null;          echo "expect 2: $?"

# 3. A typo cannot get written
printf '_config_version: 2\nmodel:\n  default: claude-sonnet-4-6\n' > "$REGENT_HOME/config.yaml"
regent config set moddel.default x;  echo "expect 2: $?"
cat "$REGENT_HOME/config.yaml"       # unchanged

printf '_config_version: 2\nmodel:\n  defalut: typo\n' > "$REGENT_HOME/config.yaml"
regent config validate;              echo "expect 1: $?"
regent config unset model.defalut;   echo "expect 0: $?"
regent config validate;              echo "expect 0: $?"

# 4. A scrambled file is never rewritten — by ANY command
printf 'model:\n  default: "unclosed\n  bad: [\n' > "$REGENT_HOME/config.yaml"
before=$(md5sum "$REGENT_HOME/config.yaml")
regent config set model.default x;   echo "expect 1: $?"
regent tools disable weather;        echo "expect 1: $?"
regent setup --provider ollama --model m; echo "expect 1: $?"
[ "$before" = "$(md5sum "$REGENT_HOME/config.yaml")" ] && echo "file untouched ✓"

# 5. Help, completions, posture
regent code --help                   # usage, flags, examples — not one line
regent completions bash | bash -n - && echo "bash syntax ✓"
regent config list --all | wc -l     # every setting there is (52 today)
regent security;                     echo "0 clean / 1 has an ✗: $?"

# 6. Headless (needs a working provider key)
unset REGENT_HOME
regent ask "Reply with exactly: OK"
regent ask "Reply with exactly: OK" --json
echo "Reply with exactly: OK" | regent ask 2>/dev/null
regent ask "Reply with exactly: OK" --events
```

On Windows PowerShell, use `$LASTEXITCODE` instead of `$?` and `Get-FileHash`
instead of `md5sum`.

## 11. Measured numbers

Windows 11, Bun 1.3.14, Windows Defender on, 20 runs each, warm:

| | |
|---|---|
| `regent --version` | ~257 ms |
| `regent help` | ~254 ms |
| `regent completions bash` | ~252 ms |
| compiled program | 95 MB |

None of these start the engine. Treat them as a trend line on one machine, not a
target to hit: a 95 MB binary is very sensitive to antivirus and code-signing,
and shared CI machines are not a stable place to measure.

---

## 12. What is not finished

Written down so nobody finds out the hard way.

* **There is no read-only mode.** As in §2: leaving `--yes` off does not stop
  Regent writing files. Making that real needs permission rules the approval
  check does not have yet.
* **`ask --max-turns`, `--max-tool-calls` and `--policy` do not exist.** They
  need "turn" and "tool call" pinned down first, and a budget model to spend
  against.
* **Stopping a run may leave things behind.** Regent stops its own engine, but a
  container or compiler that engine started can outlive it. Proper process-tree
  ownership is needed on both platforms.
* **`run_id` identifies the command you typed, not one turn of conversation.**
  Two `--session`/`-c` runs against the same conversation are not isolated from
  each other: cancelling one cancels whatever that conversation is currently
  doing, no matter who started it. Each run does get its own engine process, so
  the connection and its messages are separate — the stored conversation is not.
* **`config edit`, `config restore` and a self-updating `regent update` are
  deliberately not built.** See the plan's refusal list for why.
