'use client';
// Knowledge Graph page — a galaxy of remembered facts. Composes the data
// fetch, force layout, canvas renderer, and detail panel. The selected node
// lives in the URL (`?node=`) so browser back/forward restores it; every
// selection pushes a new entry and re-centres the camera on that node.
import { useEffect, useMemo, useRef } from 'react';
import { useRouter, useSearchParams } from '@/shared/infrastructure/router/adapter';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { useGraphData, type GraphNode } from '@/features/graph/viewmodels/useGraphData';
import { useForceLayout } from '@/features/graph/viewmodels/useForceLayout';
import { GraphCanvas, type GraphCanvasHandle } from '@/features/graph/presentation/GraphCanvas';
import { NodePanel, type Neighbor } from '@/features/graph/presentation/NodePanel';

export function GraphView() {
  const s = t().graph;
  const { nodes, edges, loading, error } = useGraphData();
  const layoutRef = useForceLayout(nodes, edges);
  const router = useRouter();
  const selected = useSearchParams().get('node') ?? undefined;
  const canvasRef = useRef<GraphCanvasHandle>(null);

  const nodeById = useMemo(() => {
    const m = new Map<string, GraphNode>();
    for (const n of nodes) m.set(n.id, n);
    return m;
  }, [nodes]);

  const selectedNode = selected !== undefined ? nodeById.get(selected) : undefined;

  const neighbors = useMemo<readonly Neighbor[]>(() => {
    if (selectedNode === undefined) return [];
    const ids = new Set<string>();
    for (const e of edges) {
      if (e.src === selectedNode.id) ids.add(e.dst);
      else if (e.dst === selectedNode.id) ids.add(e.src);
    }
    return [...ids]
      .map((id) => nodeById.get(id))
      .filter((n): n is GraphNode => n !== undefined)
      .map((n) => ({ id: n.id, name: n.name, kind: n.kind }));
  }, [selectedNode, edges, nodeById]);

  const select = (id: string) => router.push(`/graph?node=${encodeURIComponent(id)}`);
  const close = () => router.push('/graph');

  // URL → camera: whenever the selection resolves to a real node, centre it.
  useEffect(() => {
    if (selectedNode !== undefined) canvasRef.current?.focusNode(selectedNode.id);
  }, [selectedNode]);

  return (
    <div className="flex h-full flex-col">
      <h1 className="shrink-0 px-4 pb-2 pt-4 text-lg font-semibold text-text-primary">{s.title}</h1>
      <div className="relative min-h-0 flex-1">
        {loading && (
          <div className="flex h-full items-center justify-center">
            <Loader />
          </div>
        )}
        {!loading && error !== undefined && (
          <ErrorState title={s.error} description={error} />
        )}
        {!loading && error === undefined && nodes.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
            <p className="text-sm font-semibold text-text-primary">{s.empty}</p>
            <p className="max-w-md text-sm text-text-secondary">{s.emptyHint}</p>
          </div>
        )}
        {!loading && error === undefined && nodes.length > 0 && (
          <>
            <GraphCanvas
              ref={canvasRef}
              layoutRef={layoutRef}
              edges={edges}
              selectedId={selected}
              onSelect={select}
              ariaLabel={s.canvasLabel}
            />
            {selectedNode !== undefined && (
              <NodePanel node={selectedNode} neighbors={neighbors} onSelect={select} onClose={close} />
            )}
          </>
        )}
      </div>
    </div>
  );
}
