import { describe, expect, test } from 'bun:test';
import { parseUnifiedDiff } from '@/features/workspace/domain/parseDiff';

const SAMPLE = `diff --git a/src/a.ts b/src/a.ts
index 1111111..2222222 100644
--- a/src/a.ts
+++ b/src/a.ts
@@ -1,3 +1,3 @@
 const keep = 1;
-const old = 2;
+const fresh = 2;
 const tail = 3;
diff --git a/README.md b/README.md
index 3333333..4444444 100644
--- a/README.md
+++ b/README.md
@@ -1 +1,2 @@
 # Title
+added line
`;

describe('parseUnifiedDiff', () => {
  test('splits into one entry per file, newest path wins', () => {
    const files = parseUnifiedDiff(SAMPLE);
    expect(files.map((f) => f.path)).toEqual(['src/a.ts', 'README.md']);
  });

  test('classifies each line as context, removed, or added', () => {
    const [first] = parseUnifiedDiff(SAMPLE);
    expect(first.lines.map((l) => l.kind)).toEqual([
      'hunk',
      'context',
      'removed',
      'added',
      'context',
    ]);
    // The leading +/- marker is stripped for display.
    expect(first.lines[2].text).toBe('const old = 2;');
    expect(first.lines[3].text).toBe('const fresh = 2;');
  });

  test('counts additions and deletions per file', () => {
    const [a, readme] = parseUnifiedDiff(SAMPLE);
    expect({ adds: a.adds, dels: a.dels }).toEqual({ adds: 1, dels: 1 });
    expect({ adds: readme.adds, dels: readme.dels }).toEqual({ adds: 1, dels: 0 });
  });

  test('file headers and index lines are not rendered as content', () => {
    const [first] = parseUnifiedDiff(SAMPLE);
    const texts = first.lines.map((l) => l.text);
    expect(texts.some((t) => t.startsWith('diff --git'))).toBe(false);
    expect(texts.some((t) => t.startsWith('index '))).toBe(false);
    expect(texts.some((t) => t.startsWith('--- ') || t.startsWith('+++ '))).toBe(false);
  });

  test('empty or whitespace input yields no files rather than throwing', () => {
    expect(parseUnifiedDiff('')).toEqual([]);
    expect(parseUnifiedDiff('   \n')).toEqual([]);
  });

  test('a new file with no a/ side still reports its path', () => {
    const added = `diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
`;
    const [file] = parseUnifiedDiff(added);
    expect(file.path).toBe('new.txt');
    expect(file.adds).toBe(1);
  });
});
