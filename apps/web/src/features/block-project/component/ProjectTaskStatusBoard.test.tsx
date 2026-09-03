/** @vitest-environment jsdom */

import type { TaskEntityWithProperties } from '@entity';
import { PROPERTY_OPTION_IDS, SYSTEM_PROPERTY_IDS } from '@property/constants';
import type { Property } from '@property/types';
import type { SoupProperty } from '@service-storage/generated/schemas';
import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { createSignal, type JSX } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProjectTaskStatusBoard } from './ProjectTaskStatusBoard';

const mocks = vi.hoisted(() => ({
  contextMenuEntities: [] as TaskEntityWithProperties[],
  relationTasks: [] as TaskEntityWithProperties[],
  progressTaskIds: [] as string[],
}));

vi.mock('@core/component/LoadingBlock', () => ({
  LoadingBlock: () => <div data-testid="loading-block">Loading tasks</div>,
}));
vi.mock('@entity', () => ({
  Entity: {
    Icon: () => <span aria-hidden="true" />,
    Title: (props: { entity: TaskEntityWithProperties }) => props.entity.name,
  },
}));
vi.mock('@app/features/next-soup/soup-view/soup-entity-context-menu', () => ({
  SoupEntityContextMenu: (props: {
    entity: TaskEntityWithProperties;
    children: JSX.Element;
  }) => {
    mocks.contextMenuEntities.push(props.entity);
    return <div data-testid="task-context-menu">{props.children}</div>;
  },
}));
vi.mock('@property/task-dependency-relations', () => ({
  TaskDependencyRelations: (props: {
    taskId: string;
    task?: TaskEntityWithProperties;
    mode?: string;
  }) => {
    if (props.task) mocks.relationTasks.push(props.task);
    return (
      <span data-testid="task-dependency-relation" data-mode={props.mode}>
        {props.task?.name}
      </span>
    );
  },
}));
vi.mock('@property/task-subtask-progress', () => ({
  TaskSubtaskProgressIndicator: (props: { taskId: string; mode?: string }) => {
    mocks.progressTaskIds.push(props.taskId);
    return (
      <span data-testid="task-subtask-progress" data-mode={props.mode}>
        Derived progress
      </span>
    );
  },
}));

afterEach(() => {
  cleanup();
  mocks.contextMenuEntities = [];
  mocks.relationTasks = [];
  mocks.progressTaskIds = [];
});

function task(name: string, status?: unknown): TaskEntityWithProperties {
  const properties: SoupProperty[] =
    status === undefined
      ? []
      : [
          {
            definition: { id: SYSTEM_PROPERTY_IDS.STATUS },
            value: status,
          } as unknown as SoupProperty,
        ];

  return {
    id: `raw-id-for-${name}`,
    name,
    ownerId: 'owner-id',
    type: 'document',
    fileType: 'md',
    subType: { type: 'task' },
    properties,
  };
}

function selectedStatus(value: string) {
  return { type: 'SelectOption', value: [value] };
}

function milestoneTask(name: string) {
  const item = task(name);
  item.properties = [
    {
      definition: { id: SYSTEM_PROPERTY_IDS.MILESTONE },
      value: { type: 'Boolean', value: true },
    } as unknown as SoupProperty,
  ];
  return item;
}

function malformedMilestoneTask(name: string) {
  const item = task(name);
  item.properties = [
    {
      definition: { id: SYSTEM_PROPERTY_IDS.MILESTONE },
      value: { type: 'Boolean', value: 'true' },
    } as unknown as SoupProperty,
  ];
  return item;
}

const statusProperty = {
  propertyId: SYSTEM_PROPERTY_IDS.STATUS,
  propertyDefinitionId: SYSTEM_PROPERTY_IDS.STATUS,
  displayName: 'Status',
  isMultiSelect: false,
  isRequired: true,
  owner: { scope: 'system' },
  createdAt: new Date(0).toISOString(),
  updatedAt: new Date(0).toISOString(),
  valueType: 'SELECT_STRING',
  value: null,
} as unknown as Property;

