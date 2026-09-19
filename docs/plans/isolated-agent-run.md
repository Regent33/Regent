# Isolated agent run: the agent works beside you, and you always win

Status: plan written 2026-09-19 after the 2026-09-11 runaway-typing incident.
Phase 0 shipped the same day. Phases 1–2 are the implementation this plan
drives; Phase 3 is the long-term shape and is documented so nobody re-derives
the constraints.

## The problem, in one incident

`computer_use type` was given a 3,627-character document for Google Docs. The
user pressed Stop after 29 seconds. The turn was cancelled, but the PowerShell
child running `SendKeys.SendWait` was not: it kept typing into whatever window
held the keyboard, following the user from Brave to Regent's own chat box,
where every newline *submitted* a fragment as a new user message. The model
then answered those fragments and sent `ctrl+a` to the wrong window. Only a
power-off ended it.

Two things were wrong, and both are general:

1. **Cancellation did not reach the child.** A dropped tool future must kill
   whatever it spawned.
2. **Keystrokes go to the foreground window, and the foreground belongs to the
   human.** Any agent that injects input into the shared input queue is, by
   construction, typing "wherever the user is". The user's own focus change
   must stop the agent, not redirect it.

## What the field does (research, 2026-09-18)

Sources are in `agentic-computer-use-isolation-research-2026-09` (memory) with
URLs; the facts that shape this plan:

- **Nobody on Windows arbitrates simultaneous human + agent input.** The
  systems that let a user keep working (Claude Cowork background mode, Codex on
  macOS, Cua Driver) avoid the input queue instead: UI Automation patterns,
  `PostMessage`, CDP into a browser, or a separate session. Codex on Windows
  (2026-05) is foreground-takeover only.
- **Windows 11 "agent workspace"** (separate session + standard agent account,
  user can take over) is the right long-term answer, but in 2026-09 it is
  Insider-only and its third-party SDK (`@microsoft/mxc-sdk`) is non-interactive
  (no display). No API gives Regent its own desktop today.
- **This machine is Windows 11 Home**: no Windows Sandbox, no Hyper-V; loopback
  multi-session RDP needs RDPWrap (EULA/AV). A second desktop via
  `CreateDesktop`/`SwitchDesktop` takes the screen *from* the user. Virtual
  desktops share one input desktop. None of these give concurrency.
- **Browsers are the exception.** Chromium routes CDP `Input.*` to a page
  regardless of OS focus (Playwright: "bringing the page to front is not
  required"), as long as the page is the active tab of its window. Chrome 136+
  ignores `--remote-debugging-port` on the default profile, so the agent needs
  its own `--user-data-dir`.
- **"Human wins" is detectable without admin.** `WH_KEYBOARD_LL` /
  `WH_MOUSE_LL` hooks see `LLKHF_INJECTED` / `LLMHF_INJECTED` on everything
  `SendInput`/`keybd_event`/`SendKeys` produce; an unflagged event is a real
  person. `RegisterHotKey` gives a global emergency stop. Neither works while
  an elevated window or the secure desktop is up (UIPI) — accepted.
- Anthropic's reference `type` is 50-char chunks with a 12ms delay; its docs
  require a confirmation check *before each block*, since one turn can run many
  actions.

## Design

Three layers, cheapest first. Each is independently useful; each later layer
keeps the earlier ones.

### Phase 0 — cancellation reaches the child (shipped 2026-09-19)

- `kill_on_drop(true)` on every process `computer_use` spawns (PowerShell and
  cua-driver). Regression test: `dropping_a_running_action_kills_its_powershell_child`.
- `type` sends 40-char chunks and re-checks `GetForegroundWindow()` before each
  one; a change aborts with `typing stopped after N of M characters`.
- The temp script (it holds the typed text) is removed on the cancel path too.

### Phase 1 — the human is superior: pinned target, human-wins pause, e-stop

All in `regent-tools/src/infra/computer_use/`, Windows only, no new crates
beyond `windows-sys` (already in the lock file).

**1a. Pinned target.** `PowerShellBackend` remembers the HWND of the last
successful `focus_window`. Every mutating action (`click`, `type`, `key`) runs
a shared prelude:

- the foreground window's process name must not match `^regent` (the desktop
  app is `regent-desktop.exe` in dev, `Regent.exe` installed) — an agent never
  needs to act on its own UI, and this is exactly how the incident fed itself;
