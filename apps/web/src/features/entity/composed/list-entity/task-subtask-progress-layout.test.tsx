/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import { createSignal, type JSX, type ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  fetchTaskSubtaskProgress: vi.fn(),
  queries: [] as Array<Record<string, unknown>>,
  queryOptions: [] as Array<Record<string, unknown>>,
}));

vi.mock('@queries/properties/task-subtask-progress', () => ({
  fetchTaskSubtaskProgress: mocks.fetchTaskSubtaskProgress,
}));
vi.mock('@core/util/result', () => ({
  thrownResultErrorHasCode: () => false,
}));
vi.mock('@tanstack/solid-query', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tanstack/solid-query')>()),
  useQueries: (options: () => { queries: Array<Record<string, unknown>> }) => {
    mocks.queryOptions = options().queries;
    return mocks.queries;
  },
}));
vi.mock('@app/features/next-soup/soup-view/soup-view-context', () => ({
  useMaybeSoupView: () => undefined,
}));
vi.mock('@property/tags', () => ({ EntityRowTags: () => null }));
vi.mock('@property/task-dependency-relations', () => ({
  TaskDependencyRelations: () => null,
}));
vi.mock('@service-properties/generated/schemas/entityType', () => ({
  EntityType: {
    CALL_RECORD: 'call',
    CHAT: 'chat',
    DOCUMENT: 'document',
    PROJECT: 'project',
    TASK: 'task',
    THREAD: 'thread',
  },
}));
vi.mock('@ui', () => ({
  cn: (...values: unknown[]) => values.filter(Boolean).join(' '),
}));
vi.mock('../../components/Badges', () => ({
  CallDurationBadge: () => null,
  CallStatusBadge: () => null,
  SharedBadge: () => null,
}));
vi.mock('../../components/MultiSelectCheckbox', () => ({
  MultiSelectCheckbox: () => null,
}));
vi.mock('../../components/ProjectBreadCrumb', () => ({
  ProjectBreadCrumb: () => null,
}));
vi.mock('../../components/UnreadIndicator', () => ({
  UnreadIndicator: () => null,
}));
vi.mock('../../entity', () => ({
  Entity: {
    Layout: (
      props: ParentProps<{ class?: string; style?: JSX.CSSProperties }>
    ) => (
      <div class={props.class} style={props.style}>
        {props.children}
      </div>
    ),
    Slot: (props: ParentProps<{ class?: string; placement: string }>) => (
      <div class={props.class} data-placement={props.placement}>
        {props.children}
      </div>
    ),
    Icon: () => <span aria-hidden="true" />,
    Properties: () => <span>properties</span>,
    Timestamp: () => <time>timestamp</time>,
    Title: (props: { entity: { name: string } }) => (
      <span>{props.entity.name}</span>
    ),
  },
}));
vi.mock('../../types/entity', () => ({
  isAutomationEntity: () => false,
  isCallEntity: () => false,
  isChannelEntity: () => false,
  isChannelMessageEntity: () => false,
  isChatEntity: () => false,
  isDocumentEntity: (entity: { type: string }) => entity.type === 'document',
  isEmailEntity: () => false,
  isGithubPrEntity: () => false,
  isProjectContainedEntity: () => false,
  isProjectEntity: () => false,
  isReminderEntity: () => false,
  isTaskEntity: (entity: { subType?: { type?: string }; type: string }) =>
    entity.type === 'document' && entity.subType?.type === 'task',
}));
vi.mock('../../types/search', () => ({ isSearchEntity: () => false }));
vi.mock('./automation', () => ({ AutomationWideContent: () => null }));
vi.mock('./call', () => ({
  CallParticipants: () => null,
  CallWideContent: () => null,
}));
vi.mock('./channel', () => ({
  ChannelActiveCallBadge: () => null,
  ChannelJoinButton: () => null,
  ChannelMessageSingleLine: () => null,
  ChannelMessageWideContent: () => null,
  ChannelWideContent: () => null,
}));
vi.mock('./email', () => ({
  EmailInboxChip: () => null,
  EmailWideContent: () => null,
  useOwningInboxForEntity: () => () => undefined,
}));
vi.mock('./foreign', () => ({
  GithubPullRequestChecksIndicator: () => null,
  GithubPullRequestPills: () => null,
}));
vi.mock('./reminder', () => ({ ReminderWideContent: () => null }));

import { TaskSubtaskProgressProvider } from '@property/task-subtask-progress';
import { NarrowLayout } from './narrow-layout';
import type { LayoutProps } from './shared';
import { WideLayout } from './wide-layout';

const taskId = '11111111-1111-4111-8111-111111111111';

