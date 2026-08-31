import type { TaskDependencyRelationState } from '@property/task-dependency-relations';
import { describe, expect, it } from 'vitest';
import {
  getProjectTimelineDependencyEdges,
  getProjectTimelineDependencyPaths,
} from './project-task-timeline-dependencies';

const ready = (
  dependsOnTaskIds: string[]
): Extract<TaskDependencyRelationState, { kind: 'ready' }> => ({
  kind: 'ready',
  relation: {
    taskId: 'private-task',
    readiness: 'ready',
    dependsOnTaskIds,
    blockingTaskIds: [],
    hasUnavailableDependencies: false,
    successorTaskIds: [],
    hasUnavailableSuccessors: false,
  },
});

describe('project timeline dependency helpers', () => {
  it('keeps rendered dependent order and server predecessor order while deduplicating pairs', () => {
    const states = new Map<string, TaskDependencyRelationState>([
      [
        'dependent-b',
        ready(['predecessor-b', 'predecessor-a', 'predecessor-b']),
      ],
      ['dependent-a', ready(['predecessor-a'])],
    ]);
    expect(
      getProjectTimelineDependencyEdges(
        ['predecessor-a', 'predecessor-b', 'dependent-b', 'dependent-a'],
        (id) => states.get(id),
        new Set([
          'predecessor-a',
          'predecessor-b',
          'dependent-a',
          'dependent-b',
        ]),
        500
      )
    ).toEqual([
      { predecessorId: 'predecessor-b', dependentId: 'dependent-b' },
      { predecessorId: 'predecessor-a', dependentId: 'dependent-b' },
      { predecessorId: 'predecessor-a', dependentId: 'dependent-a' },
    ]);
  });

  it('fails closed for unavailable or non-rendered endpoint state', () => {
    const unavailableDependencies = ready(['predecessor']);
    expect(
      getProjectTimelineDependencyEdges(
        ['predecessor', 'dependent'],
        () => ({
          ...unavailableDependencies,
          relation: {
            ...unavailableDependencies.relation,
            hasUnavailableDependencies: true,
          },
        }),
        new Set(['predecessor', 'dependent']),
        500
      )
    ).toEqual([]);
    expect(
      getProjectTimelineDependencyEdges(
        ['dependent'],
        () => ready(['predecessor']),
        new Set(['dependent']),
        500
      )
    ).toEqual([]);
  });

  it.each<TaskDependencyRelationState | undefined>([
    undefined,
    { kind: 'loading' },
    { kind: 'offline' },
    { kind: 'error' },
    { kind: 'unavailable' },
  ])('fails closed for every non-ready dependent state', (state) => {
    expect(
      getProjectTimelineDependencyEdges(
        ['predecessor', 'dependent'],
        () => state,
        new Set(['predecessor', 'dependent']),
        500
      )
    ).toEqual([]);
  });

  it('requires both visible markers even when both tasks are in the rendered window', () => {
    expect(
      getProjectTimelineDependencyEdges(
        ['predecessor', 'dependent'],
        (taskId) =>
          taskId === 'dependent' ? ready(['predecessor']) : ready([]),
        new Set(['dependent']),
        500
      )
    ).toEqual([]);
  });

  it('accepts exactly the edge limit and suppresses a denser complete graph', () => {
    const predecessorIds = Array.from(
      { length: 501 },
      (_, index) => `predecessor-${index}`
    );
    const renderedTaskIds = [...predecessorIds, 'dependent'];
    const visibleTaskIds = new Set(renderedTaskIds);
    const relationsForTask = (taskId: string) =>
      taskId === 'dependent' ? ready(predecessorIds) : ready([]);
    expect(
      getProjectTimelineDependencyEdges(
        renderedTaskIds,
        relationsForTask,
        visibleTaskIds,
        500
      )
    ).toHaveLength(0);
    expect(
      getProjectTimelineDependencyEdges(
        renderedTaskIds.slice(1),
        relationsForTask,
        new Set(renderedTaskIds.slice(1)),
        500
      )
    ).toHaveLength(500);
  });

  it('returns finite, clipped local paths without retaining task identifiers', () => {
    const rect = (left: number, top: number, right: number, bottom: number) =>
      ({ left, top, right, bottom }) as DOMRect;
    const container = {
      getBoundingClientRect: () => rect(10, 20, 110, 120),
    } as Element;
    const markers = new Map<string, Element>([
      [
        'predecessor',
        { getBoundingClientRect: () => rect(-20, 30, 30, 40) } as Element,
      ],
      [
        'dependent',
        { getBoundingClientRect: () => rect(90, 100, 140, 110) } as Element,
      ],
    ]);
    const paths = getProjectTimelineDependencyPaths(
      [{ predecessorId: 'predecessor', dependentId: 'dependent' }],
      markers,
      container
    );
    expect(paths).toHaveLength(1);
    expect(paths[0]).toMatch(/^M 20 15 C 50 15, 50 85, 80 85$/);
    expect(paths.join('')).not.toContain('predecessor');
    expect(paths.join('')).not.toContain('dependent');
  });
});
