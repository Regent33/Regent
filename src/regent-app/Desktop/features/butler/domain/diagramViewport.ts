// Pure camera math for Butler's diagram stage. The diagram starts centred;
// x/y are screen-pixel offsets from that centre and k is its zoom scale.
// Keeping this DOM-free makes focal-point zoom behavior easy to prove.
export interface DiagramView {
  readonly x: number;
  readonly y: number;
  readonly k: number;
}

export const DIAGRAM_MIN_K = 0.45;
export const DIAGRAM_MAX_K = 4;
export const INITIAL_DIAGRAM_VIEW: DiagramView = { x: 0, y: 0, k: 1 };

export function clampDiagramScale(k: number): number {
  return Math.min(DIAGRAM_MAX_K, Math.max(DIAGRAM_MIN_K, k));
}

/** Zoom while keeping the diagram point beneath the focal screen coordinate
 * fixed. fx/fy are measured from the viewport centre. */
export function zoomDiagramAt(
  view: DiagramView,
  fx: number,
  fy: number,
  factor: number,
): DiagramView {
  const k = clampDiagramScale(view.k * factor);
  const ratio = k / view.k;
  return {
    k,
    x: fx - (fx - view.x) * ratio,
    y: fy - (fy - view.y) * ratio,
  };
}

export function panDiagram(view: DiagramView, dx: number, dy: number): DiagramView {
  return { ...view, x: view.x + dx, y: view.y + dy };
}
