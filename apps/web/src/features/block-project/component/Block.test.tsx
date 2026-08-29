/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import type { Accessor, ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  dependencyProviderTaskIds: [] as string[][],
  dependencyProviderCount: 0,
  boardProps: undefined as
    | {
        error?: boolean;
        loading?: boolean;
        onOpenTask: (task: { id: string }, event: MouseEvent) => void;
        onRetry?: () => void;
        searching?: boolean;
        tasks: Array<{ id: string }>;
      }
    | undefined,
  controllerSplit: false,
  duplicatePreview: false,
  entryStateCalls: [] as Array<{ key: string; options: { default: string } }>,
  focusSet: vi.fn(),
  openEntityInSplit: vi.fn(),
  projectId: 'project-id',
  source: [] as Array<{
    id: string;
    subType?: { type: string } | null;
    type: string;
  }>,
  sourceError: null as Error | null,
  sourceFetchNextPage: vi.fn(),
  sourceRefresh: vi.fn(() => Promise.resolve()),
  sourceFetching: false,
  sourceLoading: false,
  sourcePlaceholderData: false,
  searchText: '',
  topBarProps: undefined as
    | {
        mode: 'list' | 'board';
        onChange: (mode: 'list' | 'board') => void;
        selectorVisible: boolean;
      }
    | undefined,
  viewMode: 'list' as 'list' | 'board',
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
  useSoupView: () => ({
    rows: () => mocks.rows,
    searchText: () => mocks.searchText,
    source: {
      data: () => mocks.source,
      error: () => mocks.sourceError,
      fetchNextPage: mocks.sourceFetchNextPage,
      isFetching: () => mocks.sourceFetching,
      isLoading: () => mocks.sourceLoading,
      isPlaceholderData: () => mocks.sourcePlaceholderData,
      refresh: mocks.sourceRefresh,
    },
    soup: { focus: { set: mocks.focusSet } },
  }),
}));
vi.mock('@app/features/next-soup/utils', () => ({
  openEntityInSplitFromUnifiedList: mocks.openEntityInSplit,
  preventDuplicatePreviewEntityOpen: () => mocks.duplicatePreview,
}));
vi.mock('@block-project/isSpecial', () => ({
  getIsSpecialProject: (id: string) => id === 'special-project',
}));
vi.mock('@components/app/side-panel', () => ({
  SidePanel: { Layout: (props: ParentProps) => props.children },
}));
vi.mock('@components/app/split-layout/entry-state', async () => {
  const { createSignal } = await import('solid-js');
  return {
    useEntryState: (key: string, options: { default: string }) => {
      mocks.entryStateCalls.push({ key, options });
      return createSignal(mocks.viewMode);
    },
  };
});
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanelOrThrow: () => ({
    handle: { isControllerSplit: () => mocks.controllerSplit },
  }),
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
vi.mock('./ProjectTaskStatusBoard', () => ({
  ProjectTaskStatusBoard: (props: NonNullable<typeof mocks.boardProps>) => {
    mocks.boardProps = props;
    return <div data-testid="project-task-status-board" />;
  },
}));
vi.mock('./TopBar', () => ({
  TopBar: (props: NonNullable<typeof mocks.topBarProps>) => {
    mocks.topBarProps = props;
    return null;
  },
}));

import Block from './Block';

afterEach(cleanup);

beforeEach(() => {
  mocks.dependencyProviderCount = 0;
  mocks.dependencyProviderTaskIds = [];
  mocks.boardProps = undefined;
  mocks.controllerSplit = false;
  mocks.duplicatePreview = false;
  mocks.entryStateCalls = [];
  mocks.focusSet.mockReset();
  mocks.openEntityInSplit.mockReset();
  mocks.projectId = 'project-id';
  mocks.source = [];
  mocks.sourceError = null;
  mocks.sourceFetchNextPage.mockReset();
  mocks.sourceRefresh.mockReset();
  mocks.sourceRefresh.mockResolvedValue(undefined);
  mocks.sourceFetching = false;
  mocks.sourceLoading = false;
  mocks.sourcePlaceholderData = false;
  mocks.searchText = '';
  mocks.topBarProps = undefined;
  mocks.viewMode = 'list';
  mocks.rows = [];
});

