import type { TaskDependencyRelationState } from '@property/task-dependency-relations';

export type TimelineDependencyEdge = {
  predecessorId: string;
  dependentId: string;
};

type Rect = Pick<DOMRect, 'bottom' | 'left' | 'right' | 'top'>;

function isFiniteRect(rect: Rect) {
  return [rect.top, rect.right, rect.bottom, rect.left].every(Number.isFinite);
}

/** Returns only complete, currently renderable predecessor connections. */
export function getProjectTimelineDependencyEdges(
  renderedTaskIds: readonly string[],
  relationsForTask:
    | ((taskId: string) => TaskDependencyRelationState | undefined)
    | undefined,
  visibleTaskIds: ReadonlySet<string>,
  edgeLimit: number
) {
  if (!relationsForTask || edgeLimit < 1) return [];

  const rendered = new Set(renderedTaskIds);
  const edges: TimelineDependencyEdge[] = [];
  const seen = new Set<string>();
  for (const dependentId of renderedTaskIds) {
    const state = relationsForTask(dependentId);
    if (
      state?.kind !== 'ready' ||
      state.relation.hasUnavailableDependencies ||
      !visibleTaskIds.has(dependentId)
    ) {
      continue;
    }
    for (const predecessorId of state.relation.dependsOnTaskIds) {
      if (!rendered.has(predecessorId) || !visibleTaskIds.has(predecessorId))
        continue;
      const key = `${predecessorId}\u0000${dependentId}`;
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push({ predecessorId, dependentId });
      if (edges.length > edgeLimit) return [];
    }
  }
  return edges;
}

/** Converts private in-memory marker references into clipped local SVG paths. */
export function getProjectTimelineDependencyPaths(
  edges: readonly TimelineDependencyEdge[],
  markers: ReadonlyMap<string, Element>,
  container: Element | undefined
) {
  if (!container) return [];
  const bounds = container.getBoundingClientRect();
  if (
    !isFiniteRect(bounds) ||
    bounds.right <= bounds.left ||
    bounds.bottom <= bounds.top
  )
    return [];

  const width = bounds.right - bounds.left;
  const height = bounds.bottom - bounds.top;
  const clipX = (value: number) => Math.max(0, Math.min(width, value));
  const clipY = (value: number) => Math.max(0, Math.min(height, value));
  const paths: string[] = [];
  for (const edge of edges) {
    const predecessor = markers
      .get(edge.predecessorId)
      ?.getBoundingClientRect();
    const dependent = markers.get(edge.dependentId)?.getBoundingClientRect();
    if (
      !predecessor ||
      !dependent ||
      !isFiniteRect(predecessor) ||
      !isFiniteRect(dependent)
    )
      continue;
    const startX = clipX(predecessor.right - bounds.left);
    const startY = clipY(
      (predecessor.top + predecessor.bottom) / 2 - bounds.top
    );
    const endX = clipX(dependent.left - bounds.left);
    const endY = clipY((dependent.top + dependent.bottom) / 2 - bounds.top);
    if (![startX, startY, endX, endY].every(Number.isFinite)) continue;
    const middleX = (startX + endX) / 2;
    paths.push(
      `M ${startX} ${startY} C ${middleX} ${startY}, ${middleX} ${endY}, ${endX} ${endY}`
    );
  }
  return paths;
}
