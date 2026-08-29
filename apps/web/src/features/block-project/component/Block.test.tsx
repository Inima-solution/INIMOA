/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import type { Accessor, ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  dependencyProviderTaskIds: [] as string[][],
  dependencyProviderCount: 0,
  projectId: 'project-id',
  rows: [] as Array<{
    getIsGrouped: () => boolean;
    getIsLoadMore: () => boolean;
    original: { id: string; subType?: { type: string } | null; type: string };
  }>,
}));

vi.mock('@app/features/next-soup/actions', () => ({
  useBlockEntityCommands: () => {},
}));
vi.mock('@app/features/next-soup/create-soup-state', () => ({
  createSoupState: () => ({}),
}));
vi.mock('@app/features/next-soup/filters/filter-store', () => ({
  defineQueryFilters: (query: unknown) => query,
}));
vi.mock('@app/features/next-soup/filters/query-filters', () => ({
  soupItemMatchesProjectMembership: () => true,
}));
vi.mock('@app/features/next-soup/soup-context', () => ({
  SoupContextProvider: (props: ParentProps) => props.children,
}));
vi.mock('@app/features/next-soup/soup-view/soup-view', () => ({
  SoupViewList: () => <div data-testid="soup-view-list" />,
}));
vi.mock('@app/features/next-soup/soup-view/soup-view-context', () => ({
  SoupViewContextProvider: (props: ParentProps) => props.children,
  useSoupView: () => ({ rows: () => mocks.rows }),
}));
vi.mock('@block-project/isSpecial', () => ({
  getIsSpecialProject: (id: string) => id === 'special-project',
}));
vi.mock('@components/app/side-panel', () => ({
  SidePanel: { Layout: (props: ParentProps) => props.children },
}));
vi.mock('@core/block', () => ({ useBlockId: () => mocks.projectId }));
vi.mock('@core/component/DocumentBlockContainer', () => ({
  DocumentBlockContainer: (props: ParentProps) => props.children,
}));
vi.mock('@core/component/FileDropOverlay', () => ({
  FileDropOverlay: (props: ParentProps) => props.children,
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { failure: vi.fn() },
}));
vi.mock('@core/directive/fileFolderDrop', () => ({ fileFolderDrop: () => {} }));
vi.mock('@core/directive/fileSelector', () => ({ fileSelector: () => {} }));
vi.mock('@core/signal/blockElement', () => ({
  blockHotkeyScopeSignal: { get: () => 'scope-id' },
}));
vi.mock('@core/util/upload', () => ({
  handleFileFolderDrop: vi.fn(),
  uploadFiles: vi.fn(),
}));
vi.mock('@entity/types/entity', () => ({
  isTaskEntity: (entity: { subType?: { type: string } | null; type: string }) =>
    entity.type === 'document' && entity.subType?.type === 'task',
}));
vi.mock('@property/task-subtask-progress', () => ({
  TaskSubtaskProgressProvider: (props: ParentProps) => props.children,
}));
vi.mock('@property/task-dependency-relations', () => ({
  TaskDependencyRelationsProvider: (
    props: ParentProps<{
      taskIds: Accessor<readonly string[]>;
    }>
  ) => {
    mocks.dependencyProviderCount += 1;
    mocks.dependencyProviderTaskIds.push([...props.taskIds()]);
    return props.children;
  },
}));
vi.mock('@queries/soup/cache', () => ({ refetchSoupEntity: vi.fn() }));
vi.mock('@service-storage/util/refetchResources', () => ({
  refetchResources: vi.fn(),
}));
vi.mock('./ModalsProvider', () => ({
  ModalsProvider: (props: ParentProps) => props.children,
}));
vi.mock('./sidepanel/ProjectSidePanelSections', () => ({
  ProjectSidePanelSections: () => null,
}));
vi.mock('./TopBar', () => ({ TopBar: () => null }));

import Block from './Block';

afterEach(cleanup);

beforeEach(() => {
  mocks.dependencyProviderCount = 0;
  mocks.dependencyProviderTaskIds = [];
  mocks.projectId = 'project-id';
  mocks.rows = [];
});

describe('project task dependency relation batching', () => {
  it('uses one parent provider with only ungrouped task rows', () => {
    mocks.rows = [
      {
        getIsGrouped: () => false,
        getIsLoadMore: () => false,
        original: { id: 'task-a', subType: { type: 'task' }, type: 'document' },
      },
      {
        getIsGrouped: () => true,
        getIsLoadMore: () => false,
        original: {
          id: 'grouped-task',
          subType: { type: 'task' },
          type: 'document',
        },
      },
      {
        getIsGrouped: () => false,
        getIsLoadMore: () => true,
        original: {
          id: 'load-more-task',
          subType: { type: 'task' },
          type: 'document',
        },
      },
      {
        getIsGrouped: () => false,
        getIsLoadMore: () => false,
        original: { id: 'document-a', subType: null, type: 'document' },
      },
      {
        getIsGrouped: () => false,
        getIsLoadMore: () => false,
        original: { id: 'task-b', subType: { type: 'task' }, type: 'document' },
      },
    ];

    render(() => <Block />);

    expect(mocks.dependencyProviderCount).toBe(1);
    expect(mocks.dependencyProviderTaskIds).toEqual([['task-a', 'task-b']]);
  });

  it('keeps the special project provider empty', () => {
    mocks.projectId = 'special-project';
    mocks.rows = [
      {
        getIsGrouped: () => false,
        getIsLoadMore: () => false,
        original: { id: 'task-a', subType: { type: 'task' }, type: 'document' },
      },
    ];

    render(() => <Block />);

    expect(mocks.dependencyProviderCount).toBe(1);
    expect(mocks.dependencyProviderTaskIds).toEqual([[]]);
  });
});
