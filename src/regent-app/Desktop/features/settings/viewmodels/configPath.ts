// Safe dotted-path get/set over the deacon config JSON. The deacon's
// config.set re-validates the WHOLE file on write (the real safe path); these
// helpers only read and edit the local optimistic copy. Prototype-pollution
// guarded, mirroring the same discipline as the Rust schema keys.
const UNSAFE = new Set(['__proto__', 'constructor', 'prototype']);

function parts(path: string): readonly string[] {
  const ps = path.split('.');
  for (const p of ps) {
    if (p === '' || UNSAFE.has(p)) throw new Error(`unsafe config path: ${path}`);
  }
  return ps;
}

/** Read the value at a dotted path, or undefined if any segment is missing. */
export function getPath(obj: unknown, path: string): unknown {
  let cur: unknown = obj;
  for (const p of parts(path)) {
    if (cur === null || typeof cur !== 'object') return undefined;
    if (!Object.prototype.hasOwnProperty.call(cur, p)) return undefined;
    cur = (cur as Record<string, unknown>)[p];
  }
  return cur;
}

function define(target: Record<string, unknown>, key: string, value: unknown): void {
  Object.defineProperty(target, key, {
    value,
    writable: true,
    enumerable: true,
    configurable: true,
  });
}

/** Return a clone with the value at a dotted path replaced (intermediate
 * objects created as needed). The input is never mutated. */
export function setPath(
  obj: Record<string, unknown>,
  path: string,
  value: unknown,
): Record<string, unknown> {
  const clone = structuredClone(obj);
  const ps = parts(path);
  let cur: Record<string, unknown> = clone;
  for (let i = 0; i < ps.length - 1; i += 1) {
    const p = ps[i];
    const existing = cur[p];
    if (existing === null || typeof existing !== 'object') {
      define(cur, p, {});
    }
    cur = cur[p] as Record<string, unknown>;
  }
  define(cur, ps[ps.length - 1], value);
  return clone;
}
