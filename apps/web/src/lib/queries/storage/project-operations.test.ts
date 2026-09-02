import { ThrownResultError } from '@core/util/result';
import { queryClient } from '@queries/client';
import type {
  GetProjectOverview200DataOneOf,
  ProjectOperations,
  ReplaceProjectOperationsRequest,
} from '@service-storage/generated/schemas';
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

import { storageServiceClient } from '@service-storage/client';
import { entityKeys } from './keys';
import {
  fetchProjectOperations,
  useProjectOperationsQuery,
  useReplaceProjectOperationsMutation,
} from './project-operations';

const projectA: ProjectOperations = {
  completedAt: null,
  createdAt: '2026-08-28T00:00:00.000Z',
  leadUserId: 'lead-a',
  policy: { review: 'weekly' },
  priority: 'normal',
  projectId: 'project-a',
  startDate: '2026-08-28',
  status: 'active',
  targetDate: '2026-09-28',
  updatedAt: '2026-08-28T01:00:00.000Z',
};

const projectB: ProjectOperations = {
  ...projectA,
  projectId: 'project-b',
  priority: 'high',
};

const replacement: ReplaceProjectOperationsRequest = {
  expectedUpdatedAt: projectA.updatedAt,
  leadUserId: null,
  policy: null,
  priority: 'urgent',
  startDate: null,
  status: 'paused',
  targetDate: null,
};

const projectOverviewA: GetProjectOverview200DataOneOf = {
  immediateChildren: {
    chats: 0,
    childProjects: 0,
    nonTaskDocuments: 0,
    tasks: 0,
  },
  operations: projectA,
  progress: {
    completedTasks: 0,
    hasUnavailableStatuses: false,
    includedTasks: 0,
  },
  project: {
    createdAt: '2026-08-28T00:00:00.000Z',
    deletedAt: null,
    id: projectA.projectId,
    name: 'Project A',
    parentId: null,
    updatedAt: '2026-08-28T01:00:00.000Z',
    userId: 'user-a',
  },
  risk: {
    approachingTarget: false,
    blockedTasks: 0,
    hasUnavailableRiskData: false,
    overdueTasks: 0,
    unassignedTasks: 0,
  },
  userAccessLevel: 'owner',
};

const projectOverviewB: GetProjectOverview200DataOneOf = {
  ...projectOverviewA,
  operations: projectB,
  project: { ...projectOverviewA.project, id: projectB.projectId },
};

beforeEach(() => {
  mocks.fetchWithToken.mockReset();
  queryClient.clear();
});

describe('project operations storage client', () => {
  it('maps data from exactly one GET and one full-body PUT request', async () => {
    const canonical: ProjectOperations = {
      ...projectA,
      leadUserId: replacement.leadUserId,
      policy: replacement.policy,
      priority: replacement.priority,
      startDate: replacement.startDate,
      status: replacement.status,
      targetDate: replacement.targetDate,
      updatedAt: '2026-08-28T02:00:00.000Z',
    };
    mocks.fetchWithToken
      .mockResolvedValueOnce(ok({ data: projectA, error: false }))
      .mockResolvedValueOnce(ok({ data: canonical, error: false }));

    await expect(
      storageServiceClient.projects.getOperations({ id: projectA.projectId })
    ).resolves.toEqual(ok(projectA));
    await expect(
      storageServiceClient.projects.replaceOperations({
        id: projectA.projectId,
        ...replacement,
      })
    ).resolves.toEqual(ok(canonical));

    expect(mocks.fetchWithToken).toHaveBeenCalledTimes(2);
    const operationsUrl = `http://dss.test/v2/projects/${projectA.projectId}/operations`;
    expect(mocks.fetchWithToken).toHaveBeenNthCalledWith(1, operationsUrl, {
      method: 'GET',
    });
    expect(mocks.fetchWithToken).toHaveBeenNthCalledWith(2, operationsUrl, {
      method: 'PUT',
      body: JSON.stringify(replacement),
    });
  });
});