- if a target is pinned, the foreground must *be* that window; otherwise the
  action refuses with "the target window is not in front — the user switched
  away; call focus_window again or wait for them". With nothing pinned the
  current foreground is the target (today's behaviour, minus the self-typing).

`close_window` on the pinned window clears the pin.

**1b. Human-wins pause.** A single hook thread (`human.rs`, started lazily the
first time the computer-use tool is built) installs `WH_KEYBOARD_LL` and
`WH_MOUSE_LL` and records the time of the last *non-injected* key-down or
mouse-button/wheel event (mouse *moves* are ignored — brushing the touchpad is
not a takeover). While a mutating action runs, the backend races it against
"human input arrived": the action is dropped (child killed) and returns
`paused: the user is using the keyboard/mouse; wait for them to finish, then
re-check the screen`. The tool description tells the model what that means.

**1c. Emergency stop.** The same thread registers a global hotkey,
**Ctrl+Alt+Shift+Esc**. Firing it: (1) latches `halted`; (2) notifies the
deacon, which cancels every live turn (`SessionManager::interrupt_all`, factored
out of the shutdown path) — dropping in-flight tools kills their children;
(3) every later `computer_use` mutating action refuses with "emergency stop is
latched" until the user sends a new message, which re-arms (the next
`run_turn` clears the latch). Re-arm is tied to a deliberate user act, never to
the hotkey itself, so a panicked double-press cannot undo the stop.

If `RegisterHotKey` fails (another app owns the chord) the deacon logs a
warning at startup; nothing else changes.

**Not in Phase 1 (YAGNI until asked):** a tray "Stop everything" button in the
desktop app (Stop per session exists; the hotkey is global), a configurable
chord, macOS/Linux hooks.

### Phase 2 — the browser channel: work in parallel for real

The user's actual task was a browser task, and browsers are the one place
Windows Home allows true concurrency. A `browser` backend for `computer_use`
that never touches the OS input queue:

- **Launch**: Brave/Chrome/Edge (first found) with
  `--user-data-dir=%REGENT_HOME%\browser-profile --remote-debugging-port=0
  --new-window --window-size=1280,900`; read `DevToolsActivePort` for the port.
  The profile is Regent's own (Chrome 136 rule); the user signs into Google
  there once and it persists. The window is the agent's; the user keeps theirs.
- **Transport**: one WebSocket (`tokio-tungstenite`, already a workspace dep)
  to the page target; CDP request/response by id; `Target.activateTarget`
  before input so the page is its window's active tab.
- **Actions** (same `Action` enum, so the model's contract is unchanged):
  `screenshot` → `Page.captureScreenshot`; `click` → `Input.dispatchMouseEvent`
  (move, press, release); `type` → `Input.insertText` — the whole text lands
  atomically, no chunking, no foreground; `key` → `Input.dispatchKeyEvent`
  (rawKeyDown/keyUp with modifiers). A new `navigate {url}` action.
- **Selection**: `REGENT_COMPUTER_USE_BACKEND=browser`, or automatic when the
  model's target is a URL/tab (the tool description steers: "for web pages use
  the browser channel; it does not take the user's keyboard").
- Phase 1's e-stop and pause still apply (cancelling the turn drops the CDP
  future; the hook thread is host-side and channel-agnostic). "Human wins"
  pause is *not* applied to CDP input — that is the point: the user's typing
  elsewhere does not conflict with it.

### Phase 3 — a second session (long term)

The only true isolation is another interactive session. Options, in order of
plausibility for this user: a VM (VirtualBox/VMware — free, works on Home) that
Regent drives over the VM's own channel; Windows 365 for Agents; Microsoft's
agent workspace once MXC session isolation becomes interactive. Design rule so
this drops in later: the backend trait stays `ComputerBackend::act(&Action)`;
Phase 2 adds `navigate` and nothing else to the contract. The hook thread and
e-stop stay host-side regardless of channel — they are the only controls that
work on all of them.

## Testing

- Unit (all platforms): script generation for the prelude (pin present / absent,
  `^regent` refusal, brace balance); `interrupt_all` cancels every session's
  token; e-stop latch semantics (latched → refuse; re-arm on new turn).
- Unit (Windows): dropped action kills its child (exists); "human input
  arrives mid-action → action returns paused and the child is gone" using a
  simulated human event (the recorder is a plain atomic, so tests set it
  directly; the hook itself is not unit-testable).
- `#[ignore]` desktop test (exists): extend with "typing stops when another
  window comes forward" and "typing refuses when Regent's window is in front".
- Phase 2: CDP client against a real headless Chromium in an `#[ignore]` test
  (`--headless=new`), asserting `insertText` lands in a `<textarea>` via
  `Runtime.evaluate`.

## Rollout

Phase 1 ships behind the existing `REGENT_COMPUTER_USE=1` flag — nothing runs
for users who never enabled desktop control. Phase 2 is opt-in via the backend
variable until the desktop test above has run on two machines.
