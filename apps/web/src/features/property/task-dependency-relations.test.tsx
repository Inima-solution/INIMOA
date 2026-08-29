/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { createMutable } from 'solid-js/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const taskA = '11111111-1111-4111-8111-111111111111';
const taskB = '22222222-2222-4222-8222-222222222222';
const taskC = '33333333-3333-4333-833333333333';
const mocks = vi.hoisted(() => ({
  fetchTaskDependencyRelations: vi.fn(),
  previewLoad: vi.fn(),
  queries: [] as Array<Record<string, any>>,
  queryOptions: [] as Array<Record<string, any>>,
}));

vi.mock('@queries/properties/task-dependency-relations', () => ({
  fetchTaskDependencyRelations: mocks.fetchTaskDependencyRelations,
}));
vi.mock('@queries/preview/dataloader', () => ({
  previewDataLoader: { load: mocks.previewLoad },
}));
vi.mock('@queries/preview', () => ({
  isAccessiblePreviewItem: (item: any) => item?.access === 'access',
}));
vi.mock('@core/util/result', () => ({
  thrownResultErrorHasCode: (error: any, code: string) =>
    error?.errors?.some((item: { code: string }) => item.code === code) ??
    false,
}));
vi.mock('@tanstack/solid-query', () => ({
  useQueries: (options: () => { queries: Array<Record<string, any>> }) => {
    const next = options().queries;
    mocks.queryOptions.push(...next);
    return mocks.queries.splice(0, next.length);
  },
}));
vi.mock('@core/component/ItemPreview', () => ({
  ItemPreview: (props: { id: string; class?: string }) => (
    <button aria-label={`Open ${props.id}`} class={props.class}>
      {`Task ${props.id}`}
    </button>
  ),
}));

import {
  TaskDependencyRelations,
  TaskDependencyRelationsProvider,
} from './task-dependency-relations';

function query(overrides: Record<string, unknown> = {}) {
  return {
    data: [],
    fetchStatus: 'idle',
    isError: false,
    isPending: false,
    ...overrides,
  };
}

function relation(overrides: Record<string, unknown> = {}) {
  return {
    taskId: taskA,
    readiness: 'ready',
    dependsOnTaskIds: [],
    blockingTaskIds: [],
    hasUnavailableDependencies: false,
    successorTaskIds: [],
    hasUnavailableSuccessors: false,
    ...overrides,
  };
}

function renderRelations(
  ids: () => readonly string[] = () => [taskA],
  mode: 'detail' | 'row' = 'detail'
) {
  return render(() => (
    <TaskDependencyRelationsProvider taskIds={ids}>
      <TaskDependencyRelations taskId={taskA} mode={mode} />
    </TaskDependencyRelationsProvider>
  ));
}

beforeEach(() => {
  mocks.fetchTaskDependencyRelations.mockReset();
  mocks.previewLoad.mockReset();
  mocks.queries = [];
  mocks.queryOptions = [];
});
afterEach(cleanup);

describe('TaskDependencyRelationsProvider', () => {
  it('first-seen deduplicates and executes the actual 200-ID query function', async () => {
    const ids = [
      taskA,
      taskA,
      ...Array.from({ length: 199 }, (_, i) => `task-${i}`),
    ];
    const [taskIds] = createSignal(ids);
    mocks.queries = [query()];
    renderRelations(taskIds);
    expect(mocks.queryOptions[0]?.queryKey).toEqual([
      'properties',
      'taskDependencyRelations',
      [taskA, ...ids.slice(2)],
    ]);
    await mocks.queryOptions[0]?.queryFn();
    expect(mocks.fetchTaskDependencyRelations).toHaveBeenCalledWith([
      taskA,
      ...ids.slice(2),
    ]);
  });

  it('chunks 201 requested IDs and executes both actual query functions', async () => {
    const ids = Array.from({ length: 201 }, (_, i) => `task-${i}`);
    mocks.queries = [query(), query()];
    renderRelations(() => ids);
    expect(mocks.queryOptions).toHaveLength(2);
    await Promise.all(mocks.queryOptions.map((option) => option.queryFn()));
    expect(mocks.fetchTaskDependencyRelations).toHaveBeenNthCalledWith(
      1,
      ids.slice(0, 200)
    );
    expect(mocks.fetchTaskDependencyRelations).toHaveBeenNthCalledWith(
      2,
      ids.slice(200)
    );
  });
});

