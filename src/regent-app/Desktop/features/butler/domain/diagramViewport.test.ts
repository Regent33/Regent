import { describe, expect, test } from 'bun:test';
import {
  DIAGRAM_MAX_K,
  DIAGRAM_MIN_K,
  INITIAL_DIAGRAM_VIEW,
  panDiagram,
  zoomDiagramAt,
} from './diagramViewport';

describe('Butler diagram viewport', () => {
  test('zoom keeps the diagram point under the cursor fixed', () => {
    const view = { x: 30, y: -20, k: 1.25 };
    const focus = { x: 240, y: 110 };
    const diagramX = (focus.x - view.x) / view.k;
    const diagramY = (focus.y - view.y) / view.k;
    const next = zoomDiagramAt(view, focus.x, focus.y, 1.4);

    expect(diagramX * next.k + next.x).toBeCloseTo(focus.x, 6);
    expect(diagramY * next.k + next.y).toBeCloseTo(focus.y, 6);
  });

  test('zoom is bounded so the diagram remains recoverable', () => {
    expect(zoomDiagramAt(INITIAL_DIAGRAM_VIEW, 0, 0, 100).k).toBe(DIAGRAM_MAX_K);
    expect(zoomDiagramAt(INITIAL_DIAGRAM_VIEW, 0, 0, 0.001).k).toBe(DIAGRAM_MIN_K);
  });

  test('pan moves in screen pixels without changing zoom', () => {
    expect(panDiagram({ x: 5, y: 10, k: 2 }, -20, 15)).toEqual({ x: -15, y: 25, k: 2 });
  });
});
