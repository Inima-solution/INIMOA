import { ThrownResultError } from '@core/util/result';
import { queryClient } from '@queries/client';
import type { GetProjectOverview200DataOneOf } from '@service-storage/generated/schemas';
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
  fetchProjectOverview,
  invalidateProjectOverviews,
  useProjectOverviewQuery,
} from './project-overview';

const overview: GetProjectOverview200DataOneOf = {
  immediateChildren: {
    chats: 2,
    childProjects: 1,
    nonTaskDocuments: 3,
    tasks: 4,
  },
  progress: {
    completedTasks: 1,
    wipTasks: 2,
    hasUnavailableStatuses: false,
    includedTasks: 4,
  },
  operations: {
    completedAt: null,
    createdAt: '2026-08-28T00:00:00.000Z',
    leadUserId: 'lead-a',
    policy: { escalation: { owners: ['lead-a'], windowHours: 24 } },
    priority: 'high',
    projectId: 'project-a',
    startDate: '2026-08-28',
    status: 'active',
    targetDate: '2026-09-28',
    updatedAt: '2026-08-28T01:00:00.000Z',
  },
  project: {
    createdAt: '2026-08-28T00:00:00.000Z',
    deletedAt: null,
    id: 'project-a',
    name: 'Project A',
    parentId: null,
    updatedAt: '2026-08-28T01:00:00.000Z',
    userId: 'user-a',
  },
  risk: {
    atRiskMilestones: 0,
    approachingTarget: false,
    blockedTasks: 0,
    hasUnavailableRiskData: false,
    openMilestones: 0,
    overdueTasks: 0,
    unassignedTasks: 0,
  },
  userAccessLevel: 'owner',
};

beforeEach(() => {
  mocks.fetchWithToken.mockReset();
  queryClient.clear();
});

describe('project overview storage client', () => {
  it('maps result.data from exactly one GET request and preserves free-form policy', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(
      ok({ data: overview, error: false })
    );

    await expect(
      storageServiceClient.projects.getOverview({
        asOfDate: '2026-09-01',
        id: 'project-a',
      })
    ).resolves.toEqual(ok(overview));

    expect(mocks.fetchWithToken).toHaveBeenCalledTimes(1);
    expect(mocks.fetchWithToken).toHaveBeenCalledWith(
      'http://dss.test/v2/projects/project-a/overview?asOfDate=2026-09-01',
      { method: 'GET' }
    );
  });
});

