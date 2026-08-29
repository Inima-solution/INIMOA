/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import { createSignal, type JSX, type ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  previewLoad: vi.fn(),
  queries: [] as Array<Record<string, unknown>>,
  queryOptions: [] as Array<Record<string, unknown>>,
}));

vi.mock('@queries/properties/task-dependency-relations', () => ({
  fetchTaskDependencyRelations: vi.fn(),
}));
vi.mock('@core/component/ItemPreview', () => ({ ItemPreview: () => null }));
vi.mock('@queries/preview', () => ({ isAccessiblePreviewItem: () => true }));
vi.mock('@queries/preview/dataloader', () => ({
  previewDataLoader: { load: mocks.previewLoad },
}));
vi.mock('@queries/preview/keys', () => ({
  previewKeys: { item: (id: string) => ({ queryKey: [id] }) },
}));
vi.mock('@queries/properties/keys', () => ({
  propertiesKeys: {
    taskDependencyRelations: (ids: string[]) => ({ queryKey: ids }),
  },
}));
vi.mock('@core/util/result', () => ({ thrownResultErrorHasCode: () => false }));
vi.mock('@tanstack/solid-query', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tanstack/solid-query')>()),
  useQueries: (options: () => { queries: Array<Record<string, unknown>> }) => {
    mocks.queryOptions.push(...options().queries);
    return mocks.queries;
  },
}));
vi.mock('@app/features/next-soup/soup-view/soup-view-context', () => ({
  useMaybeSoupView: () => undefined,
}));
vi.mock('@block-call/utils', () => ({ formatCallDuration: () => '' }));
vi.mock('@property/tags', () => ({ EntityRowTags: () => null }));
vi.mock('@property/task-subtask-progress', () => ({
  TaskSubtaskProgressIndicator: (props: { taskId: string }) => (
    <span aria-label={`Subtask progress for ${props.taskId}`}>subtask</span>
  ),
}));
vi.mock('@service-properties/generated/schemas/entityType', () => ({
  EntityType: {
    TASK: 'task',
    DOCUMENT: 'document',
    PROJECT: 'project',
    THREAD: 'thread',
    CHAT: 'chat',
    CALL_RECORD: 'call',
  },
}));
vi.mock('@ui', () => ({
  cn: (...values: unknown[]) => values.filter(Boolean).join(' '),
}));
vi.mock('@ui/utils/classname', () => ({
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
  MultiSelectCheckbox: () => null,
  ProjectBreadCrumb: () => null,
  UnreadIndicator: () => null,
}));
vi.mock('@entity', () => ({
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
    Title: (props: { entity: { name: string } }) => (
      <span>{props.entity.name}</span>
    ),
    Timestamp: () => <time>timestamp</time>,
  },
  MultiSelectCheckbox: () => null,
  ProjectBreadCrumb: () => null,
  UnreadIndicator: () => null,
  isProjectContainedEntity: () => false,
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
vi.mock('@core/component/UserIcon', () => ({ UserIcon: () => null }));
vi.mock('@core/user', () => ({
  getDisplayNameParts: () => ({ firstName: 'Owner' }),
  tryMacroId: (id: string) => id,
}));
vi.mock('@entity/components/Badges', () => ({
  CreatedByBadgeSmall: () => null,
  SharedBadgeSmall: () => null,
}));
vi.mock('@entity/extractors-property', () => ({
  soupPropertyToProperty: (property: unknown) => property,
}));
vi.mock('@property/component/modal', () => ({ Modals: () => null }));
vi.mock('@property/context/PropertiesContext', () => ({
  PropertiesProvider: (props: ParentProps) => props.children,
}));
vi.mock('@queries/auth', () => ({ useUserId: () => () => 'owner-id' }));
vi.mock('@queries/properties/entity', () => ({
  useBulkSaveEntityPropertiesMutation: () => ({ mutateAsync: vi.fn() }),
}));
vi.mock('../../../next-soup/soup-view/views/tasks/list-property-value', () => ({
  ListPropertyValue: () => null,
}));
vi.mock('../../../next-soup/soup-view/views/tasks/task-grid-template', () => ({
  TASK_GRID_COLUMNS: [],
  TASK_GRID_TEMPLATE_AREAS_WIDE: '"content"',
  TASK_GRID_TEMPLATE_AREAS_WIDE_NO_INDICATOR: '"content"',
  TASK_GRID_TEMPLATE_COLUMNS_WIDE: '1fr',
  TASK_GRID_TEMPLATE_COLUMNS_WIDE_NO_INDICATOR: '1fr',
}));

import { TaskDependencyRelationsProvider } from '@property/task-dependency-relations';
import { TaskGridLayout } from '../../../next-soup/soup-view/views/tasks/task-grid-layout';
import { NarrowLayout } from './narrow-layout';
import type { LayoutProps } from './shared';
import { WideLayout } from './wide-layout';

const taskId = '11111111-1111-4111-8111-111111111111';

function relationQuery(readiness: 'blocked' | 'ready') {
  return {
    data: [
      {
        taskId,
        readiness,
        dependsOnTaskIds: [],
        successorTaskIds: [],
        hasUnavailableDependencies: false,
        hasUnavailableSuccessors: false,
      },
    ],
    error: undefined,
    fetchStatus: 'idle',
    isError: false,
    isPending: false,
  };
}

function entity(kind: 'task' | 'document'): LayoutProps['entity'] {
  return {
    fileType: 'md',
    id: taskId,
    name: `${kind} title`,
    ownerId: 'owner-id',
    type: 'document',
    ...(kind === 'task'
      ? { subType: { type: 'task' }, properties: [] }
      : { subType: null }),
  } as LayoutProps['entity'];
}

function props(kind: 'task' | 'document'): LayoutProps {
  return {
    chars: 100,
    entity: entity(kind),
    hasNotifications: false,
    isShared: false,
    setSnippetContainerRef: () => {},
    showHitSnippet: false,
    unread: false,
  };
}

function withRelations(layout: () => JSX.Element) {
  const [ids] = createSignal([taskId]);
  return render(() => (
    <TaskDependencyRelationsProvider taskIds={ids}>
      {layout()}
    </TaskDependencyRelationsProvider>
  ));
}

beforeEach(() => {
  mocks.previewLoad.mockReset();
  mocks.queries = [relationQuery('blocked')];
  mocks.queryOptions = [];
});
afterEach(cleanup);

describe('task dependency relation list placement', () => {
  it.each([
    ['wide', 'blocked', 'Blocked', () => <WideLayout {...props('task')} />],
    [
      'narrow/mobile',
      'blocked',
      'Blocked',
      () => <NarrowLayout {...props('task')} />,
    ],
    [
      'task grid',
      'blocked',
      'Blocked',
      () => <TaskGridLayout {...props('task')} />,
    ],
    ['wide', 'ready', 'Ready', () => <WideLayout {...props('task')} />],
    [
      'narrow/mobile',
      'ready',
      'Ready',
      () => <NarrowLayout {...props('task')} />,
    ],
    [
      'task grid',
      'ready',
      'Ready',
      () => <TaskGridLayout {...props('task')} />,
    ],
  ] as const)(
    'renders the %s %s task relation status in row mode',
    (_, readiness, label, layout) => {
      mocks.queries = [relationQuery(readiness)];
      const view = withRelations(layout);

      const status = view.getByLabelText(label);
      expect(status.getAttribute('aria-live')).toBe('polite');
      expect(status.className).toContain('text-ink-muted');
      expect(view.getByText('task title')).toBeTruthy();
      if (_ === 'narrow/mobile') {
        expect(status.parentElement?.className).toContain('flex');
        expect(status.parentElement?.className).toContain('gap-2');
      }
      expect(mocks.queryOptions).toHaveLength(1);
      expect(mocks.queryOptions[0]?.queryKey).toEqual([taskId]);
      expect(mocks.previewLoad).not.toHaveBeenCalled();
    }
  );

  it('keeps non-task rows and task rows outside the provider as no-ops', () => {
    const nonTask = withRelations(() => <WideLayout {...props('document')} />);
    expect(nonTask.queryByLabelText('Blocked')).toBeNull();
    expect(
      nonTask.queryByLabelText(`Subtask progress for ${taskId}`)
    ).toBeNull();

    const withoutProvider = render(() => <NarrowLayout {...props('task')} />);
    expect(withoutProvider.queryByLabelText('Blocked')).toBeNull();
    expect(
      withoutProvider.getByLabelText(`Subtask progress for ${taskId}`)
    ).toBeTruthy();
  });
});
