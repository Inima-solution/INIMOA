/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
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
  thrownResultErrorHasCode: (
    error: { errors?: Array<{ code: string }> } | undefined,
    code: string
  ) => error?.errors?.some((item) => item.code === code) ?? false,
}));

vi.mock('@tanstack/solid-query', () => ({
  useQueries: (options: () => { queries: Array<Record<string, unknown>> }) => {
    mocks.queryOptions = options().queries;
    return mocks.queries;
  },
}));

import {
  TaskSubtaskProgressIndicator,
  TaskSubtaskProgressProvider,
} from './task-subtask-progress';

const taskA = '11111111-1111-4111-8111-111111111111';

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

function renderProgress(
  taskIds: () => readonly string[],
  mode: 'detail' | 'row' = 'detail'
) {
  return render(() => (
    <TaskSubtaskProgressProvider taskIds={taskIds}>
      <TaskSubtaskProgressIndicator taskId={taskA} mode={mode} />
    </TaskSubtaskProgressProvider>
  ));
}

beforeEach(() => {
  mocks.fetchTaskSubtaskProgress.mockReset();
  mocks.queries = [];
  mocks.queryOptions = [];
});

afterEach(cleanup);

describe('TaskSubtaskProgressProvider', () => {
  it('deduplicates first-seen task IDs into one request for up to 200 IDs', async () => {
    const ids = Array.from({ length: 199 }, (_, index) => `task-${index}`);
    const [taskIds] = createSignal([taskA, taskA, ...ids]);
    mocks.queries = [query()];

    renderProgress(taskIds);

    expect(mocks.queryOptions).toHaveLength(1);
    expect(mocks.queryOptions[0]?.enabled).toBe(true);
    expect(mocks.queryOptions[0]?.queryKey).toEqual([
      'properties',
      'taskSubtaskProgress',
      [taskA, ...ids],
    ]);
    const firstQuery = mocks.queryOptions[0];
    if (!firstQuery) throw new Error('Expected one batch query');
    await (firstQuery.queryFn as () => Promise<unknown>)();
    expect(mocks.fetchTaskSubtaskProgress).toHaveBeenCalledOnce();
    expect(mocks.fetchTaskSubtaskProgress).toHaveBeenCalledWith([
      taskA,
      ...ids,
    ]);
  });

  it('splits 201 unique task IDs into two batch requests', async () => {
    const ids = Array.from({ length: 201 }, (_, index) => `task-${index}`);
    const [taskIds] = createSignal(ids);
    mocks.queries = [query(), query()];

    renderProgress(taskIds);

    expect(mocks.queryOptions).toHaveLength(2);
    const [firstQuery, secondQuery] = mocks.queryOptions;
    if (!firstQuery || !secondQuery)
      throw new Error('Expected two batch queries');
    expect((firstQuery.queryKey as readonly unknown[])[2]).toHaveLength(200);
    expect((secondQuery.queryKey as readonly unknown[])[2]).toEqual([
      'task-200',
    ]);
    await Promise.all(
      mocks.queryOptions.map((options) =>
        (options.queryFn as () => Promise<unknown>)()
      )
    );
    expect(mocks.fetchTaskSubtaskProgress).toHaveBeenCalledTimes(2);
    expect(mocks.fetchTaskSubtaskProgress).toHaveBeenNthCalledWith(
      1,
      ids.slice(0, 200)
    );
    expect(mocks.fetchTaskSubtaskProgress).toHaveBeenNthCalledWith(2, [
      'task-200',
    ]);
  });

  it('does not configure a request for an empty task ID list', () => {
    const [taskIds] = createSignal<readonly string[]>([]);

    renderProgress(taskIds);

    expect(mocks.queryOptions).toEqual([]);
    expect(mocks.fetchTaskSubtaskProgress).not.toHaveBeenCalled();
  });
});

