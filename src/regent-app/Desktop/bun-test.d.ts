// Types for the `bun:test` runner used by this app's *.test.ts files.
//
// regent-cli gets these from the `@types/bun` devDependency; this package has
// no bun dependency of its own (it ships through Vite/Tauri), and pulling the
// whole Bun runtime typing in just to name four test helpers is more than the
// job needs. Declaring the surface we actually use keeps the test files
// type-checked — before this they were excluded from tsconfig entirely, so
// nothing checked them and editors flagged every import as unresolved.
declare module 'bun:test' {
  interface Matchers {
    toBe(expected: unknown): void;
    toEqual(expected: unknown): void;
    toMatchObject(expected: object): void;
    toContain(expected: unknown): void;
    toMatch(expected: RegExp | string): void;
    toHaveLength(expected: number): void;
    toBeNull(): void;
    toBeUndefined(): void;
    toBeGreaterThan(expected: number): void;
    toBeGreaterThanOrEqual(expected: number): void;
    toBeLessThan(expected: number): void;
    toBeLessThanOrEqual(expected: number): void;
    toBeCloseTo(expected: number, precision?: number): void;
  }

  /** `.not` inverts every matcher, so it exposes the same shape. */
  interface Expectation extends Matchers {
    readonly not: Matchers;
  }

  export function expect(actual: unknown): Expectation;
  export function describe(label: string, body: () => void): void;
  export function test(label: string, body: () => void | Promise<void>): void;
  export function beforeEach(body: () => void | Promise<void>): void;
  export function afterEach(body: () => void | Promise<void>): void;
  export function beforeAll(body: () => void | Promise<void>): void;
  export function afterAll(body: () => void | Promise<void>): void;
}