describe('project overview queries', () => {
  it('uses exact entity-scoped, distinct keys and disables nullish ids without a request', () => {
    expect(entityKeys.projectOverview('project-a').queryKey).toEqual([
      'entity',
      'projectOverview',
      'project-a',
    ]);
    expect(entityKeys.projectOverview('project-a').queryKey).not.toEqual(
      entityKeys.projectOverview('project-b').queryKey
    );

    for (const value of [null, undefined]) {
      createRoot((dispose) => {
        const [id] = createSignal<string | null | undefined>(value);
        let query!: ReturnType<typeof useProjectOverviewQuery>;
        QueryClientProvider({
          client: queryClient,
          get children() {
            query = useProjectOverviewQuery(id);
            return null;
          },
        });

        expect(query.isEnabled).toBe(false);
        expect(mocks.fetchWithToken).not.toHaveBeenCalled();
        dispose();
      });
    }
  });

  it('uses the exact local calendar date in the hook request and cache-key leaf', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-01T00:30:00.000Z'));
    mocks.fetchWithToken.mockResolvedValue(
      ok({ data: overview, error: false })
    );

    const now = new Date();
    const asOfDate = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    const key = [...entityKeys.projectOverview('project-a').queryKey, asOfDate];
    let dispose!: () => void;
    let query!: ReturnType<typeof useProjectOverviewQuery>;

    try {
      createRoot((rootDispose) => {
        dispose = rootDispose;
        const [id] = createSignal<string | null | undefined>('project-a');
        QueryClientProvider({
          client: queryClient,
          get children() {
            query = useProjectOverviewQuery(id);
            return null;
          },
        });
      });

      expect(queryClient.getQueryState(key)).toBeDefined();
      await query.refetch();
      expect(mocks.fetchWithToken).toHaveBeenLastCalledWith(
        `http://dss.test/v2/projects/project-a/overview?asOfDate=${asOfDate}`,
        { method: 'GET' }
      );
    } finally {
      dispose();
      vi.useRealTimers();
    }
  });

  it('returns the overview and throws typed Result errors unchanged', async () => {
    mocks.fetchWithToken.mockResolvedValueOnce(
      ok({ data: overview, error: false })
    );
    await expect(
      fetchProjectOverview('project-a', '2026-09-01')
    ).resolves.toEqual(overview);

    const errors = [{ code: 'FORBIDDEN', message: 'missing access' }];
    mocks.fetchWithToken.mockResolvedValueOnce(err(errors));
    await expect(fetchProjectOverview('project-a')).rejects.toEqual(
      expect.objectContaining({ errors })
    );
    await expect(
      (async () => {
        mocks.fetchWithToken.mockResolvedValueOnce(err(errors));
        await fetchProjectOverview('project-a', '2026-09-01');
      })()
    ).rejects.toBeInstanceOf(ThrownResultError);
  });

  it('invalidates every date variant for only the requested project overview cache', async () => {
    const overviewAKey = [
      ...entityKeys.projectOverview('project-a').queryKey,
      '2026-08-31',
    ];
    const overviewANextDayKey = [
      ...entityKeys.projectOverview('project-a').queryKey,
      '2026-09-01',
    ];
    const overviewBKey = [
      ...entityKeys.projectOverview('project-b').queryKey,
      '2026-09-01',
    ];
    const operationsAKey = entityKeys.projectOperations('project-a').queryKey;
    const documentMetadataKey =
      entityKeys.documentMetadata('document-a').queryKey;

    queryClient.setQueryData(overviewAKey, overview);
    queryClient.setQueryData(overviewANextDayKey, overview);
    queryClient.setQueryData(overviewBKey, {
      ...overview,
      project: { ...overview.project, id: 'project-b' },
    });
    queryClient.setQueryData(operationsAKey, overview.operations);
    queryClient.setQueryData(documentMetadataKey, { id: 'document-a' });

    await invalidateProjectOverviews('project-a');

    expect(queryClient.getQueryState(overviewAKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(overviewANextDayKey)?.isInvalidated).toBe(
      true
    );
    expect(queryClient.getQueryState(overviewBKey)?.isInvalidated).toBe(false);
    expect(queryClient.getQueryState(operationsAKey)?.isInvalidated).toBe(
      false
    );
    expect(queryClient.getQueryState(documentMetadataKey)?.isInvalidated).toBe(
      false
    );
  });

  it('invalidates every project overview without invalidating other entity caches', async () => {
    const overviewAKey = entityKeys.projectOverview('project-a').queryKey;
    const overviewBKey = entityKeys.projectOverview('project-b').queryKey;
    const operationsAKey = entityKeys.projectOperations('project-a').queryKey;
    const documentMetadataKey =
      entityKeys.documentMetadata('document-a').queryKey;

    queryClient.setQueryData(overviewAKey, overview);
    queryClient.setQueryData(overviewBKey, {
      ...overview,
      project: { ...overview.project, id: 'project-b' },
    });
    queryClient.setQueryData(operationsAKey, overview.operations);
    queryClient.setQueryData(documentMetadataKey, { id: 'document-a' });

    await invalidateProjectOverviews();

    expect(queryClient.getQueryState(overviewAKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(overviewBKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(operationsAKey)?.isInvalidated).toBe(
      false
    );
    expect(queryClient.getQueryState(documentMetadataKey)?.isInvalidated).toBe(
      false
    );
  });
});
