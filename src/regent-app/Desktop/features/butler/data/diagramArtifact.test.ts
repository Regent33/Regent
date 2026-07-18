import { describe, expect, test } from 'bun:test';
import { extractDiagramArtifactPath, recoverDiagramArtifact } from './diagramArtifact';

describe('Butler diagram artifact recovery', () => {
  test('reduces Windows and slash paths to an artifact-relative path', () => {
    expect(
      extractDiagramArtifactPath(
        'saved at `C:\\Users\\Ralph\\.regent\\artifacts\\engine-explanation\\diagram.json`',
      ),
    ).toBe('engine-explanation/diagram.json');
    expect(extractDiagramArtifactPath('artifacts/topic/diagram.json')).toBe('topic/diagram.json');
    expect(extractDiagramArtifactPath('C:\\tmp\\diagram.json')).toBeNull();
  });

  test('loads and validates any supported PresentSpec through artifacts.get', async () => {
    const seen: Array<[string, Record<string, unknown>]> = [];
    const spec = await recoverDiagramArtifact(
      'Created .regent/artifacts/engine-explanation/diagram.json',
      async (method, params) => {
        seen.push([method, params]);
        return {
          ok: true,
          value: {
            text: JSON.stringify({
              type: 'cycle',
              title: 'Four-Stroke Engine',
              nodes: ['Intake', 'Compression', 'Power', 'Exhaust'],
            }),
          },
        };
      },
    );

    expect(seen).toEqual([['artifacts.get', { path: 'engine-explanation/diagram.json' }]]);
    expect(spec?.type).toBe('cycle');
  });

  test('fails closed for missing, unreadable, or invalid artifacts', async () => {
    expect(await recoverDiagramArtifact('No artifact here')).toBeNull();
    expect(
      await recoverDiagramArtifact('artifacts/bad/diagram.json', async () => ({
        ok: true,
        value: { text: '{"type":"unknown"}' },
      })),
    ).toBeNull();
  });
});