describe('ProjectTaskStatusBoard', () => {
  it('uses canonical status order and preserves source order inside buckets', () => {
    const notStarted = task(
      'First not started',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.NOT_STARTED)
    );
    const inProgress = task(
      'In progress',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS)
    );
    const completed = task(
      'Completed',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.COMPLETED)
    );
    const canceled = task(
      'Canceled',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.CANCELED)
    );
    const secondNotStarted = task(
      'Second not started',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.NOT_STARTED)
    );
    const malformed = task('Malformed', { type: 'Text', value: 'bad' });
    const unknown = task('Unknown', selectedStatus('not-a-status'));
    const noStatus = task('No status');
    const view = render(() => (
      <ProjectTaskStatusBoard
        tasks={[
          secondNotStarted,
          inProgress,
          completed,
          canceled,
          notStarted,
          malformed,
          unknown,
          noStatus,
        ]}
        onOpenTask={() => {}}
      />
    ));

    expect(
      [
        'Not Started',
        'In Progress',
        'In Review',
        'Completed',
        'Canceled',
        'No status',
      ].map((label) =>
        view
          .getByRole('region', { name: `${label} tasks` })
          .getAttribute('aria-label')
      )
    ).toEqual([
      'Not Started tasks',
      'In Progress tasks',
      'In Review tasks',
      'Completed tasks',
      'Canceled tasks',
      'No status tasks',
    ]);
    expect(
      Array.from(
        view
          .getByRole('region', { name: 'Not Started tasks' })
          .querySelectorAll('button')
      ).map((button) => button.textContent)
    ).toEqual(['Second not started', 'First not started']);
    expect(
      Array.from(
        view
          .getByRole('region', { name: 'No status tasks' })
          .querySelectorAll('button')
      ).map((button) => button.textContent)
    ).toEqual(['Malformed', 'Unknown', 'No status']);
    expect(mocks.contextMenuEntities).toEqual([
      secondNotStarted,
      notStarted,
      inProgress,
      completed,
      canceled,
      malformed,
      unknown,
      noStatus,
    ]);
    expect(view.getAllByTestId('task-context-menu')).toHaveLength(8);
  });

  it('renders keyboard-operable native task buttons and forwards the task with the native event', async () => {
    const item = task('Readable title');
    const onOpenTask = vi.fn();
    const view = render(() => (
      <ProjectTaskStatusBoard tasks={[item]} onOpenTask={onOpenTask} />
    ));
    const button = view.getByRole('button', { name: 'Readable title' });

    expect(
      view.getByRole('region', { name: 'Project task status board' })
    ).toBeTruthy();
    expect(button.tagName).toBe('BUTTON');
    expect(view.queryByText(item.id)).toBeNull();
    expect(button.getAttribute('draggable')).toBeNull();
    expect(button.classList.contains('focus-visible:ring-2')).toBe(true);

    fireEvent.click(button, { shiftKey: true });
    expect(onOpenTask).toHaveBeenCalledWith(item, expect.any(MouseEvent));
    expect(onOpenTask.mock.calls[0][1].shiftKey).toBe(true);

    expect(button.getAttribute('role')).toBeNull();
    const user = userEvent.setup();
    button.focus();
    await user.keyboard('{Enter}');
    await user.keyboard(' ');
    expect(onOpenTask).toHaveBeenCalledTimes(3);
  });

  it('places derived progress only on milestone rows and preserves relation rendering', () => {
    const milestone = milestoneTask('Milestone task');
    const ordinary = task('Ordinary task');
    const malformed = malformedMilestoneTask('Malformed milestone');
    const view = render(() => (
      <ProjectTaskStatusBoard
        tasks={[milestone, ordinary, malformed]}
        onOpenTask={() => {}}
      />
    ));

    const progress = view.getByTestId('task-subtask-progress');
    expect(progress.getAttribute('data-mode')).toBe('row');
    expect(mocks.progressTaskIds).toEqual([milestone.id]);
    expect(progress.closest('[data-testid="task-context-menu"]')).toBeTruthy();
    expect(mocks.relationTasks).toEqual([milestone, ordinary, malformed]);
    expect(view.getAllByTestId('task-dependency-relation')).toHaveLength(3);
  });

  it('keeps populated board content during loading and distinguishes loading from empty', () => {
    const item = task('Retained task');
    const loading = render(() => (
      <ProjectTaskStatusBoard tasks={[]} loading onOpenTask={() => {}} />
    ));
    expect(loading.queryByText('No tasks in this project')).toBeNull();
    expect(loading.getByTestId('loading-block')).toBeTruthy();
    loading.unmount();

    const refetch = render(() => (
      <ProjectTaskStatusBoard tasks={[item]} loading onOpenTask={() => {}} />
    ));
    expect(refetch.getByRole('button', { name: 'Retained task' })).toBeTruthy();
    refetch.unmount();

    const empty = render(() => (
      <ProjectTaskStatusBoard tasks={[]} onOpenTask={() => {}} />
    ));
    expect(empty.getByText('No tasks in this project')).toBeTruthy();
    empty.unmount();

    const searchEmpty = render(() => (
      <ProjectTaskStatusBoard tasks={[]} searching onOpenTask={() => {}} />
    ));
    expect(searchEmpty.getByText('No tasks match this search')).toBeTruthy();
  });

  it('shows a fixed retry state for an initial error and keeps retained tasks visible', () => {
    const onRetry = vi.fn();
    const error = render(() => (
      <ProjectTaskStatusBoard
        tasks={[]}
        error
        onOpenTask={() => {}}
        onRetry={onRetry}
      />
    ));

    expect(error.getByText('Couldn’t load tasks')).toBeTruthy();
    expect(error.queryByText('No tasks in this project')).toBeNull();
    expect(error.queryByText('No tasks match this search')).toBeNull();
    fireEvent.click(error.getByRole('button', { name: 'Retry' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
    error.unmount();

    const retained = render(() => (
      <ProjectTaskStatusBoard
        tasks={[task('Retained after error')]}
        error
        onOpenTask={() => {}}
      />
    ));
    expect(
      retained.getByRole('button', { name: 'Retained after error' })
    ).toBeTruthy();
    expect(retained.queryByText('Couldn’t load tasks')).toBeNull();
    expect(retained.queryByText('No tasks in this project')).toBeNull();
  });

  it('offers one bounded continuation below the board with loading and retry states', async () => {
    const first = task(
      'Original task',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.NOT_STARTED)
    );
    const second = task(
      'Appended task',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.NOT_STARTED)
    );
    const onLoadMore = vi.fn();
    let appendTasks = () => {};
    const view = render(() => {
      const [tasks, setTasks] = createSignal([first]);
      appendTasks = () => setTasks([first, second]);
      return (
        <ProjectTaskStatusBoard
          tasks={tasks()}
          hasNextPage
          onLoadMore={onLoadMore}
          onOpenTask={() => {}}
        />
      );
    });
    const loadMore = view.getByRole('button', { name: 'Load more tasks' });

    expect(
      view.getAllByRole('button', { name: 'Load more tasks' })
    ).toHaveLength(1);
    expect(
      view
        .getByRole('region', { name: 'Project task status board' })
        .contains(loadMore)
    ).toBe(true);
    expect(loadMore.closest('section[aria-label$=" tasks"]')).toBeNull();
    fireEvent.click(loadMore);
    const user = userEvent.setup();
    loadMore.focus();
    await user.keyboard('{Enter}');
    expect(onLoadMore).toHaveBeenCalledTimes(2);
    appendTasks();
    expect(
      Array.from(
        view
          .getByRole('region', { name: 'Not Started tasks' })
          .querySelectorAll('button')
      ).map((button) => button.textContent)
    ).toEqual(['Original task', 'Appended task']);
    view.unmount();

    const loading = render(() => (
      <ProjectTaskStatusBoard
        tasks={[first]}
        hasNextPage
        fetchingNextPage
        onLoadMore={onLoadMore}
        onOpenTask={() => {}}
      />
    ));
    const loadingButton = loading.getByRole('button', {
      name: 'Loading more…',
    });
    expect(loadingButton.hasAttribute('disabled')).toBe(true);
    expect(loadingButton.getAttribute('aria-busy')).toBe('true');
    loading.unmount();

    const retry = render(() => (
      <ProjectTaskStatusBoard
        tasks={[first]}
        error
        hasNextPage
        onLoadMore={onLoadMore}
        onOpenTask={() => {}}
      />
    ));
    const retryButton = retry.getByRole('button', {
      name: 'Retry loading more',
    });
    expect(retry.queryByText('Couldn’t load tasks')).toBeNull();
    expect(retry.queryByText('source unavailable')).toBeNull();
    const callsBeforeRetry = onLoadMore.mock.calls.length;
    fireEvent.click(retryButton);
    expect(onLoadMore).toHaveBeenCalledTimes(callsBeforeRetry + 1);
    retry.unmount();

    const onEmptyLoadMore = vi.fn();
    const emptyContinuation = render(() => (
      <ProjectTaskStatusBoard
        tasks={[]}
        hasNextPage
        onLoadMore={onEmptyLoadMore}
        onOpenTask={() => {}}
      />
    ));
    expect(
      emptyContinuation.queryByText('No tasks in this project')
    ).toBeNull();
    fireEvent.click(
      emptyContinuation.getByRole('button', { name: 'Load more tasks' })
    );
    expect(onEmptyLoadMore).toHaveBeenCalledTimes(1);
    emptyContinuation.unmount();

    const finalPage = render(() => (
      <ProjectTaskStatusBoard tasks={[]} onOpenTask={() => {}} />
    ));
    expect(finalPage.getByText('No tasks in this project')).toBeTruthy();
    expect(finalPage.queryByRole('button', { name: /more tasks/i })).toBeNull();
  });

  it('offers only canonical status moves and keeps open and status controls separate', async () => {
    const noStatusTask = task('Move from no status');
    const currentTask = task(
      'Current status',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS)
    );
    const onMoveTaskStatus = vi.fn();
    const view = render(() => (
      <ProjectTaskStatusBoard
        tasks={[noStatusTask, currentTask]}
        canEdit
        statusProperty={statusProperty}
        onMoveTaskStatus={onMoveTaskStatus}
        onOpenTask={() => {}}
      />
    ));
    const noStatusControl = view.container.querySelector(
      'select[aria-label="Move from no status status"]'
    );
    const currentControl = view.container.querySelector(
      'select[aria-label="Current status status"]'
    );
    if (
      !(noStatusControl instanceof HTMLSelectElement) ||
      !(currentControl instanceof HTMLSelectElement)
    ) {
      throw new Error('Expected native task status selects');
    }

    expect(noStatusControl.querySelectorAll('option')).toHaveLength(6);
    expect(
      Array.from(noStatusControl.options)
        .map((option) => option.value)
        .filter(Boolean)
    ).toEqual([
      PROPERTY_OPTION_IDS.STATUS.NOT_STARTED,
      PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS,
      PROPERTY_OPTION_IDS.STATUS.IN_REVIEW,
      PROPERTY_OPTION_IDS.STATUS.COMPLETED,
      PROPERTY_OPTION_IDS.STATUS.CANCELED,
    ]);
    expect(noStatusControl.value).toBe('');
    const openButton = view.getByRole('button', {
      name: 'Move from no status',
    });
    const contextRow = noStatusControl.closest(
      '[data-testid="task-context-menu"]'
    );
    expect(contextRow?.getAttribute('role')).toBeNull();
    expect(contextRow?.contains(openButton)).toBe(true);
    expect(contextRow?.contains(noStatusControl)).toBe(true);
    expect(openButton.contains(noStatusControl)).toBe(false);
    expect(noStatusControl.contains(openButton)).toBe(false);
    expect(openButton.classList.contains('touch:min-h-11')).toBe(true);
    expect(noStatusControl.classList.contains('touch:min-h-11')).toBe(true);
    const user = userEvent.setup();
    noStatusControl.focus();
    expect(document.activeElement).toBe(noStatusControl);
    await user.selectOptions(
      noStatusControl,
      PROPERTY_OPTION_IDS.STATUS.NOT_STARTED
    );
    fireEvent.change(currentControl, {
      target: { value: PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS },
    });
    expect(onMoveTaskStatus).toHaveBeenCalledTimes(1);
    expect(onMoveTaskStatus).toHaveBeenCalledWith(
      noStatusTask,
      PROPERTY_OPTION_IDS.STATUS.NOT_STARTED
    );
  });

  it('restores focus to the same task status control after an optimistic rebucket', async () => {
    const original = task(
      'Rebucket task',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS)
    );
    const rebucketed = task(
      'Rebucket task',
      selectedStatus(PROPERTY_OPTION_IDS.STATUS.COMPLETED)
    );
    let animationFrame: FrameRequestCallback | undefined;
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      animationFrame = callback;
      return 1;
    });

    try {
      const view = render(() => {
        const [tasks, setTasks] = createSignal([original]);
        return (
          <ProjectTaskStatusBoard
            tasks={tasks()}
            canEdit
            statusProperty={statusProperty}
            onOpenTask={() => {}}
            onMoveTaskStatus={() => setTasks([rebucketed])}
          />
        );
      });
      const control = view.getByRole('combobox', {
        name: 'Rebucket task status',
      });
      const user = userEvent.setup();

      await user.selectOptions(control, PROPERTY_OPTION_IDS.STATUS.COMPLETED);
      animationFrame?.(0);

      const movedControl = view
        .getByRole('region', { name: 'Completed tasks' })
        .querySelector<HTMLSelectElement>(
          'select[aria-label="Rebucket task status"]'
        );
      expect(document.activeElement).toBe(movedControl);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('hides status controls without permission or the canonical definition and marks only the active task busy', () => {
    const first = task('First task');
    const second = task('Second task');
    const noEdit = render(() => (
      <ProjectTaskStatusBoard
        tasks={[first]}
        statusProperty={statusProperty}
        onOpenTask={() => {}}
        onMoveTaskStatus={vi.fn()}
      />
    ));
    expect(noEdit.queryByRole('combobox')).toBeNull();
    noEdit.unmount();

    const missingDefinition = render(() => (
      <ProjectTaskStatusBoard
        tasks={[first]}
        canEdit
        onOpenTask={() => {}}
        onMoveTaskStatus={vi.fn()}
      />
    ));
    expect(missingDefinition.queryByRole('combobox')).toBeNull();
    missingDefinition.unmount();

    const pending = render(() => (
      <ProjectTaskStatusBoard
        tasks={[first, second]}
        canEdit
        statusProperty={statusProperty}
        statusPending
        activeStatusTaskId={first.id}
        onMoveTaskStatus={vi.fn()}
        onOpenTask={() => {}}
      />
    ));
    const controls = pending.getAllByRole('combobox');
    expect(controls.every((control) => control.hasAttribute('disabled'))).toBe(
      true
    );
    expect(controls[0].getAttribute('aria-busy')).toBe('true');
    expect(controls[1].getAttribute('aria-busy')).toBeNull();
  });

  it('uses responsive classes for horizontal board columns and narrow stacked sections', () => {
    const view = render(() => (
      <ProjectTaskStatusBoard
        tasks={[task('Responsive task')]}
        onOpenTask={() => {}}
      />
    ));
    const board = view.getByRole('region', {
      name: 'Project task status board',
    });
    const columns = board.querySelector('div');
    const column = view.getByRole('region', { name: 'Not Started tasks' });
    const taskBody = column.querySelector('header + div');

    expect(
      board.classList.contains('@container/project-task-status-board')
    ).toBe(true);
    expect(board.classList.contains('size-full')).toBe(true);
    expect(board.classList.contains('min-w-0')).toBe(true);
    expect(columns?.classList.contains('overflow-x-auto')).toBe(true);
    expect(
      columns?.classList.contains(
        '@max-[640px]/project-task-status-board:flex-col'
      )
    ).toBe(true);
    expect(
      column.classList.contains(
        '@max-[640px]/project-task-status-board:min-w-0'
      )
    ).toBe(true);
    expect(column.classList.contains('h-full')).toBe(true);
    expect(
      column.classList.contains('@max-[640px]/project-task-status-board:h-auto')
    ).toBe(true);
    expect(taskBody?.classList.contains('overflow-y-auto')).toBe(true);
    expect(
      taskBody?.classList.contains(
        '@max-[640px]/project-task-status-board:overflow-visible'
      )
    ).toBe(true);
  });
});