describe('TaskSubtaskProgressIndicator', () => {
  it('renders the exact accessible ready count using the existing progress meter', () => {
    mocks.queries = [
      query({
        data: [
          {
            completedSubtasks: 2,
            hasUnavailableSubtasks: false,
            taskId: taskA,
            totalSubtasks: 3,
          },
        ],
      }),
    ];
    const view = renderProgress(() => [taskA]);

    expect(view.getByLabelText('2 of 3 subtasks complete')).toBeTruthy();
    expect(view.container.querySelector('.tabular-nums')?.textContent).toBe(
      '2/3'
    );
  });

  it('fails closed when the batch response omits the requested task', () => {
    mocks.queries = [query({ data: [] })];
    const view = renderProgress(() => [taskA]);

    expect(view.getByText('Subtask progress unavailable')).toBeTruthy();
    expect(view.container.querySelector('.tabular-nums')).toBeNull();
  });

  it.each([
    ['loading', query({ isPending: true }), 'Loading subtask progress'],
    [
      'empty',
      query({
        data: [
          {
            completedSubtasks: 0,
            hasUnavailableSubtasks: false,
            taskId: taskA,
            totalSubtasks: 0,
          },
        ],
      }),
      'No subtasks',
    ],
    [
      'unavailable',
      query({
        data: [
          {
            completedSubtasks: 5,
            hasUnavailableSubtasks: true,
            taskId: taskA,
            totalSubtasks: 7,
          },
        ],
      }),
      'Subtask progress unavailable',
    ],
    [
      'request error',
      query({ error: new Error('private'), isError: true }),
      "Couldn't load subtask progress",
    ],
    [
      'unauthorized',
      query({
        error: { errors: [{ code: 'UNAUTHORIZED' }] },
        isError: true,
      }),
      'Subtask progress unavailable',
    ],
    [
      'forbidden',
      query({ error: { errors: [{ code: 'FORBIDDEN' }] }, isError: true }),
      'Subtask progress unavailable',
    ],
    [
      'not found',
      query({ error: { errors: [{ code: 'NOT_FOUND' }] }, isError: true }),
      'Subtask progress unavailable',
    ],
    [
      'offline',
      query({ fetchStatus: 'paused', isPending: true }),
      'Subtask progress offline',
    ],
  ])(
    'renders %s without counts or private response details',
    (_, result, label) => {
      mocks.queries = [result];
      const view = renderProgress(() => [taskA]);

      expect(view.getByText(label).getAttribute('role')).toBe('status');
      expect(view.container.textContent).not.toContain('5');
      expect(view.container.textContent).not.toContain('7');
      expect(view.container.textContent).not.toContain(taskA);
      expect(view.container.textContent).not.toContain('private');
    }
  );

  it.each([
    [
      'loading',
      query({ isPending: true }),
      'Loading…',
      'Loading subtask progress',
    ],
    [
      'empty',
      query({
        data: [
          {
            completedSubtasks: 0,
            hasUnavailableSubtasks: false,
            taskId: taskA,
            totalSubtasks: 0,
          },
        ],
      }),
      'No subtasks',
      'No subtasks',
    ],
    [
      'unavailable',
      query({
        data: [
          {
            completedSubtasks: 5,
            hasUnavailableSubtasks: true,
            taskId: taskA,
            totalSubtasks: 7,
          },
        ],
      }),
      'Unavailable',
      'Subtask progress unavailable',
    ],
    [
      'error',
      query({ error: new Error('private'), isError: true }),
      'Load failed',
      "Couldn't load subtask progress",
    ],
    [
      'offline',
      query({ fetchStatus: 'paused', isPending: true }),
      'Offline',
      'Subtask progress offline',
    ],
  ])(
    'renders compact %s row copy with the full accessible label',
    (_, result, visibleLabel, accessibleLabel) => {
      mocks.queries = [result];
      const view = renderProgress(() => [taskA], 'row');
      const indicator = view.getByText(visibleLabel);

      expect(indicator.getAttribute('aria-label')).toBe(accessibleLabel);
      expect(indicator.getAttribute('role')).toBe(null);
      expect(indicator.className).toContain('text-xs');
      expect(view.container.querySelector('.tabular-nums')).toBeNull();
      expect(view.container.textContent).not.toContain('5');
      expect(view.container.textContent).not.toContain('7');
      expect(view.container.textContent).not.toContain('private');
    }
  );
});
