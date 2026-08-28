import { ThrownResultError } from '@core/util/result';
import { queryClient } from '@queries/client';
import type { GetProjectTaskDependencyReadiness200Item } from '@service-storage/generated/schemas';
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
  SYNC_PERMISSION_TOKEN_DSS_HOST: 'http://sync-permission.test',
  SYNC_SERVICE_HOSTS: { worker: 'http://sync-worker.test' },
}));

import { entityKeys } from './keys';
import {
  fetchProjectTaskDependencyReadiness,
  useProjectTaskDependencyReadinessQuery,
} from './project-task-dependency-readiness';

const taskA = '11111111-1111-4111-8111-111111111111';
const taskB = '22222222-2222-4222-8222-222222222222';
const rawReadiness: GetProjectTaskDependencyReadiness200Item[] = [
  {
    blockingTaskIds: [taskB],
    dependsOnTaskIds: [taskB],
    hasUnavailableDependencies: false,
    readiness: 'blocked',
    taskId: taskA,
  },
  {
    blockingTaskIds: [],
    dependsOnTaskIds: [],
    hasUnavailableDependencies: false,
    readiness: 'ready',
    taskId: taskA,
  },
];

beforeEach(() => {
  mocks.fetchWithToken.mockReset();
  queryClient.clear();
});

describe('project task dependency readiness storage client', () => {
  it('makes one exact POST and preserves the raw ordered duplicate response array', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(ok(rawReadiness));

    await expect(
      fetchProjectTaskDependencyReadiness('project-a', [taskA, taskA])
    ).resolves.toEqual(rawReadiness);

    expect(mocks.fetchWithToken).toHaveBeenCalledTimes(1);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/v2/projects/project-a/task-dependency-readiness',
      { method: 'POST', body: JSON.stringify({ taskIds: [taskA, taskA] }) }
    );
  });

  it('sends and returns an empty raw array for a present project with no task IDs', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(ok([]));

    await expect(
      fetchProjectTaskDependencyReadiness('project-a', [])
    ).resolves.toEqual([]);

    expect(mocks.fetchWithToken).toHaveBeenCalledTimes(1);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/v2/projects/project-a/task-dependency-readiness',
      { method: 'POST', body: JSON.stringify({ taskIds: [] }) }
    );
  });

  it('validates malformed and over-limit task IDs before making a request', async () => {
    await expect(
      fetchProjectTaskDependencyReadiness('project-a', ['not-a-uuid'])
    ).rejects.toThrow();
    await expect(
      fetchProjectTaskDependencyReadiness(
        'project-a',
        Array.from({ length: 201 }, () => taskA)
      )
    ).rejects.toThrow();

    expect(mocks.fetchWithToken).not.toHaveBeenCalled();

    mocks.fetchWithToken.mockResolvedValueOnce(ok(rawReadiness));
    const maxTaskIds = Array.from({ length: 200 }, () => taskA);
    await expect(
      fetchProjectTaskDependencyReadiness('project-a', maxTaskIds)
    ).resolves.toEqual(rawReadiness);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/v2/projects/project-a/task-dependency-readiness',
      { method: 'POST', body: JSON.stringify({ taskIds: maxTaskIds }) }
    );
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

    await expect(
      fetchProjectTaskDependencyReadiness('project-a', [taskA])
    ).rejects.toMatchObject({ errors });
    mocks.fetchWithToken.mockResolvedValueOnce(err(errors));
    await expect(
      fetchProjectTaskDependencyReadiness('project-a', [taskA])
    ).rejects.toBeInstanceOf(ThrownResultError);
  });
});

describe('project task dependency readiness queries', () => {
  it('uses stable exact entity keys that preserve project, order, and duplicates', () => {
    const orderedTaskIds = [taskA, taskB, taskA];
    const key = entityKeys.projectTaskDependencyReadiness(
      'project-a',
      orderedTaskIds
    ).queryKey;

    expect(key).toEqual([
      'entity',
      'projectTaskDependencyReadiness',
      'project-a',
      orderedTaskIds,
    ]);
    expect(key).toEqual(
      entityKeys.projectTaskDependencyReadiness('project-a', [
        taskA,
        taskB,
        taskA,
      ]).queryKey
    );
    expect(key).not.toEqual(
      entityKeys.projectTaskDependencyReadiness('project-a', [
        taskB,
        taskA,
        taskA,
      ]).queryKey
    );
    expect(key).not.toEqual(
      entityKeys.projectTaskDependencyReadiness('project-a', [taskA, taskB])
        .queryKey
    );
    expect(key).not.toEqual(
      entityKeys.projectTaskDependencyReadiness('project-b', [
        taskA,
        taskB,
        taskA,
      ]).queryKey
    );
  });

  it('disables nullish and empty project IDs without a request', () => {
    for (const value of [null, undefined, '']) {
      createRoot((dispose) => {
        const [id] = createSignal<string | null | undefined>(value);
        const [ids] = createSignal<readonly string[]>([taskA]);
        let query!: ReturnType<typeof useProjectTaskDependencyReadinessQuery>;
        QueryClientProvider({
          client: queryClient,
          get children() {
            query = useProjectTaskDependencyReadinessQuery(id, ids);
            return null;
          },
        });

        expect(query.isEnabled).toBe(false);
        expect(mocks.fetchWithToken).not.toHaveBeenCalled();
        dispose();
      });
    }
  });

  it('enables a present project with empty task IDs', () => {
    createRoot((dispose) => {
      const [id] = createSignal('project-a');
      const [ids] = createSignal<readonly string[]>([]);
      let query!: ReturnType<typeof useProjectTaskDependencyReadinessQuery>;
      QueryClientProvider({
        client: queryClient,
        get children() {
          query = useProjectTaskDependencyReadinessQuery(id, ids);
          return null;
        },
      });

      expect(query.isEnabled).toBe(true);
      dispose();
    });
  });
});