describe('project operations queries', () => {
  it('uses entity-scoped stable, distinct keys and disables undefined ids', () => {
    expect(entityKeys.projectOperations('project-a').queryKey).toEqual([
      'entity',
      'projectOperations',
      'project-a',
    ]);
    expect(entityKeys.projectOperations('project-a').queryKey).not.toEqual(
      entityKeys.projectOperations('project-b').queryKey
    );

    createRoot((dispose) => {
      const [id] = createSignal<string | undefined>(undefined);
      let query!: ReturnType<typeof useProjectOperationsQuery>;
      QueryClientProvider({
        client: queryClient,
        get children() {
          query = useProjectOperationsQuery(id);
          return null;
        },
      });

      expect(query.isEnabled).toBe(false);
      expect(mocks.fetchWithToken).not.toHaveBeenCalled();
      dispose();
    });
  });

  it('returns the canonical read object and preserves result error codes', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(
      ok({ data: projectA, error: false })
    );
    await expect(fetchProjectOperations(projectA.projectId)).resolves.toEqual(
      projectA
    );

    mocks.fetchWithToken.mockResolvedValueOnce(
      err([{ code: 'CONFLICT', message: 'version conflict' }])
    );
    await expect(fetchProjectOperations(projectA.projectId)).rejects.toEqual(
      expect.objectContaining({
        errors: [{ code: 'CONFLICT', message: 'version conflict' }],
      })
    );
  });

  it('replaces the successful project operations cache and invalidates only its overview', async () => {
    const canonical: ProjectOperations = {
      ...projectA,
      leadUserId: replacement.leadUserId,
      policy: replacement.policy,
      priority: replacement.priority,
      startDate: replacement.startDate,
      status: replacement.status,
      targetDate: replacement.targetDate,
      updatedAt: '2026-08-28T03:00:00.000Z',
    };
    const aKey = entityKeys.projectOperations(projectA.projectId).queryKey;
    const bKey = entityKeys.projectOperations(projectB.projectId).queryKey;
    const overviewAKey = [
      ...entityKeys.projectOverview(projectA.projectId).queryKey,
      '2026-09-03',
    ] as const;
    const overviewBKey = [
      ...entityKeys.projectOverview(projectB.projectId).queryKey,
      '2026-09-03',
    ] as const;
    queryClient.setQueryData(aKey, projectA);
    queryClient.setQueryData(bKey, projectB);
    queryClient.setQueryData(overviewAKey, projectOverviewA);
    queryClient.setQueryData(overviewBKey, projectOverviewB);
    mocks.fetchWithToken.mockResolvedValueOnce(
      ok({ data: canonical, error: false })
    );

    await createRoot(async (dispose) => {
      let mutation!: ReturnType<typeof useReplaceProjectOperationsMutation>;
      QueryClientProvider({
        client: queryClient,
        get children() {
          mutation = useReplaceProjectOperationsMutation();
          return null;
        },
      });
      await mutation.mutateAsync({
        projectId: projectA.projectId,
        request: replacement,
      });
      dispose();
    });

    expect(queryClient.getQueryData(aKey)).toEqual(canonical);
    expect(queryClient.getQueryData(bKey)).toEqual(projectB);
    expect(queryClient.getQueryData(overviewAKey)).toEqual(projectOverviewA);
    expect(queryClient.getQueryData(overviewBKey)).toEqual(projectOverviewB);
    expect(queryClient.getQueryState(overviewAKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(overviewBKey)?.isInvalidated).toBe(false);
    const operationsUrl = `http://dss.test/v2/projects/${projectA.projectId}/operations`;
    expect(mocks.fetchWithToken).toHaveBeenLastCalledWith(operationsUrl, {
      method: 'PUT',
      body: JSON.stringify(replacement),
    });
  });

  it('leaves operations and overview caches plus invalidation state unchanged on failure', async () => {
    const aKey = entityKeys.projectOperations(projectA.projectId).queryKey;
    const bKey = entityKeys.projectOperations(projectB.projectId).queryKey;
    const overviewAKey = [
      ...entityKeys.projectOverview(projectA.projectId).queryKey,
      '2026-09-03',
    ] as const;
    const overviewBKey = [
      ...entityKeys.projectOverview(projectB.projectId).queryKey,
      '2026-09-03',
    ] as const;
    queryClient.setQueryData(aKey, projectA);
    queryClient.setQueryData(bKey, projectB);
    queryClient.setQueryData(overviewAKey, projectOverviewA);
    queryClient.setQueryData(overviewBKey, projectOverviewB);
    expect(queryClient.getQueryState(overviewAKey)?.isInvalidated).toBe(false);
    expect(queryClient.getQueryState(overviewBKey)?.isInvalidated).toBe(false);

    mocks.fetchWithToken.mockResolvedValueOnce(
      err([{ code: 'CONFLICT', message: 'version conflict' }])
    );
    await createRoot(async (dispose) => {
      let mutation!: ReturnType<typeof useReplaceProjectOperationsMutation>;
      QueryClientProvider({
        client: queryClient,
        get children() {
          mutation = useReplaceProjectOperationsMutation();
          return null;
        },
      });
      await expect(
        mutation.mutateAsync({
          projectId: projectA.projectId,
          request: replacement,
        })
      ).rejects.toBeInstanceOf(ThrownResultError);
      dispose();
    });

    expect(queryClient.getQueryData(aKey)).toEqual(projectA);
    expect(queryClient.getQueryData(bKey)).toEqual(projectB);
    expect(queryClient.getQueryData(overviewAKey)).toEqual(projectOverviewA);
    expect(queryClient.getQueryData(overviewBKey)).toEqual(projectOverviewB);
    expect(queryClient.getQueryState(overviewAKey)?.isInvalidated).toBe(false);
    expect(queryClient.getQueryState(overviewBKey)?.isInvalidated).toBe(false);
  });
});
