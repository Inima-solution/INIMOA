import { ThrownResultError } from '@core/util/result';
import { queryClient } from '@queries/client';
import { QueryClientProvider } from '@tanstack/solid-query';
import { err, ok } from 'neverthrow';
import { createRoot, createSignal } from 'solid-js';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  fetchWithToken: vi.fn(),
}));

vi.mock('@core/util/fetchWithToken', () => ({
  fetchWithToken: mocks.fetchWithToken,
}));
vi.mock('@core/constant/servers', () => ({
  SERVER_HOSTS: { 'document-storage-service': 'http://dss.test' },
}));

import { propertiesKeys } from './keys';
import {
  fetchTaskSubtaskProgress,
  useTaskSubtaskProgressQuery,
} from './task-subtask-progress';

const taskA = '11111111-1111-4111-8111-111111111111';
const taskB = '22222222-2222-4222-8222-222222222222';

const rawProgress = [
  {
    completedSubtasks: 1,
    hasUnavailableSubtasks: false,
    hiddenSubtaskCount: 7,
    subtaskIds: [taskB],
    subtaskNames: ['Private child'],
    taskId: taskA,
    taskName: 'Private parent',
    totalSubtasks: 2,
  },
  {
    completedSubtasks: 0,
    hasUnavailableSubtasks: true,
    hiddenSubtaskCount: 3,
    subtaskIds: [taskB],
    subtaskNames: ['Another private child'],
    taskId: taskA,
    taskName: 'Duplicate parent',
    totalSubtasks: 1,
  },
];

const projectedProgress = [
  {
    completedSubtasks: 1,
    hasUnavailableSubtasks: false,
    taskId: taskA,
    totalSubtasks: 2,
  },
  {
    completedSubtasks: 0,
    hasUnavailableSubtasks: true,
    taskId: taskA,
    totalSubtasks: 1,
  },
];

beforeEach(() => {
  mocks.fetchWithToken.mockReset();
  queryClient.clear();
});

describe('task subtask progress properties client', () => {
  it('makes one exact POST and projects the ordered duplicate response', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(ok(rawProgress));

    await expect(fetchTaskSubtaskProgress([taskA, taskA])).resolves.toEqual(
      projectedProgress
    );

    expect(mocks.fetchWithToken).toHaveBeenCalledTimes(1);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/properties/task-subtask-progress',
      { method: 'POST', body: JSON.stringify({ taskIds: [taskA, taskA] }) }
    );
  });

  it('sends and returns an empty array for a present empty task ID list', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(ok([]));

    await expect(fetchTaskSubtaskProgress([])).resolves.toEqual([]);

    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/properties/task-subtask-progress',
      { method: 'POST', body: JSON.stringify({ taskIds: [] }) }
    );
  });

  it('validates malformed and over-limit task IDs before making a request', async () => {
    await expect(fetchTaskSubtaskProgress(['not-a-uuid'])).rejects.toThrow();
    await expect(
      fetchTaskSubtaskProgress(Array.from({ length: 201 }, () => taskA))
    ).rejects.toThrow();

    expect(mocks.fetchWithToken).not.toHaveBeenCalled();

    const maxTaskIds = Array.from({ length: 200 }, () => taskA);
    mocks.fetchWithToken.mockResolvedValueOnce(ok([]));
    await expect(fetchTaskSubtaskProgress(maxTaskIds)).resolves.toEqual([]);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/properties/task-subtask-progress',
      { method: 'POST', body: JSON.stringify({ taskIds: maxTaskIds }) }
    );
  });

  it('rejects malformed responses after receiving them', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(
      ok([
        {
          completedSubtasks: -1,
          hasUnavailableSubtasks: false,
          taskId: taskA,
          totalSubtasks: 0,
        },
      ])
    );

    await expect(fetchTaskSubtaskProgress([taskA])).rejects.toThrow();
  });

  it.each([
    'BAD_REQUEST',
    'UNAUTHORIZED',
    'FORBIDDEN',
    'NOT_FOUND',
    'INTERNAL_SERVER_ERROR',
  ])('preserves %s Result errors in ThrownResultError', async (code) => {
    const errors = [{ code, message: `${code} from backend` }];
    mocks.fetchWithToken.mockResolvedValueOnce(err(errors));

    await expect(fetchTaskSubtaskProgress([taskA])).rejects.toMatchObject({
      errors,
    });
    mocks.fetchWithToken.mockResolvedValueOnce(err(errors));
    await expect(fetchTaskSubtaskProgress([taskA])).rejects.toBeInstanceOf(
      ThrownResultError
    );
  });
});

describe('task subtask progress queries', () => {
  it('uses exact keys that preserve order and duplicates', () => {
    const orderedTaskIds = [taskA, taskB, taskA];
    const key = propertiesKeys.taskSubtaskProgress(orderedTaskIds).queryKey;

    expect(key).toEqual(['properties', 'taskSubtaskProgress', orderedTaskIds]);
    expect(key).toEqual(
      propertiesKeys.taskSubtaskProgress([taskA, taskB, taskA]).queryKey
    );
    expect(key).not.toEqual(
      propertiesKeys.taskSubtaskProgress([taskB, taskA, taskA]).queryKey
    );
    expect(key).not.toEqual(
      propertiesKeys.taskSubtaskProgress([taskA, taskB]).queryKey
    );
    expect(key).not.toEqual(
      propertiesKeys.entity({
        entityId: taskA,
        entityType: 'TASK',
      }).queryKey
    );
  });

  it('disables nullish task ID accessors with the family key', () => {
    for (const value of [null, undefined]) {
      createRoot((dispose) => {
        const [ids] = createSignal<readonly string[] | null | undefined>(value);
        let query!: ReturnType<typeof useTaskSubtaskProgressQuery>;
        QueryClientProvider({
          client: queryClient,
          get children() {
            query = useTaskSubtaskProgressQuery(ids);
            return null;
          },
        });

        expect(query.isEnabled).toBe(false);
        expect(mocks.fetchWithToken).not.toHaveBeenCalled();
        dispose();
      });
    }
  });

  it('enables an empty task ID list', () => {
    createRoot((dispose) => {
      const [ids] = createSignal<readonly string[]>([]);
      let query!: ReturnType<typeof useTaskSubtaskProgressQuery>;
      QueryClientProvider({
        client: queryClient,
        get children() {
          query = useTaskSubtaskProgressQuery(ids);
          return null;
        },
      });

      expect(query.isEnabled).toBe(true);
      dispose();
    });
  });
});