describe('project task dependency relation batching', () => {
  it('defaults to the existing list with the project-local entry key', () => {
    render(() => <Block />);

    expect(mocks.entryStateCalls).toEqual([
      { key: 'project.taskViewMode', options: { default: 'list' } },
    ]);
    expect(mocks.topBarProps).toMatchObject({
      mode: 'list',
      selectorVisible: true,
    });
    expect(
      document.querySelector('[data-testid="soup-view-list"]')
    ).toBeTruthy();
    expect(mocks.boardProps).toBeUndefined();
  });

  it('switches an ordinary project to an ordered task-only board without paging', () => {
    mocks.source = [
      { id: 'task-b', subType: { type: 'task' }, type: 'document' },
      { id: 'document-a', subType: null, type: 'document' },
      { id: 'task-a', subType: { type: 'task' }, type: 'document' },
    ];
    mocks.sourceFetching = true;
    mocks.sourceError = new Error('source unavailable');
    mocks.searchText = 'follow up';

    render(() => <Block />);
    mocks.topBarProps?.onChange('board');

    expect(mocks.boardProps).toMatchObject({
      error: true,
      loading: false,
      searching: true,
      tasks: [{ id: 'task-b' }, { id: 'task-a' }],
    });
    expect(mocks.sourceFetchNextPage).not.toHaveBeenCalled();
    expect(document.querySelector('[data-testid="soup-view-list"]')).toBeNull();
  });

  it('keeps the board loading while its initial task source is empty', () => {
    mocks.viewMode = 'board';
    mocks.sourceLoading = true;
    mocks.searchText = '   ';

    render(() => <Block />);

    expect(mocks.boardProps).toMatchObject({
      loading: true,
      searching: false,
      tasks: [],
    });
  });

  it('threads an initial source error to the board and retries the source once', () => {
    mocks.viewMode = 'board';
    mocks.sourceError = new Error('source unavailable');

    render(() => <Block />);
    mocks.boardProps?.onRetry?.();

    expect(mocks.boardProps).toMatchObject({
      error: true,
      loading: false,
      tasks: [],
    });
    expect(document.querySelector('[data-testid="soup-view-list"]')).toBeNull();
    expect(mocks.sourceRefresh).toHaveBeenCalledTimes(1);
  });

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
    mocks.viewMode = 'board';
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
    expect(mocks.topBarProps).toMatchObject({
      mode: 'list',
      selectorVisible: false,
    });
    expect(
      document.querySelector('[data-testid="soup-view-list"]')
    ).toBeTruthy();
    expect(mocks.boardProps).toBeUndefined();
  });

  it('uses the canonical split path for board task opens', () => {
    mocks.source = [
      { id: 'task-a', subType: { type: 'task' }, type: 'document' },
    ];
    mocks.viewMode = 'board';
    const task = mocks.source[0];

    render(() => <Block />);

    mocks.boardProps?.onOpenTask(task, new MouseEvent('click'));
    mocks.boardProps?.onOpenTask(
      task,
      new MouseEvent('click', { shiftKey: true })
    );
    mocks.boardProps?.onOpenTask(
      task,
      new MouseEvent('click', { altKey: true })
    );

    expect(mocks.focusSet).toHaveBeenCalledTimes(3);
    expect(mocks.openEntityInSplit).toHaveBeenNthCalledWith(
      1,
      task,
      expect.objectContaining({
        openInNewSplit: false,
        replacePreview: false,
      })
    );
    expect(mocks.openEntityInSplit).toHaveBeenNthCalledWith(
      2,
      task,
      expect.objectContaining({
        openInNewSplit: true,
        replacePreview: false,
      })
    );
    expect(mocks.openEntityInSplit).toHaveBeenNthCalledWith(
      3,
      task,
      expect.objectContaining({
        openInNewSplit: false,
        replacePreview: true,
      })
    );
  });

  it('does not reopen a task already previewed by the controller split', () => {
    mocks.source = [
      { id: 'task-a', subType: { type: 'task' }, type: 'document' },
    ];
    mocks.viewMode = 'board';
    mocks.controllerSplit = true;
    mocks.duplicatePreview = true;

    render(() => <Block />);
    mocks.boardProps?.onOpenTask(mocks.source[0], new MouseEvent('click'));

    expect(mocks.focusSet).not.toHaveBeenCalled();
    expect(mocks.openEntityInSplit).not.toHaveBeenCalled();
  });
});
