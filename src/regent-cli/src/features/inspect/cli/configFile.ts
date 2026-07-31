// CLI-string → config-value coercion. This is ALL that is left of the CLI's own
// config handling: reading, locking, validating and writing config.yaml now live
// in Rust (`regent-deacon config …` / the `config.set` RPC), so there is exactly
// one implementation and it is the one that owns the schema.

/** What a coerced CLI argument can become on its way into config.yaml. */
export type ConfigValue = string | number | boolean | Array<string | number | boolean>;

/** Coerce a CLI string to a YAML value: booleans, plain numbers and JSON arrays
 * get typed; everything else stays a string.
 *
 * Arrays are here because several config keys are lists — `tools.deferred`,
 * `tools.pinned`, `providers.<name>.models`. Without this, `config set
 * tools.deferred '["a","b"]'` stored the BRACKETS AS TEXT: config.yaml came out
 * holding `deferred: '["a","b"]'`, a single string, and tool deferral silently
 * stopped matching anything. It was worse than a visible failure because the
 * write reported success. */
export function coerce(value: string): ConfigValue {
  if (value === "true") return true;
  if (value === "false") return false;
  if (/^-?\d+(\.\d+)?$/.test(value)) return Number(value);
  // Only a well-formed JSON array of scalars. A malformed one falls through to
  // string rather than throwing, so a value that merely LOOKS bracketed (a
  // prompt fragment, a glob) is still settable.
  if (value.startsWith("[") && value.endsWith("]")) {
    try {
      const parsed: unknown = JSON.parse(value);
      if (
        Array.isArray(parsed) &&
        parsed.every((item) => item !== null && typeof item !== "object")
      ) {
        return parsed as Array<string | number | boolean>;
      }
    } catch {
      // not JSON — fall through and store it verbatim
    }
  }
  return value;
}
