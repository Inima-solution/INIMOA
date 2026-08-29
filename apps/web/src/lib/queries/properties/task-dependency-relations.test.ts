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
  fetchTaskDependencyRelations,
  useTaskDependencyRelationsQuery,
} from './task-dependency-relations';

const projectTask = '11111111-1111-4111-8111-111111111111';
const personalTask = '22222222-2222-4222-8222-222222222222';
const relatedTask = '33333333-3333-4333-8333-333333333333';

const rawRelations = [
  {
    blockingTaskIds: [relatedTask],
    blockingTaskNames: ['Hidden project blocker'],
    dependsOnTaskIds: [relatedTask],
    dependsOnTaskNames: ['Hidden project dependency'],
    hasUnavailableDependencies: true,
    hasUnavailableSuccessors: false,
    hiddenDependencyCount: 4,
    projectId: 'private-project-id',
    readiness: 'blocked',
    successorTaskIds: [],
    successorTaskNames: ['Hidden successor'],
    taskId: projectTask,
    taskName: 'Private project task',
  },
  {
    blockingTaskIds: [],
    dependsOnTaskIds: [],
    hasUnavailableDependencies: false,
    hasUnavailableSuccessors: true,
    hiddenSuccessorCount: 2,
    readiness: 'ready',
    successorTaskIds: [relatedTask],
    successorTaskNames: ['Hidden personal successor'],
    taskId: personalTask,
    taskName: 'Private personal task',
  },
  {
    blockingTaskIds: [],
    dependsOnTaskIds: [relatedTask],
    hasUnavailableDependencies: false,
    hasUnavailableSuccessors: false,
    readiness: 'ready',
    successorTaskIds: [],
    taskId: projectTask,
  },
];

beforeEach(() => {
  mocks.fetchWithToken.mockReset();
  queryClient.clear();
});

describe('task dependency relations properties client', () => {
  it('makes one exact POST and preserves project and personal ordered duplicates', async () => {
    const requestTaskIds = [projectTask, personalTask, projectTask];
    mocks.fetchWithToken.mockResolvedValueOnce(ok(rawRelations));

    await expect(
      fetchTaskDependencyRelations(requestTaskIds)
    ).resolves.toMatchObject([
      { taskId: projectTask },
      { taskId: personalTask },
      { taskId: projectTask },
    ]);
    expect(mocks.fetchWithToken).toHaveBeenCalledTimes(1);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/properties/task-dependency-relations',
      { method: 'POST', body: JSON.stringify({ taskIds: requestTaskIds }) }
    );
  });

  it('projects only the seven public relation fields', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(ok(rawRelations));

    const relations = await fetchTaskDependencyRelations([
      projectTask,
      personalTask,
      projectTask,
    ]);

    expect(relations).toEqual([
      {
        blockingTaskIds: [relatedTask],
        dependsOnTaskIds: [relatedTask],
        hasUnavailableDependencies: true,
        hasUnavailableSuccessors: false,
        readiness: 'blocked',
        successorTaskIds: [],
        taskId: projectTask,
      },
      {
        blockingTaskIds: [],
        dependsOnTaskIds: [],
        hasUnavailableDependencies: false,
        hasUnavailableSuccessors: true,
        readiness: 'ready',
        successorTaskIds: [relatedTask],
        taskId: personalTask,
      },
      {
        blockingTaskIds: [],
        dependsOnTaskIds: [relatedTask],
        hasUnavailableDependencies: false,
        hasUnavailableSuccessors: false,
        readiness: 'ready',
        successorTaskIds: [],
        taskId: projectTask,
      },
    ]);
    for (const relation of relations) {
      expect(Object.keys(relation).sort()).toEqual([
        'blockingTaskIds',
        'dependsOnTaskIds',
        'hasUnavailableDependencies',
        'hasUnavailableSuccessors',
        'readiness',
        'successorTaskIds',
        'taskId',
      ]);
    }
  });

  it('sends and returns an empty array for a present empty task ID list', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(ok([]));

    await expect(fetchTaskDependencyRelations([])).resolves.toEqual([]);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/properties/task-dependency-relations',
      { method: 'POST', body: JSON.stringify({ taskIds: [] }) }
    );
  });

  it('validates malformed and over-limit task IDs before network, accepting exactly 200', async () => {
    await expect(
      fetchTaskDependencyRelations(['not-a-uuid'])
    ).rejects.toThrow();
    await expect(
      fetchTaskDependencyRelations(
        Array.from({ length: 201 }, () => projectTask)
      )
    ).rejects.toThrow();
    expect(mocks.fetchWithToken).not.toHaveBeenCalled();

    const maxTaskIds = Array.from({ length: 200 }, () => projectTask);
    mocks.fetchWithToken.mockResolvedValueOnce(ok([]));
    await expect(fetchTaskDependencyRelations(maxTaskIds)).resolves.toEqual([]);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/properties/task-dependency-relations',
      { method: 'POST', body: JSON.stringify({ taskIds: maxTaskIds }) }
    );
  });

  it.each([
    { taskId: 'not-a-uuid' },
    { readiness: 'unknown' },
    { hasUnavailableSuccessors: undefined },
  ])('rejects malformed responses after network: %o', async (invalid) => {
    mocks.fetchWithToken.mockResolvedValueOnce(
      ok([{ ...rawRelations[0], ...invalid }])
    );
    await expect(fetchTaskDependencyRelations([projectTask])).rejects.toThrow();
    expect(mocks.fetchWithToken).toHaveBeenCalledTimes(1);
  });

  it.each([
    'BAD_REQUEST',
    'UNAUTHORIZED',
    'FORBIDDEN',
    'NOT_FOUND',
    'INTERNAL_SERVER_ERROR',
  ])('preserves %s Result errors as ThrownResultError', async (code) => {
    const errors = [{ code, message: `${code} from backend` }];
    mocks.fetchWithToken.mockResolvedValueOnce(err(errors));
    await expect(
      fetchTaskDependencyRelations([projectTask])
    ).rejects.toMatchObject({
      errors,
    });
    mocks.fetchWithToken.mockResolvedValueOnce(err(errors));
    await expect(
      fetchTaskDependencyRelations([projectTask])
    ).rejects.toBeInstanceOf(ThrownResultError);
  });
});

