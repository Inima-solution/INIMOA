/** @vitest-environment jsdom */

import type { UploadInput } from '@core/util/upload';
import { cleanup, render } from '@solidjs/testing-library';
import type { Accessor, ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  blockEntityResolver: undefined as
    | (() => Record<string, unknown> | undefined)
    | undefined,
  dependencyProviderTaskIds: [] as string[][],
  dependencyProviderCount: 0,
  decisionListProps: undefined as
    | { projectId: string; scopeId: string }
    | undefined,
  boardProps: undefined as
    | {
        activeStatusTaskId?: string;
        canEdit?: boolean;
        error?: boolean;
        fetching?: boolean;
        fetchingNextPage?: boolean;
        hasNextPage?: boolean;
        loading?: boolean;
        onLoadMore?: () => void;
        onMoveTaskStatus?: (task: { id: string }, statusId: string) => void;
        onOpenTask: (task: { id: string }, event: MouseEvent) => void;
        onRetry?: () => void;
        searching?: boolean;
        statusPending?: boolean;
        statusProperty?: { propertyDefinitionId: string };
        tasks: Array<{ id: string }>;
      }
    | undefined,
  timelineProps: undefined as
    | {
        error?: boolean;
        fetching?: boolean;
        fetchingNextPage?: boolean;
        hasNextPage?: boolean;
        loading?: boolean;
        onLoadMore?: () => void;
        onOpenTask: (task: { id: string }, event: MouseEvent) => void;
        onRetry?: () => void;
        searching?: boolean;
        projectStartDate?: string;
        projectTargetDate?: string;
        rangeUnavailable?: boolean;
        tasks: Array<{ id: string }>;
      }
    | undefined,
  canEdit: true,
  controllerSplit: false,
  duplicatePreview: false,
  entryStateCalls: [] as Array<{ key: string; options: { default: string } }>,
  fileFolderDropOptions: undefined as
    | {
        disabled?: boolean;
        onDrop?: (
          fileEntries: FileSystemFileEntry[],
          folderEntries: FileSystemDirectoryEntry[]
        ) => void;
      }
    | undefined,
  focusSet: vi.fn(),
  milestoneFilterProps: undefined as { viewOverride?: string } | undefined,
  milestoneToolbarItem: undefined as
    | { id: string; priority: number }
    | undefined,
  milestoneToolbarPortalCount: 0,
  soupViewProviderCount: 0,
  openEntityInSplit: vi.fn(),
  projectId: 'project-id',
  projectMetadata: { name: 'Project Atlas', userId: 'owner-id' },
  projectOverviewAccessors: [] as Array<Accessor<string | undefined>>,
  projectOverviewQuery: {
    data: undefined as
      | { operations: { startDate?: string; targetDate?: string } }
      | undefined,
    fetchStatus: 'idle',
    isError: false,
    isPending: false,
  },
  sidePanelQuery: undefined as unknown,
  source: [] as Array<{
    id: string;
    subType?: { type: string } | null;
    type: string;
  }>,
  sourceError: null as Error | null,
  sourceFetchNextPage: vi.fn(),
  sourceRefresh: vi.fn(() => Promise.resolve()),
  sourceFetching: false,
  sourceFetchingNextPage: false,
  sourceHasNextPage: false,
  sourceLoading: false,
  sourcePlaceholderData: false,
  statusMutation: vi.fn(),
  statusMutationCallbacks: undefined as { onSettled?: () => void } | undefined,
  statusMutationPending: false,
  statusProperties: [] as Array<Record<string, unknown>>,
  searchText: '',
  topBarProps: undefined as
    | {
        mode: 'list' | 'board' | 'timeline' | 'decisions';
        onChange: (mode: 'list' | 'board' | 'timeline' | 'decisions') => void;
        selectorVisible: boolean;
      }
    | undefined,
  viewMode: 'list' as 'list' | 'board' | 'timeline' | 'decisions',
  uploadCallback: undefined as
    | ((files: UploadInput[]) => Promise<void>)
    | undefined,
  rows: [] as Array<{
    getIsGrouped: () => boolean;
    getIsLoadMore: () => boolean;
    original: { id: string; subType?: { type: string } | null; type: string };
  }>,
}));

