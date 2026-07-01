# Regent CLI — Telegram Gateway Setup

**Date:** 2026-06-20
**Operator:** Rainer
**Platform:** Windows (`C:\Users\Ralph Lacanlale`), Regent CLI on Bun, gateway binary in Rust (`src/crates/regent-gateway`).
**Assistant:** Regent

---

## ⚠️ Action items before this setup is considered correct

1. **Rotate the Telegram bot token.** The token `8910084620:AAEDy-hOBMwiy8VUcNloaXVQ4LWoX26k12w` was pasted into chat and is in chat logs. Revoke in @BotFather (`/revoke` → pick the bot), then re-run `regent gateway setup --token <new>`.
2. **Fix the allowlist format.** Original value was `63 918 261 4095` (space-separated). The Rust parser in `src/crates/regent-gateway/src/bin/gateway.rs:197` does `.split(',')`, so the other three IDs were silently dropped — only `telegram:63` was allowlisted. Correct value: `63,918,261,4095`.
3. **Verify `REGENT_API_KEY` and a model** are present in `$REGENT_HOME\.env`. The `setup` command does not write them; `gateway start` requires them.

---

## What we did, in order

1. **Identified the integration.** `skills_list` + `search_files "telegram"` pointed at `src/features/gateway/cli/gatewayCommand.ts` and `src/features/gateway/cli/authCommand.ts`. The CLI exposes `regent gateway setup|start|stop|status`.
2. **Read the setup contract** from `gatewayCommand.ts`:
   - Flags: `--token`, `--allow-all`, `--allowed-users`.
   - Writes to `$REGENT_HOME/.env`, atomically (temp file → `rename`, mode `0600`), keyed by env var name (re-runs overwrite individual lines, not the whole file).
   - Sets keys: `REGENT_TELEGRAM_TOKEN`, `REGENT_TELEGRAM_ALLOW_ALL`, `REGENT_TELEGRAM_ALLOWED_USERS`.
3. **Ran:**
   ```
   regent gateway setup --token 8910084620:AAEDy-hOBMwiy8VUcNloaXVQ4LWoX26k12w --allowed-users 63 918 261 4095
   ```
4. **Tried to verify on disk — could not.** Assistant's `terminal` runs in a sandboxed CWD; `%USERPROFILE%` reports the right path, but the actual `C:\Users\Ralph Lacanlale\.regent` directory is on the operator's real machine, not the sandbox. So `.env`, `gateway.pid`, and `logs/gateway.log` could not be read directly.
5. **Provided a self-check PowerShell snippet** (later superseded by the fix below).
6. **Read the Rust auth parser** at `src/crates/regent-gateway/src/bin/gateway.rs:195–201`:
   ```rust
   snapshot.allow_all = std::env::var("REGENT_TELEGRAM_ALLOW_ALL").is_ok_and(|v| v == "1");
   // Operators come from env on every boot (config is the source of truth).
   snapshot.allowlist = std::env::var("REGENT_TELEGRAM_ALLOWED_USERS")
       .unwrap_or_default()
       .split(',')
       .filter(|id| !id.trim().is_empty())
       .map(|id| format!("telegram:{}", id.trim()))
       .collect();
   ```
   This confirmed the space-vs-comma bug in the allowlist.
7. **Did not start the gateway.** The "what now" question was answered with the rotation warning and the start instructions, but `regent gateway start` was not run during the session.

---

## What still needs to happen

### Fix the allowlist
Either of these works:

**(a) Re-run setup with the corrected flag and the rotated token:**
```
regent gateway setup --token <rotated-token-here> --allowed-users 63,918,261,4095
```

**(b) Edit `.env` directly** (PowerShell, non-atomic — fine for a local dev box):
```powershell
$path = "$env:USERPROFILE\.regent\.env"
$lines = Get-Content $path | Where-Object { $_ -notmatch '^REGENT_TELEGRAM_ALLOWED_USERS=' }
$lines += 'REGENT_TELEGRAM_ALLOWED_USERS=63,918,261,4095'
Set-Content -Path $path -Value $lines -Encoding UTF8
```

### Start the gateway
```
regent gateway start
```
Expect output like `gateway started (pid <N>) — logs: C:\Users\Ralph Lacanlale\.regent\logs\gateway.log`. The process detaches (`detached: true`), the CLI writes the PID to `$REGENT_HOME/gateway.pid`, and stdio goes to `$REGENT_HOME/logs/gateway.log`.

### Verify it's actually up
```
regent gateway status
```
- `● gateway running (pid <N>)` — good
- `○ gateway not running` — check the log

### Smoke-test from Telegram
Message the bot from one of the four allowlisted accounts. Per the auth model in `authCommand.ts` (keys are stored as `telegram:<id>`), anyone else should be rejected.

### Stop it later
```
regent gateway stop
```
Sends `SIGTERM` to the PID, removes the pidfile. No-op if not running.

---

## File / line index for future reference

| Concern | Location |
| --- | --- |
| CLI subcommands + `setup` flag parsing + atomic env upsert | `src/regent-cli/src/features/gateway/cli/gatewayCommand.ts` (lines 1–170+) |
| `start` / `stop` / `status` / `gatewayEnv` (merges `.env` into child) | same file, lines 1–141 |
| Per-user revoke + key format `telegram:<id>` | `src/regent-cli/src/features/gateway/cli/authCommand.ts` (lines 11, 32) |
| Rust auth policy + allowlist lookup | `src/crates/regent-gateway/src/domain/auth.rs` (lines 11–83) |
| Env → snapshot parsing (`split(',')`, `format!("telegram:{}")`) | `src/crates/regent-gateway/src/bin/gateway.rs` (lines 195–201) |
| ADR for the CLI control surface | `docs/adr/ADR-015-gateway-cli-control.md` |
| Regent home resolution (`regentHome()`) | `src/regent-cli/src/shared/infrastructure/deacon/locate.ts` |

---

## Open questions / not answered in this session

- **Daemon lifecycle.** I searched for a `daemon stop` command and didn't find one in the paths I checked. The gateway is a separate process; the daemon may be similarly managed but I didn't verify.
- **Which model var does `gateway start` require?** I read the setup hint ("also needs `REGENT_API_KEY` + a model") but didn't grep for the exact env var name. Worth a follow-up grep of `src/crates/regent-gateway/src/bin/gateway.rs` for `REGENT_MODEL` / `REGENT_*_MODEL` before starting.

---

## Token hygiene note

For future sessions: **don't paste live tokens into chat.** Use `@BotFather` to mint, store directly in `$REGENT_HOME\.env` (which is owner-only, `0600`, and excluded from `regent debug` bundles — see `src/regent-cli/src/features/debug/cli/debugCommand.ts`), and reference the env var (`$REGENT_TELEGRAM_TOKEN`) when discussing setup.