describe('task dependency relations queries', () => {
  it('uses exact keys that preserve order and duplicates', () => {
    const orderedTaskIds = [projectTask, personalTask, projectTask];
    const key = propertiesKeys.taskDependencyRelations(orderedTaskIds).queryKey;

    expect(key).toEqual([
      'properties',
      'taskDependencyRelations',
      orderedTaskIds,
    ]);
    expect(key).toEqual(
      propertiesKeys.taskDependencyRelations([
        projectTask,
        personalTask,
        projectTask,
      ]).queryKey
    );
    expect(key).not.toEqual(
      propertiesKeys.taskDependencyRelations([
        personalTask,
        projectTask,
        projectTask,
      ]).queryKey
    );
    expect(key).not.toEqual(
      propertiesKeys.taskDependencyRelations([projectTask, personalTask])
        .queryKey
    );
    expect(key).not.toEqual(
      propertiesKeys.taskSubtaskProgress(orderedTaskIds).queryKey
    );
    expect(key).not.toEqual(
      propertiesKeys.entity({
        entityId: projectTask,
        entityType: 'TASK',
      }).queryKey
    );
  });

  it('disables nullish task ID accessors with the family key', () => {
    for (const value of [null, undefined]) {
      createRoot((dispose) => {
        const [ids] = createSignal<readonly string[] | null | undefined>(value);
        let query!: ReturnType<typeof useTaskDependencyRelationsQuery>;
        QueryClientProvider({
          client: queryClient,
          get children() {
            query = useTaskDependencyRelationsQuery(ids);
            return null;
          },
        });

        expect(query.isEnabled).toBe(false);
        expect(
          queryClient.getQueryCache().find({
            exact: true,
            queryKey: propertiesKeys.taskDependencyRelations._def,
          })
        ).toBeDefined();
        expect(mocks.fetchWithToken).not.toHaveBeenCalled();
        dispose();
      });
    }
  });

  it('enables a present empty task ID list', () => {
    createRoot((dispose) => {
      const [ids] = createSignal<readonly string[]>([]);
      let query!: ReturnType<typeof useTaskDependencyRelationsQuery>;
      QueryClientProvider({
        client: queryClient,
        get children() {
          query = useTaskDependencyRelationsQuery(ids);
          return null;
        },
      });

      expect(query.isEnabled).toBe(true);
      expect(
        queryClient.getQueryCache().find({
          exact: true,
          queryKey: propertiesKeys.taskDependencyRelations([]).queryKey,
        })
      ).toBeDefined();
      dispose();
    });
  });
});