describe('TaskDependencyRelations', () => {
  it('keeps server predecessor and successor order and exposes links only after every preview is accessible', () => {
    mocks.queries = [
      query({
        data: [
          relation({
            dependsOnTaskIds: [taskC, taskB],
            blockingTaskIds: [taskB],
            successorTaskIds: [taskB, taskC],
          }),
        ],
      }),
      query({ data: { access: 'access', id: taskC, type: 'task' } }),
      query({ data: { access: 'access', id: taskB, type: 'task' } }),
      query({ data: { access: 'access', id: taskB, type: 'task' } }),
      query({ data: { access: 'access', id: taskC, type: 'task' } }),
    ];
    const view = renderRelations();
    expect(
      [...view.getAllByRole('button')].map((node) =>
        node.getAttribute('aria-label')
      )
    ).toEqual([
      `Open ${taskC}`,
      `Open ${taskB}`,
      `Open ${taskB}`,
      `Open ${taskC}`,
    ]);
    expect(view.getByRole('group', { name: 'Predecessors' })).toBeTruthy();
    expect(view.getByRole('group', { name: 'Successors' })).toBeTruthy();
    expect(view.getAllByRole('button')[0]?.className).toContain('max-w-full');
    const marker = view.getByLabelText('Blocking predecessor');
    expect(marker.textContent).toBe('Blocking');
    expect(
      marker.parentElement?.querySelector('button')?.getAttribute('aria-label')
    ).toBe(`Open ${taskB}`);
    expect(view.getAllByLabelText('Blocking predecessor')).toHaveLength(1);
  });

  it.each([
    ['missing relation', query({ data: [] }), 'Task relations unavailable'],
    [
      'offline relation',
      query({ fetchStatus: 'paused' }),
      'Task relations offline',
    ],
    [
      'request error',
      query({ isError: true, error: new Error('private') }),
      "Couldn't load task relations",
    ],
  ])(
    'separates %s state without exposing IDs',
    (_name, relationQuery, text) => {
      mocks.queries = [relationQuery];
      const view = renderRelations();
      expect(view.getByText(text)).toBeTruthy();
      expect(view.container.textContent).not.toContain(taskB);
    }
  );

  it('fails a whole direction closed if a preview is denied or errors after another resolved', () => {
    mocks.queries = [
      query({
        data: [
          relation({
            dependsOnTaskIds: [taskB, taskC],
            blockingTaskIds: [taskB],
          }),
        ],
      }),
      query({ data: { access: 'access', id: taskB, type: 'task' } }),
      query({ data: { access: 'no_access', id: taskC, type: 'task' } }),
    ];
    const view = renderRelations();
    expect(view.getByText('Predecessors unavailable')).toBeTruthy();
    expect(view.queryByRole('button')).toBeNull();
    expect(view.container.textContent).not.toContain('Task name');
    expect(view.container.textContent).not.toContain(taskB);
    expect(view.container.textContent).not.toContain(taskC);
  });

  it('renders a loading direction without partial links', () => {
    mocks.queries = [
      query({
        data: [
          relation({
            readiness: 'blocked',
            blockingTaskIds: [taskB],
            dependsOnTaskIds: [taskB],
          }),
        ],
      }),
      query({ isPending: true }),
    ];
    const view = renderRelations();
    expect(view.getByText('Loading predecessors')).toBeTruthy();
    expect(
      view.getByText('This task is blocked. Checking blocking tasks.')
    ).toBeTruthy();
    expect(view.queryByRole('button')).toBeNull();
  });

  it('uses the unavailable blocker copy without exposing a partial direction', () => {
    mocks.queries = [
      query({
        data: [
          relation({
            readiness: 'blocked',
            hasUnavailableDependencies: true,
            dependsOnTaskIds: [taskB],
          }),
        ],
      }),
      query({ data: { access: 'access', id: taskB, type: 'task' } }),
    ];
    const view = renderRelations();
    expect(
      view.getByText('This task is blocked. Some dependencies are unavailable.')
    ).toBeTruthy();
    expect(view.getByText('Predecessors unavailable')).toBeTruthy();
    expect(view.queryByRole('button')).toBeNull();
    expect(mocks.queryOptions).toHaveLength(1);
  });

  it('reactively removes every previously visible predecessor when one preview becomes denied', () => {
    const predecessorB = createMutable<Record<string, any>>(
      query({ data: { access: 'access', id: taskB, type: 'task' } })
    );
    const predecessorC = createMutable<Record<string, any>>(
      query({ data: { access: 'access', id: taskC, type: 'task' } })
    );
    mocks.queries = [
      query({
        data: [
          relation({
            dependsOnTaskIds: [taskB, taskC],
            blockingTaskIds: [taskB],
          }),
        ],
      }),
      predecessorB,
      predecessorC,
    ];
    const view = renderRelations();
    expect(
      [...view.getAllByRole('button')].map((node) => node.textContent)
    ).toEqual([`Task ${taskB}`, `Task ${taskC}`]);
    expect(view.getByLabelText('Blocking predecessor')).toBeTruthy();

    predecessorC.data = { access: 'no_access', id: taskC, type: 'task' };

    expect(view.getByText('Predecessors unavailable')).toBeTruthy();
    expect(view.queryByRole('button')).toBeNull();
    expect(view.queryByLabelText('Blocking predecessor')).toBeNull();
    expect(view.container.textContent).not.toContain(`Task ${taskB}`);
    expect(view.container.textContent).not.toContain(`Task ${taskC}`);
    expect(view.container.textContent).not.toContain(taskB);
    expect(view.container.textContent).not.toContain(taskC);
    expect(view.container.textContent).not.toContain('server sentinel');
  });

  it('is a no-op with no provider or unrequested task', () => {
    const noProvider = render(() => <TaskDependencyRelations taskId={taskA} />);
    expect(noProvider.container.textContent).toBe('');
    mocks.queries = [query({ data: [relation()] })];
    const unrequested = render(() => (
      <TaskDependencyRelationsProvider taskIds={() => [taskB]}>
        <TaskDependencyRelations taskId={taskA} />
      </TaskDependencyRelationsProvider>
    ));
    expect(unrequested.container.textContent).toBe('');
  });

  it('renders Ready in neutral text for a ready task', () => {
    mocks.queries = [query({ data: [relation()] })];
    const view = renderRelations();
    expect(view.getByText('Ready').parentElement?.className).toContain(
      'basis-full'
    );
    expect(view.getByText('Ready').parentElement?.className).toContain(
      'min-w-0'
    );
  });

  it.each([
    ['ready', query({ data: [relation()] }), 'Ready'],
    [
      'blocked',
      query({ data: [relation({ readiness: 'blocked' })] }),
      'Blocked',
    ],
    ['loading', query({ isPending: true }), 'Loading…'],
    ['offline', query({ fetchStatus: 'paused' }), 'Offline'],
    [
      'error',
      query({ isError: true, error: new Error('server sentinel') }),
      'Load failed',
    ],
    ['unavailable', query({ data: [] }), 'Unavailable'],
  ])(
    'row mode renders only %s without preview queries',
    (_name, relationQuery, label) => {
      mocks.queries = [relationQuery];
      const view = renderRelations(() => [taskA], 'row');
      expect(view.getByLabelText(label).textContent).toBe(label);
      expect(view.getByLabelText(label).className).toContain('text-xs');
      expect(view.getByLabelText(label).className).toContain('shrink-0');
      expect(view.getByLabelText(label).className).toContain(
        'whitespace-nowrap'
      );
      expect(mocks.queryOptions).toHaveLength(1);
      expect(mocks.previewLoad).not.toHaveBeenCalled();
      expect(view.queryByRole('button')).toBeNull();
      expect(view.container.textContent).not.toContain('server sentinel');
    }
  );
});