function query(overrides: Record<string, unknown> = {}) {
  return {
    data: [],
    error: undefined,
    fetchStatus: 'idle',
    isError: false,
    isPending: false,
    ...overrides,
  };
}

function taskOrDocumentEntity(
  kind: 'task' | 'document',
  id = taskId
): LayoutProps['entity'] {
  const base = {
    fileType: 'md',
    id,
    name: `${kind} name`,
    ownerId: '33333333-3333-4333-8333-333333333333',
    type: 'document' as const,
  };

  return kind === 'task'
    ? { ...base, subType: { type: 'task' } }
    : { ...base, subType: null };
}

function layoutProps(kind: 'task' | 'document', id = taskId): LayoutProps {
  return {
    chars: 100,
    entity: taskOrDocumentEntity(kind, id),
    hasNotifications: false,
    isShared: false,
    setSnippetContainerRef: () => {},
    showHitSnippet: false,
    unread: false,
  };
}

function renderWithProgress(layout: () => JSX.Element) {
  const [taskIds] = createSignal([taskId]);
  return render(() => (
    <TaskSubtaskProgressProvider taskIds={taskIds}>
      {layout()}
    </TaskSubtaskProgressProvider>
  ));
}

beforeEach(() => {
  mocks.fetchTaskSubtaskProgress.mockReset();
  mocks.queries = [];
  mocks.queryOptions = [];
});

afterEach(cleanup);

describe('project task subtask progress list placement', () => {
  it.each([
    ['wide', () => <WideLayout {...layoutProps('task')} />],
    ['narrow mobile', () => <NarrowLayout {...layoutProps('task')} />],
  ])('renders the ready count in the %s task row', (_, layout) => {
    mocks.queries = [
      query({
        data: [
          {
            completedSubtasks: 2,
            hasUnavailableSubtasks: false,
            taskId,
            totalSubtasks: 3,
          },
        ],
      }),
    ];

    const view = renderWithProgress(layout);

    expect(view.getByLabelText('2 of 3 subtasks complete')).toBeTruthy();
    expect(view.container.querySelector('.tabular-nums')?.textContent).toBe(
      '2/3'
    );
    expect(
      view.container.querySelector(
        _ === 'wide' ? "[data-placement='meta']" : "[data-placement='title']"
      )?.textContent
    ).toContain('2/3');
  });

  it('keeps narrow/mobile unavailable state private', () => {
    mocks.queries = [
      query({
        data: [
          {
            completedSubtasks: 99,
            hasUnavailableSubtasks: true,
            taskId,
            totalSubtasks: 101,
          },
        ],
      }),
    ];
    const view = renderWithProgress(() => (
      <NarrowLayout {...layoutProps('task')} />
    ));

    expect(view.getByText('Unavailable').getAttribute('aria-label')).toBe(
      'Subtask progress unavailable'
    );
    expect(view.container.querySelector('.tabular-nums')).toBeNull();
    expect(view.container.textContent).not.toContain('99');
    expect(view.container.textContent).not.toContain('101');
    expect(view.container.textContent).not.toContain(taskId);
  });

  it('does not render progress for a non-task row', () => {
    mocks.queries = [query()];
    const view = renderWithProgress(() => (
      <WideLayout {...layoutProps('document')} />
    ));

    expect(view.container.textContent).not.toContain('subtask progress');
    expect(view.container.querySelector('.tabular-nums')).toBeNull();
  });

  it('does not render progress when a task row is outside the project provider', () => {
    const view = render(() => <NarrowLayout {...layoutProps('task')} />);

    expect(view.container.textContent).not.toContain('subtask progress');
    expect(view.container.querySelector('.tabular-nums')).toBeNull();
  });

  it('uses one shared batch query for multiple task rows', async () => {
    const secondTaskId = '22222222-2222-4222-8222-222222222222';
    const [taskIds] = createSignal([taskId, secondTaskId]);
    mocks.queries = [query()];

    render(() => (
      <TaskSubtaskProgressProvider taskIds={taskIds}>
        <WideLayout {...layoutProps('task')} />
        <NarrowLayout {...layoutProps('task', secondTaskId)} />
      </TaskSubtaskProgressProvider>
    ));

    expect(mocks.queryOptions).toHaveLength(1);
    const batch = mocks.queryOptions[0];
    if (!batch) throw new Error('Expected one batch query');
    await (batch.queryFn as () => Promise<unknown>)();
    expect(mocks.fetchTaskSubtaskProgress).toHaveBeenCalledOnce();
    expect(mocks.fetchTaskSubtaskProgress).toHaveBeenCalledWith([
      taskId,
      secondTaskId,
    ]);
  });
});