vi.mock('@app/features/next-soup/actions', () => ({
  useBlockEntityCommands: (
    resolver?: () => Record<string, unknown> | undefined
  ) => {
    mocks.blockEntityResolver = resolver;
  },
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
vi.mock(
  '@app/features/next-soup/soup-view/filters-bar/unified-filter-dropdown',
  () => ({
    UnifiedFilterDropdown: (props: { viewOverride?: string }) => {
      mocks.milestoneFilterProps = props;
      return <button data-testid="project-milestone-filter">Milestones</button>;
    },
  })
);
vi.mock('@components/app/split-layout/components/CollapsibleItem', () => ({
  CollapsibleToolbarItem: (props: {
    children: (isCollapsed: Accessor<boolean>) => unknown;
    id: string;
    priority: number;
  }) => {
    mocks.milestoneToolbarItem = { id: props.id, priority: props.priority };
    return <>{props.children(() => false)}</>;
  },
}));
vi.mock('@components/app/split-layout/components/SplitToolbar', () => ({
  SplitToolbarLeft: (props: ParentProps) => {
    mocks.milestoneToolbarPortalCount += 1;
    return <div data-testid="project-milestone-toolbar">{props.children}</div>;
  },
}));
vi.mock('@app/features/next-soup/soup-view/soup-view-context', () => ({
  SoupViewContextProvider: (props: ParentProps) => {
    mocks.soupViewProviderCount += 1;
    return props.children;
  },
  useSoupView: () => ({
    rows: () => mocks.rows,
    searchText: () => mocks.searchText,
    source: {
      data: () => mocks.source,
      error: () => mocks.sourceError,
      fetchNextPage: mocks.sourceFetchNextPage,
      hasNextPage: () => mocks.sourceHasNextPage,
      isFetching: () => mocks.sourceFetching,
      isFetchingNextPage: () => mocks.sourceFetchingNextPage,
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
vi.mock('../signal/projectBlockData', () => ({
  projectBlockDataSignal: () => ({ projectMetadata: mocks.projectMetadata }),
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
vi.mock('@core/signal/permissions', () => ({
  useCanEdit: () => () => mocks.canEdit,
}));
vi.mock('@core/component/DocumentBlockContainer', () => ({
  DocumentBlockContainer: (props: ParentProps) => props.children,
}));
vi.mock('@core/component/FileDropOverlay', () => ({
  FileDropOverlay: (props: ParentProps) => props.children,
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { failure: vi.fn() },
}));
vi.mock('@core/directive/fileFolderDrop', () => ({
  fileFolderDrop: (
    _element: HTMLElement,
    accessor: Accessor<typeof mocks.fileFolderDropOptions>
  ) => {
    mocks.fileFolderDropOptions = accessor();
  },
}));
vi.mock('@core/directive/fileSelector', () => ({ fileSelector: () => {} }));
vi.mock('@core/signal/blockElement', () => ({
  blockHotkeyScopeSignal: { get: () => 'scope-id' },
}));
vi.mock('@core/util/upload', () => ({
  handleFileFolderDrop: vi.fn(
    (
      _fileEntries: FileSystemFileEntry[],
      _folderEntries: FileSystemDirectoryEntry[],
      onFilesReady: (files: UploadInput[]) => Promise<void>
    ) => {
      mocks.uploadCallback = onFilesReady;
    }
  ),
  uploadFiles: vi.fn(),
}));
vi.mock('@entity/types/entity', () => ({
  isTaskEntity: (entity: { subType?: { type: string } | null; type: string }) =>
    entity.type === 'document' && entity.subType?.type === 'task',
}));
vi.mock('@property/task-subtask-progress', () => ({
  TaskSubtaskProgressProvider: (props: ParentProps) => props.children,
}));
vi.mock('@property/editor/hooks/useAllProperties', () => ({
  useAllProperties: () => () => mocks.statusProperties,
}));
vi.mock('@queries/properties/entity', () => ({
  useBulkSaveEntityPropertiesMutation: (callbacks: {
    onSettled?: () => void;
  }) => {
    mocks.statusMutationCallbacks = callbacks;
    return {
      get isPending() {
        return mocks.statusMutationPending;
      },
      mutate: mocks.statusMutation,
    };
  },
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
vi.mock('@queries/storage/project-overview', () => ({
  useProjectOverviewQuery: (accessor: Accessor<string | undefined>) => {
    mocks.projectOverviewAccessors.push(accessor);
    return mocks.projectOverviewQuery;
  },
}));
vi.mock('@service-storage/util/refetchResources', () => ({
  refetchResources: vi.fn(),
}));
vi.mock('./ModalsProvider', () => ({
  ModalsProvider: (props: ParentProps) => props.children,
}));
vi.mock('./ProjectDecisionList', () => ({
  ProjectDecisionList: (props: { projectId: string; scopeId: string }) => {
    mocks.decisionListProps = props;
    return <div data-testid="project-decision-list" />;
  },
}));
vi.mock('./sidepanel/ProjectSidePanelSections', () => ({
  ProjectSidePanelSections: (props: { query: unknown }) => {
    mocks.sidePanelQuery = props.query;
    return null;
  },
}));
vi.mock('./ProjectTaskStatusBoard', () => ({
  ProjectTaskStatusBoard: (props: NonNullable<typeof mocks.boardProps>) => {
    mocks.boardProps = props;
    return (
      <select
        data-project-task-status-control={encodeURIComponent(
          props.tasks[0]?.id ?? 'unrelated-task'
        )}
        data-testid="project-task-status-board"
      />
    );
  },
}));
vi.mock('./ProjectTaskDeadlineTimeline', () => ({
  ProjectTaskDeadlineTimeline: (
    props: NonNullable<typeof mocks.timelineProps>
  ) => {
    mocks.timelineProps = props;
    return <div data-testid="project-task-deadline-timeline" />;
  },
}));
vi.mock('./TopBar', () => ({
  TopBar: (props: NonNullable<typeof mocks.topBarProps>) => {
    mocks.topBarProps = props;
    return null;
  },
}));

import { toast } from '@core/component/Toast/Toast';
import { handleFileFolderDrop, uploadFiles } from '@core/util/upload';
import { refetchSoupEntity } from '@queries/soup/cache';
import { refetchResources } from '@service-storage/util/refetchResources';
import Block from './Block';

afterEach(cleanup);

beforeEach(() => {
  mocks.blockEntityResolver = undefined;
  mocks.dependencyProviderCount = 0;
  mocks.dependencyProviderTaskIds = [];
  mocks.decisionListProps = undefined;
  mocks.boardProps = undefined;
  mocks.timelineProps = undefined;
  mocks.canEdit = true;
  mocks.controllerSplit = false;
  mocks.duplicatePreview = false;
  mocks.entryStateCalls = [];
  mocks.fileFolderDropOptions = undefined;
  mocks.focusSet.mockReset();
  mocks.milestoneFilterProps = undefined;
  mocks.milestoneToolbarItem = undefined;
  mocks.milestoneToolbarPortalCount = 0;
  mocks.soupViewProviderCount = 0;
  mocks.openEntityInSplit.mockReset();
  mocks.projectId = 'project-id';
  mocks.projectMetadata = { name: 'Project Atlas', userId: 'owner-id' };
  mocks.projectOverviewAccessors = [];
  mocks.projectOverviewQuery = {
    data: undefined,
    fetchStatus: 'idle',
    isError: false,
    isPending: false,
  };
  mocks.sidePanelQuery = undefined;
  mocks.source = [];
  mocks.sourceError = null;
  mocks.sourceFetchNextPage.mockReset();
  mocks.sourceRefresh.mockReset();
  mocks.sourceRefresh.mockResolvedValue(undefined);
  mocks.sourceFetching = false;
  mocks.sourceFetchingNextPage = false;
  mocks.sourceHasNextPage = false;
  mocks.sourceLoading = false;
  mocks.sourcePlaceholderData = false;
  mocks.statusMutation.mockReset();
  mocks.statusMutationCallbacks = undefined;
  mocks.statusMutationPending = false;
  mocks.statusProperties = [];
  mocks.searchText = '';
  mocks.topBarProps = undefined;
  mocks.viewMode = 'list';
  mocks.uploadCallback = undefined;
  mocks.rows = [];
  vi.mocked(handleFileFolderDrop).mockClear();
  vi.mocked(uploadFiles).mockReset();
  vi.mocked(refetchSoupEntity).mockReset();
  vi.mocked(refetchResources).mockReset();
  vi.mocked(toast.failure).mockReset();
});

it('resolves the current project for block-scoped entity commands', () => {
  render(() => <Block />);

  expect(mocks.blockEntityResolver?.()).toMatchObject({
    id: 'project-id',
    name: 'Project Atlas',
    ownerId: 'owner-id',
    type: 'project',
  });
});

it('does not resolve special projects for entity commands', () => {
  mocks.projectId = 'special-project';
  render(() => <Block />);

  expect(mocks.blockEntityResolver?.()).toBeUndefined();
});

describe('project native drop upload lifecycle', () => {
  const captureUploadCallback = () => {
    render(() => <Block />);
    mocks.fileFolderDropOptions?.onDrop?.([], []);
    expect(handleFileFolderDrop).toHaveBeenCalledWith(
      [],
      [],
      expect.any(Function)
    );
    expect(mocks.uploadCallback).toEqual(expect.any(Function));
    return mocks.uploadCallback!;
  };

  it('uploads immediate documents through the captured native drop callback and refreshes their project surfaces', async () => {
    const callback = captureUploadCallback();
    const file = new File(['document'], 'proposal.pdf');
    vi.mocked(uploadFiles).mockResolvedValue([
      {
        destination: 'dss',
        documentId: 'document-id',
        failed: false,
        fileType: undefined,
        name: 'proposal.pdf',
        pending: false,
        type: 'document',
      } as never,
    ]);

    await callback([file]);

    expect(uploadFiles).toHaveBeenCalledWith([file], 'dss', {
      projectId: 'project-id',
    });
    expect(refetchSoupEntity).toHaveBeenCalledTimes(1);
    expect(refetchSoupEntity).toHaveBeenCalledWith('document-id', 'document');
    expect(refetchResources).toHaveBeenCalledTimes(1);
    expect(toast.failure).not.toHaveBeenCalled();
  });

  it('does not refresh entities or resources when every upload result failed', async () => {
    const callback = captureUploadCallback();
    vi.mocked(uploadFiles).mockResolvedValue([
      {
        failed: true,
        name: 'failed.pdf',
        error: new Error('transport'),
      } as never,
    ]);

    await callback([new File(['document'], 'failed.pdf')]);

    expect(refetchSoupEntity).not.toHaveBeenCalled();
    expect(refetchResources).not.toHaveBeenCalled();
    expect(toast.failure).not.toHaveBeenCalled();
  });

  it('waits for a pending folder project before refreshing that project and resources', async () => {
    const callback = captureUploadCallback();
    let resolveProjectId: (projectId: string | undefined) => void = () => {};
    const projectId = new Promise<string | undefined>((resolve) => {
      resolveProjectId = resolve;
    });
    vi.mocked(uploadFiles).mockResolvedValue([
      {
        destination: 'dss',
        failed: false,
        name: 'folder.zip',
        pending: true,
        projectId,
        requestId: 'request-id',
        type: 'folder',
      } as never,
    ]);

    const completion = callback([
      { file: new File(['folder'], 'folder.zip'), isFolder: true },
    ]);
    await Promise.resolve();
    expect(refetchSoupEntity).not.toHaveBeenCalled();
    expect(refetchResources).not.toHaveBeenCalled();

    resolveProjectId('resolved-project-id');
    await completion;

    expect(refetchSoupEntity).toHaveBeenCalledTimes(1);
    expect(refetchSoupEntity).toHaveBeenCalledWith(
      'resolved-project-id',
      'project'
    );
    expect(refetchResources).toHaveBeenCalledTimes(1);
  });

  it('preserves immediate-document then pending-folder refresh order', async () => {
    const callback = captureUploadCallback();
    let resolveProjectId: (projectId: string | undefined) => void = () => {};
    const projectId = new Promise<string | undefined>((resolve) => {
      resolveProjectId = resolve;
    });
    vi.mocked(uploadFiles).mockResolvedValue([
      {
        destination: 'dss',
        documentId: 'immediate-document-id',
        failed: false,
        fileType: undefined,
        name: 'immediate.pdf',
        pending: false,
        type: 'document',
      } as never,
      {
        destination: 'dss',
        failed: false,
        name: 'folder.zip',
        pending: true,
        projectId,
        requestId: 'request-id',
        type: 'folder',
      } as never,
    ]);

    const completion = callback([new File(['document'], 'immediate.pdf')]);
    await Promise.resolve();
    expect(refetchSoupEntity).toHaveBeenCalledWith(
      'immediate-document-id',
      'document'
    );
    expect(refetchResources).toHaveBeenCalledTimes(1);

    resolveProjectId('pending-folder-project-id');
    await completion;

    expect(refetchSoupEntity).toHaveBeenNthCalledWith(
      1,
      'immediate-document-id',
      'document'
    );
    expect(refetchResources).toHaveBeenNthCalledWith(1);
    expect(refetchSoupEntity).toHaveBeenNthCalledWith(
      2,
      'pending-folder-project-id',
      'project'
    );
    expect(refetchResources).toHaveBeenCalledTimes(2);
    const [documentRefetchOrder, folderRefetchOrder] =
      vi.mocked(refetchSoupEntity).mock.invocationCallOrder;
    const [immediateResourceOrder, pendingResourceOrder] =
      vi.mocked(refetchResources).mock.invocationCallOrder;
    expect(documentRefetchOrder).toBeLessThan(immediateResourceOrder);
    expect(immediateResourceOrder).toBeLessThan(folderRefetchOrder);
    expect(folderRefetchOrder).toBeLessThan(pendingResourceOrder);
  });

  it('does not transport empty input', async () => {
    const callback = captureUploadCallback();

    await callback([]);

    expect(uploadFiles).not.toHaveBeenCalled();
    expect(refetchSoupEntity).not.toHaveBeenCalled();
    expect(refetchResources).not.toHaveBeenCalled();
  });

  it('keeps special projects disabled and fail-closed when their captured callback is invoked', async () => {
    mocks.projectId = 'special-project';
    const callback = captureUploadCallback();

    expect(mocks.fileFolderDropOptions?.disabled).toBe(true);
    await callback([new File(['document'], 'blocked.pdf')]);

    expect(uploadFiles).not.toHaveBeenCalled();
    expect(toast.failure).toHaveBeenCalledTimes(1);
    expect(toast.failure).toHaveBeenCalledWith(
      'Cannot upload files to this location'
    );
    expect(refetchSoupEntity).not.toHaveBeenCalled();
    expect(refetchResources).not.toHaveBeenCalled();
  });

  it('shows only the fixed failure toast when upload rejects without exposing the raw error', async () => {
    const callback = captureUploadCallback();
    const rawError = new Error('secret transport detail');
    const consoleError = vi
      .spyOn(console, 'error')
      .mockImplementation(() => undefined);
    vi.mocked(uploadFiles).mockRejectedValue(rawError);

    try {
      await callback([new File(['document'], 'failed.pdf')]);
    } finally {
      consoleError.mockRestore();
    }

    expect(toast.failure).toHaveBeenCalledTimes(1);
    expect(toast.failure).toHaveBeenCalledWith(
      'Upload failed. Please try again.'
    );
    expect(toast.failure).not.toHaveBeenCalledWith(rawError);
    expect(document.body.textContent).not.toContain('secret transport detail');
    expect(refetchSoupEntity).not.toHaveBeenCalled();
    expect(refetchResources).not.toHaveBeenCalled();
  });
});

describe('project task dependency relation batching', () => {
  it('shares one overview observer with the ordinary side panel and timeline', () => {
    mocks.viewMode = 'timeline';
    mocks.projectOverviewQuery.data = {
      operations: { startDate: '2026-01-02', targetDate: '2026-02-03' },
    };
    render(() => <Block />);

    expect(mocks.projectOverviewAccessors).toHaveLength(1);
    expect(mocks.projectOverviewAccessors[0]()).toBe('project-id');
    expect(mocks.sidePanelQuery).toBe(mocks.projectOverviewQuery);
    expect(mocks.timelineProps).toMatchObject({
      projectStartDate: '2026-01-02',
      projectTargetDate: '2026-02-03',
      rangeUnavailable: false,
    });
  });

  it('keeps retained overview dates when an error or offline pause follows data', () => {
    mocks.viewMode = 'timeline';
    mocks.projectOverviewQuery = {
      data: {
        operations: { startDate: '2026-01-02', targetDate: '2026-02-03' },
      },
      fetchStatus: 'idle',
      isError: true,
      isPending: false,
    };
    render(() => <Block />);
    expect(mocks.projectOverviewAccessors).toHaveLength(1);
    expect(mocks.timelineProps).toMatchObject({
      projectStartDate: '2026-01-02',
      projectTargetDate: '2026-02-03',
      rangeUnavailable: false,
    });
  });

  it('keeps retained overview dates during a paused pending refresh', () => {
    mocks.viewMode = 'timeline';
    mocks.projectOverviewQuery = {
      data: {
        operations: { startDate: '2026-01-02', targetDate: '2026-02-03' },
      },
      fetchStatus: 'paused',
      isError: false,
      isPending: true,
    };
    render(() => <Block />);
    expect(mocks.timelineProps).toMatchObject({
      projectStartDate: '2026-01-02',
      projectTargetDate: '2026-02-03',
      rangeUnavailable: false,
    });
  });

  it('marks only data-less failed or paused overview ranges unavailable', () => {
    mocks.viewMode = 'timeline';
    mocks.projectOverviewQuery = {
      data: undefined,
      fetchStatus: 'paused',
      isError: true,
      isPending: true,
    };
    render(() => <Block />);
    expect(mocks.projectOverviewAccessors).toHaveLength(1);
    expect(mocks.timelineProps).toMatchObject({
      projectStartDate: undefined,
      projectTargetDate: undefined,
      rangeUnavailable: true,
    });
  });

  it('marks a data-less initial pending overview range unavailable', () => {
    mocks.viewMode = 'timeline';
    mocks.projectOverviewQuery = {
      data: undefined,
      fetchStatus: 'fetching',
      isError: false,
      isPending: true,
    };
    render(() => <Block />);
    expect(mocks.timelineProps).toMatchObject({
      rangeUnavailable: true,
      projectStartDate: undefined,
      projectTargetDate: undefined,
    });
  });

  it('keeps special projects disabled and outside the shared overview surface', () => {
    mocks.projectId = 'special-project';
    render(() => <Block />);
    expect(mocks.projectOverviewAccessors).toHaveLength(1);
    expect(mocks.projectOverviewAccessors[0]()).toBeUndefined();
    expect(mocks.sidePanelQuery).toBeUndefined();
    expect(mocks.timelineProps).toBeUndefined();
  });

  it('routes the task Milestones filter through the ordinary project toolbar provider', () => {
    render(() => <Block />);

    expect(
      document.querySelector('[data-testid="project-milestone-filter"]')
    ).toBeTruthy();
    expect(
      document.querySelector('[data-testid="project-milestone-toolbar"]')
    ).toBeTruthy();
    expect(mocks.milestoneFilterProps).toMatchObject({
      viewOverride: 'tasks',
      hideLabel: false,
    });
    expect(mocks.milestoneToolbarItem).toEqual({
      id: 'project-toolbar-task-filter',
      priority: 1,
    });
    expect(mocks.milestoneToolbarPortalCount).toBe(1);
    expect(mocks.soupViewProviderCount).toBe(1);
  });

  it('keeps the board and Milestones control on one project provider without automatic paging', () => {
    mocks.viewMode = 'board';
    mocks.source = [
      { id: 'milestone-task', subType: { type: 'task' }, type: 'document' },
    ];
    render(() => <Block />);

    expect(mocks.boardProps?.tasks).toEqual(mocks.source);
    expect(mocks.soupViewProviderCount).toBe(1);
    expect(mocks.sourceFetchNextPage).not.toHaveBeenCalled();
    expect(mocks.boardProps).toBeDefined();
  });

  it('keeps the timeline on the same filtered source without automatic paging', () => {
    mocks.viewMode = 'timeline';
    mocks.source = [
      { id: 'timeline-task', subType: { type: 'task' }, type: 'document' },
      { id: 'document-a', subType: null, type: 'document' },
    ];
    render(() => <Block />);

    expect(mocks.timelineProps?.tasks).toEqual([mocks.source[0]]);
    expect(mocks.soupViewProviderCount).toBe(1);
    expect(mocks.sourceFetchNextPage).not.toHaveBeenCalled();
    expect(mocks.boardProps).toBeUndefined();
  });

  it('threads bounded continuation state and invokes the active Soup source once', () => {
    mocks.viewMode = 'board';
    mocks.searchText = 'follow up';
    mocks.sourceHasNextPage = true;
    mocks.source = [
      { id: 'task-a', subType: { type: 'task' }, type: 'document' },
    ];

    render(() => <Block />);

    expect(mocks.boardProps).toMatchObject({
      fetching: false,
      fetchingNextPage: false,
      hasNextPage: true,
      searching: true,
    });
    expect(mocks.sourceFetchNextPage).not.toHaveBeenCalled();

    mocks.sourceFetchNextPage.mockImplementation(() => {
      mocks.sourceFetchingNextPage = true;
    });
    mocks.boardProps?.onLoadMore?.();
    mocks.boardProps?.onLoadMore?.();

    expect(mocks.sourceFetchNextPage).toHaveBeenCalledTimes(1);
  });

  it('does not continue when the active Soup source has no page or is fetching', () => {
    mocks.viewMode = 'board';
    mocks.sourceHasNextPage = false;
    render(() => <Block />);
    mocks.boardProps?.onLoadMore?.();

    mocks.sourceHasNextPage = true;
    mocks.sourceFetching = true;
    mocks.boardProps?.onLoadMore?.();

    mocks.sourceFetching = false;
    mocks.sourceFetchingNextPage = true;
    mocks.boardProps?.onLoadMore?.();

    expect(mocks.sourceFetchNextPage).not.toHaveBeenCalled();
  });

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

  it('mounts the project-scoped Decision list only in Decisions mode', () => {
    render(() => <Block />);

    expect(mocks.decisionListProps).toBeUndefined();
    expect(
      document.querySelector('[data-testid="project-decision-list"]')
    ).toBeNull();

    mocks.topBarProps?.onChange('decisions');

    expect(mocks.decisionListProps).toEqual({
      projectId: 'project-id',
      scopeId: 'scope-id',
    });
    expect(
      document.querySelector('[data-testid="project-decision-list"]')
    ).toBeTruthy();
    expect(
      document.querySelector('[data-testid="project-milestone-filter"]')
    ).toBeNull();
    expect(mocks.boardProps).toBeUndefined();
    expect(mocks.timelineProps).toBeUndefined();
  });

  it('preserves the shared Task filter control and source across ordinary List, Board, and List', () => {
    mocks.searchText = 'follow up';
    mocks.source = [
      { id: 'matching-task-b', subType: { type: 'task' }, type: 'document' },
      { id: 'non-task-result', subType: null, type: 'document' },
      { id: 'matching-task-a', subType: { type: 'task' }, type: 'document' },
    ];

    render(() => <Block />);

    const filterControl = document.querySelector(
      '[data-testid="project-milestone-filter"]'
    );
    expect(filterControl).toBeTruthy();
    expect(mocks.soupViewProviderCount).toBe(1);

    mocks.topBarProps?.onChange('board');

    expect(mocks.boardProps).toMatchObject({
      searching: true,
      tasks: [{ id: 'matching-task-b' }, { id: 'matching-task-a' }],
    });
    expect(
      document.querySelector('[data-testid="project-milestone-filter"]')
    ).toBe(filterControl);
    expect(mocks.soupViewProviderCount).toBe(1);
    expect(mocks.sourceFetchNextPage).not.toHaveBeenCalled();

    mocks.topBarProps?.onChange('list');

    expect(
      document.querySelector('[data-testid="soup-view-list"]')
    ).toBeTruthy();
    expect(
      document.querySelector('[data-testid="project-milestone-filter"]')
    ).toBe(filterControl);
    expect(mocks.soupViewProviderCount).toBe(1);
    expect(mocks.sourceFetchNextPage).not.toHaveBeenCalled();
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

  it('threads loading, errors, paging, and ordinary split opens through the timeline', () => {
    mocks.viewMode = 'timeline';
    const task = { id: 'task-a', subType: { type: 'task' }, type: 'document' };
    mocks.source = [task];
    mocks.sourceHasNextPage = true;
    mocks.sourceError = new Error('source unavailable');
    mocks.searchText = 'follow up';
    render(() => <Block />);

    expect(mocks.timelineProps).toMatchObject({
      error: true,
      hasNextPage: true,
      loading: false,
      searching: true,
      tasks: [task],
    });
    mocks.timelineProps?.onRetry?.();
    mocks.timelineProps?.onLoadMore?.();
    mocks.timelineProps?.onOpenTask(
      task,
      new MouseEvent('click', { shiftKey: true })
    );
    expect(mocks.sourceRefresh).toHaveBeenCalledTimes(1);
    expect(mocks.sourceFetchNextPage).toHaveBeenCalledTimes(1);
    expect(mocks.openEntityInSplit).toHaveBeenCalledWith(
      task,
      expect.objectContaining({ openInNewSplit: true, replacePreview: false })
    );
  });

  it('submits a canonical task status move through the shared mutation seam', () => {
    mocks.viewMode = 'board';
    mocks.source = [
      { id: 'task-a', subType: { type: 'task' }, type: 'document' },
    ];
    mocks.statusProperties = [
      {
        id: '00000001-0000-0000-0000-000000000002',
        displayName: 'Status',
        valueType: 'SELECT_STRING',
        isMultiSelect: false,
        isMetadata: false,
        isSystem: true,
        owner: { scope: 'system' },
        createdAt: new Date(0).toISOString(),
        updatedAt: new Date(0).toISOString(),
      },
    ];

    render(() => <Block />);
    mocks.boardProps?.onMoveTaskStatus?.(
      mocks.source[0],
      '00000001-0000-0000-0002-000000000002'
    );

    expect(mocks.statusMutation).toHaveBeenCalledWith({
      properties: [
        expect.objectContaining({
          entityId: 'task-a',
          entityType: 'TASK',
          property: expect.objectContaining({
            propertyDefinitionId: '00000001-0000-0000-0000-000000000002',
          }),
          apiValues: {
            valueType: 'SELECT_STRING',
            values: ['00000001-0000-0000-0002-000000000002'],
          },
        }),
      ],
    });
    expect(mocks.boardProps?.activeStatusTaskId).toBe('task-a');
  });

  it('restores focus to the final status control after a settled rollback and ignores unrelated controls without an active task', () => {
    mocks.viewMode = 'board';
    const task = {
      id: 'task-a',
      subType: { type: 'task' },
      type: 'document',
    };
    mocks.source = [task];
    mocks.statusProperties = [
      {
        id: '00000001-0000-0000-0000-000000000002',
        displayName: 'Status',
        valueType: 'SELECT_STRING',
        isMultiSelect: false,
        isMetadata: false,
        isSystem: true,
        owner: { scope: 'system' },
        createdAt: new Date(0).toISOString(),
        updatedAt: new Date(0).toISOString(),
      },
    ];
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      frames.push(callback);
      return frames.length;
    });

    try {
      render(() => <Block />);
      mocks.boardProps?.onMoveTaskStatus?.(
        task,
        '00000001-0000-0000-0002-000000000002'
      );
      const optimisticControl = document.querySelector<HTMLSelectElement>(
        '[data-project-task-status-control="task-a"]'
      );
      expect(optimisticControl).toBeInstanceOf(HTMLSelectElement);
      optimisticControl?.focus();

      const rollbackControl = document.createElement('select');
      rollbackControl.dataset.projectTaskStatusControl = 'task-a';
      optimisticControl?.replaceWith(rollbackControl);
      mocks.statusMutationCallbacks?.onSettled?.();
      expect(mocks.boardProps?.activeStatusTaskId).toBe('task-a');
      frames.forEach((callback) => callback(0));

      expect(mocks.boardProps?.activeStatusTaskId).toBeUndefined();
      expect(document.activeElement).toBe(
        document.querySelector('[data-project-task-status-control="task-a"]')
      );
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('does not schedule focus for an unrelated status control without an active task', () => {
    mocks.viewMode = 'board';
    const requestAnimationFrame = vi.fn();
    vi.stubGlobal('requestAnimationFrame', requestAnimationFrame);

    try {
      render(() => <Block />);
      const unrelated = document.querySelector<HTMLSelectElement>(
        '[data-project-task-status-control="unrelated-task"]'
      );
      unrelated?.focus();
      mocks.statusMutationCallbacks?.onSettled?.();

      expect(requestAnimationFrame).not.toHaveBeenCalled();
      expect(document.activeElement).toBe(unrelated);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('does not transport a status move without edit permission or the canonical status definition', () => {
    const task = {
      id: 'task-a',
      subType: { type: 'task' },
      type: 'document',
    };
    mocks.viewMode = 'board';
    mocks.source = [task];
    mocks.statusProperties = [
      {
        id: '00000001-0000-0000-0000-000000000002',
        displayName: 'Status',
        valueType: 'SELECT_STRING',
        isMultiSelect: false,
        isMetadata: false,
        isSystem: true,
        owner: { scope: 'system' },
        createdAt: new Date(0).toISOString(),
        updatedAt: new Date(0).toISOString(),
      },
    ];
    mocks.canEdit = false;

    const noEdit = render(() => <Block />);
    mocks.boardProps?.onMoveTaskStatus?.(
      task,
      '00000001-0000-0000-0002-000000000001'
    );
    expect(mocks.statusMutation).not.toHaveBeenCalled();
    noEdit.unmount();

    mocks.canEdit = true;
    mocks.statusProperties = [];
    render(() => <Block />);
    mocks.boardProps?.onMoveTaskStatus?.(
      task,
      '00000001-0000-0000-0002-000000000001'
    );
    expect(mocks.statusMutation).not.toHaveBeenCalled();
  });

  it('threads the shared pending status mutation state to the board', () => {
    mocks.viewMode = 'board';
    mocks.statusMutationPending = true;

    render(() => <Block />);

    expect(mocks.boardProps?.statusPending).toBe(true);
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
    expect(
      document.querySelector('[data-testid="project-milestone-filter"]')
    ).toBeNull();
    expect(
      document.querySelector('[data-testid="project-milestone-toolbar"]')
    ).toBeNull();
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
