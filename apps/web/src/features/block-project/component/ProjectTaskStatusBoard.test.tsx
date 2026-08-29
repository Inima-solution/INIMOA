/** @vitest-environment jsdom */

import type { TaskEntityWithProperties } from '@entity';
import { PROPERTY_OPTION_IDS, SYSTEM_PROPERTY_IDS } from '@property/constants';
import type { SoupProperty } from '@service-storage/generated/schemas';
import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import type { JSX } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProjectTaskStatusBoard } from './ProjectTaskStatusBoard';

const mocks = vi.hoisted(() => ({
  contextMenuEntities: [] as TaskEntityWithProperties[],
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

afterEach(() => {
  cleanup();
  mocks.contextMenuEntities = [];
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
