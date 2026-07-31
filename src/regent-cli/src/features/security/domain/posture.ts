// The security posture, as data (plan C.3).
//
// Framing, stated honestly: this is OBSERVABILITY, not enforcement. Nothing here
// makes Regent safer — it makes what is already true visible, which matters
// because a control nobody can observe is a control nobody can rely on.
//
// Two rules the first draft of the plan got wrong and this encodes:
//   * "safe" is CONTEXTUAL. A loopback listener with a strong token is not the
//     same finding as an unauthenticated one bound to 0.0.0.0.
//   * booleans must parse the way the runtime parses them, not the way
//     JavaScript coerces them.

export type Status = "default" | "review" | "unsafe";

export interface Control {
  readonly control: string;
  /** Already redacted where it is a secret — presence, never the value. */
  readonly value: string;
  readonly origin: "env" | "config.yaml" | "default";
  readonly status: Status;
  readonly note: string;
}

/** Matches the Rust `env_flag` helper. `REGENT_SANDBOX=0` is not "on". */
export function envFlag(v: string | undefined): boolean {
  return v !== undefined && ["1", "true", "yes"].includes(v.trim().toLowerCase());
}

/** A value that is an actual command, not a redaction marker or an empty one. */
function isRealCommand(v: string): boolean {
  const t = v.trim();
  return t !== "" && t !== "<unset>" && t !== "<redacted>";
}

type Env = Record<string, string | undefined>;
/** Effective config values, keyed by dotted path — from the Rust descriptor. */
export type ConfigValues = Record<string, unknown>;

function envControl(
  env: Env,
  name: string,
  whenSet: Status,
  note: string,
  safeNote = "not set",
): Control {
  const on = envFlag(env[name]);
  return {
    control: name,
    value: on ? "on" : "off",
    origin: on ? "env" : "default",
    status: on ? whenSet : "default",
    note: on ? note : safeNote,
  };
}

/**
 * The HTTP listener, judged in context rather than by a single boolean: not
 * enabled is nothing; loopback with a token is a deliberate choice worth seeing;
 * a non-loopback bind or a missing token is the actual finding.
 */
function httpControl(cfg: ConfigValues): Control {
  const enabled = cfg["http.enabled"] === true;
  const bind = String(cfg["http.bind"] ?? "");
  const hasToken = cfg["http.token"] === "<set>";
  if (!enabled) {
    return {
      control: "http listener",
      value: "off",
      origin: "default",
      status: "default",
      note: "no HTTP agent listener",
    };
  }
  const loopback = /^(127\.|::1|localhost)/.test(bind) || bind === "";
  const status: Status = loopback && hasToken ? "review" : "unsafe";
  const note = !hasToken
    ? "listener is ON with NO token — anything that can reach it can drive the agent"
    : loopback
      ? "listener is on, loopback-bound and token-protected"
      : `listener is on and bound to ${bind} — reachable beyond this machine`;
  return {
    control: "http listener",
    value: `on · bind=${bind || "(default)"} · token=${hasToken ? "set" : "MISSING"}`,
    origin: "config.yaml",
    status,
    note,
  };
}

/** Every control, in reporting order. `cfg` comes from `regent-deacon config describe`. */
export function posture(cfg: ConfigValues, env: Env): Control[] {
  const autoConfig = cfg["tools.auto_approve"] === true;
  const autoEnv = envFlag(env.REGENT_AUTO_APPROVE);
  // Hook commands are redacted in the descriptor (they can carry a token in
  // their arguments), so the value is the marker "<set>" / "<unset>", never the
  // command. Testing for a non-empty string would call every install hooked,
  // because "<unset>" is a perfectly non-empty string.
  const hooks = ["tools.hook_tool_start", "tools.hook_tool_complete"].filter(
    (k) => cfg[k] === "<set>" || (typeof cfg[k] === "string" && isRealCommand(cfg[k] as string)),
  );
  const backend = env.REGENT_TERMINAL_BACKEND ?? "local";

  return [
    {
      control: "approvals",
      value: autoConfig || autoEnv ? "AUTO-APPROVED" : "prompted",
      origin: autoEnv ? "env" : autoConfig ? "config.yaml" : "default",
      status: autoConfig || autoEnv ? "unsafe" : "default",
      note:
        autoConfig || autoEnv
          ? "every tool runs without asking (ask_user still reaches you)"
          : "sensitive actions ask first",
    },
    envControl(
      env,
      "REGENT_UNSAFE_NO_SANDBOX",
      "unsafe",
      "the filesystem jail is widened",
      "filesystem jail in force",
    ),
    {
      control: "REGENT_SANDBOX",
      value: envFlag(env.REGENT_SANDBOX) ? "on" : "off",
      origin: envFlag(env.REGENT_SANDBOX) ? "env" : "default",
      status: "default",
      // Not a finding either way. The note names what the flag covers because
      // it once did NOT cover cron, board workers or `regent mcp serve` — those
      // built their catalog on a path that skipped enforcement (fixed
      // 2026-07-31; see docs/audits/approval-sandbox-boundary-2026-07-31.md §4).
      note: envFlag(env.REGENT_SANDBOX)
        ? "host `local` backend refused — sessions, cron, board workers and `regent mcp serve`"
        : "shell runs on the host",
    },
    {
      control: "REGENT_TERMINAL_BACKEND",
      value: backend,
      origin: env.REGENT_TERMINAL_BACKEND ? "env" : "default",
      status: "default",
      note: backend === "local" ? "commands run on this machine" : `commands run via ${backend}`,
    },
    httpControl(cfg),
    {
      control: "tool hooks",
      value: hooks.length > 0 ? hooks.join(", ") : "none",
      origin: hooks.length > 0 ? "config.yaml" : "default",
      status: hooks.length > 0 ? "review" : "default",
      note:
        hooks.length > 0
          ? "shell runs at every tool dispatch, outside the approval gate and before it"
          : "no shell runs around tool dispatch",
    },
    envControl(
      env,
      "REGENT_VOICE_FULL_CONTROL",
      "unsafe",
      "a voice call approves every action it asks for",
    ),
    envControl(
      env,
      "REGENT_COMPUTER_USE",
      "review",
      "desktop control is registered (mutating actions are still gated)",
      "desktop control not registered",
    ),
  ];
}

/** Worst status present — what the command's exit code is built from. */
export function worst(controls: readonly Control[]): Status {
  if (controls.some((c) => c.status === "unsafe")) return "unsafe";
  return controls.some((c) => c.status === "review") ? "review" : "default";
}
