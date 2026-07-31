// What the status line has to say out loud about how this session will behave.
//
// The plan's reasoning, and it is right: a control nobody can see is a control
// nobody can rely on. `regent security` reports the same things, but you have to
// remember to run it — whereas the status line is the one place a user is always
// looking. If approvals are off or the sandbox is disabled, it belongs there.

/** True for the value forms the Rust `env_flag` helper accepts. */
function envOn(value: string | undefined): boolean {
  return value !== undefined && ["1", "true", "yes"].includes(value.trim().toLowerCase());
}

/**
 * Short markers for anything that makes this session less guarded than default.
 * Empty is the normal, safe case — nothing is shown then, so the marker keeps
 * its meaning instead of becoming decoration.
 */
export function unattendedMarkers(
  config: unknown,
  env: Record<string, string | undefined>,
): string[] {
  const markers: string[] = [];
  const tools = (config as { tools?: { auto_approve?: unknown; hook_tool_start?: unknown } } | null)
    ?.tools;

  if (tools?.auto_approve === true || envOn(env.REGENT_AUTO_APPROVE)) {
    markers.push("auto-approve");
  }
  // The sandbox is opt-in, so "not sandboxed" is the default and not worth
  // saying. Explicitly DISABLING it after asking for it is worth saying.
  if (envOn(env.REGENT_UNSAFE_NO_SANDBOX)) markers.push("sandbox off");
  if (envOn(env.REGENT_VOICE_FULL_CONTROL)) markers.push("voice full-control");
  // Hooks run arbitrary shell at every tool dispatch, outside the approval gate
  // (docs/audits/approval-sandbox-boundary-2026-07-31.md §3.1). Nothing told the
  // user they were armed.
  if (typeof tools?.hook_tool_start === "string" && tools.hook_tool_start.trim() !== "") {
    markers.push("tool hooks");
  }
  return markers;
}
