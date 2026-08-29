/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const taskId = '11111111-1111-4111-8111-111111111111';
const mocks = vi.hoisted(() => ({
  blockName: 'task',
  progressStats: undefined as { completed: number; total: number } | undefined,
  providerTaskIds: [] as Array<() => readonly string[]>,
  dependencyProviderTaskIds: [] as Array<() => readonly string[]>,
}));

vi.mock('@core/block', () => ({
  useBlockAliasedName: () => mocks.blockName,
  useBlockId: () => taskId,
}));
vi.mock('@core/component/LexicalMarkdown/component/status/Progress', () => ({
  ProgressChip: () => <span>Markdown checklist progress</span>,
}));
vi.mock('@core/signal/permissions', () => ({ useCanEdit: () => () => false }));
vi.mock('@core/util/currentBlockDocumentName', () => ({
  useBlockDocumentName: () => () => 'Task title',
}));
vi.mock('@property/component/modal', () => ({ Modals: () => null }));
vi.mock('@property/constants', () => ({
  SYSTEM_PROPERTY_IDS: {
    ASSIGNEES: 'assignees',
    PRIORITY: 'priority',
    STATUS: 'status',
  },
}));
vi.mock('@property/context/PropertiesContext', () => ({
  PropertiesProvider: (props: { children: unknown }) => props.children,
}));
vi.mock('@property/hooks', () => ({
  useEntityProperties: () => ({ properties: () => [], refetch: vi.fn() }),
}));
vi.mock('@property/tags', () => ({ InlineFetchedEntityTagsPill: () => null }));
vi.mock('@queries/properties/entity', () => ({
  useBulkSaveEntityPropertiesMutation: () => ({ mutateAsync: vi.fn() }),
}));
vi.mock('@property/task-subtask-progress', () => ({
  TaskSubtaskProgressProvider: (props: {
    children: unknown;
    taskIds: () => readonly string[];
  }) => {
    mocks.providerTaskIds.push(props.taskIds);
    return props.children;
  },
  TaskSubtaskProgressIndicator: (props: { taskId: string }) => (
    <span data-subtask-progress-for={props.taskId}>Subtask progress</span>
  ),
}));
vi.mock('@property/task-dependency-relations', () => ({
  TaskDependencyRelationsProvider: (props: {
    children: unknown;
    taskIds: () => readonly string[];
  }) => {
    mocks.dependencyProviderTaskIds.push(props.taskIds);
    return props.children;
  },
  TaskDependencyRelations: (props: { taskId: string }) => (
    <span data-dependency-relations-for={props.taskId}>Task relations</span>
  ),
}));
vi.mock('../signal/markdownBlockData', () => ({
  mdStore: {
    get: {
      get progressStats() {
        return mocks.progressStats;
      },
    },
  },
}));
vi.mock('./InlinePropertyValue', () => ({ InlinePropertyValue: () => null }));

import { InlineTaskProperties } from './InlineTaskProperties';

beforeEach(() => {
  mocks.blockName = 'task';
  mocks.progressStats = undefined;
  mocks.providerTaskIds = [];
  mocks.dependencyProviderTaskIds = [];
});

afterEach(cleanup);

describe('InlineTaskProperties', () => {
  it('owns the current task ID and renders the canonical subtask indicator', () => {
    const view = render(() => <InlineTaskProperties />);

    expect(mocks.providerTaskIds).toHaveLength(1);
    expect(mocks.providerTaskIds[0]?.()).toEqual([taskId]);
    expect(
      view.container.querySelector(`[data-subtask-progress-for="${taskId}"]`)
    ).toBeTruthy();
    expect(mocks.dependencyProviderTaskIds[0]?.()).toEqual([taskId]);
    expect(
      view.container.querySelector(
        `[data-dependency-relations-for="${taskId}"]`
      )
    ).toBeTruthy();
  });

  it('does not provide or render subtask progress for non-task markdown', () => {
    mocks.blockName = 'md';
    const view = render(() => <InlineTaskProperties />);

    expect(mocks.providerTaskIds).toEqual([]);
    expect(mocks.dependencyProviderTaskIds).toEqual([]);
    expect(view.queryByText('Subtask progress')).toBeNull();
    expect(view.queryByText('Task relations')).toBeNull();
  });

  it('keeps markdown checklist progress separate from backend subtask progress', () => {
    mocks.progressStats = { completed: 1, total: 2 };
    const view = render(() => <InlineTaskProperties />);

    expect(view.getByText('Markdown checklist progress')).toBeTruthy();
    expect(view.getByText('Subtask progress')).toBeTruthy();
  });
});
