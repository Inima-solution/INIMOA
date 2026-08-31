import { queryClient } from '@queries/client';
import type { Project } from '@service-storage/generated/schemas/project';
import { QueryClientProvider } from '@tanstack/solid-query';
import { ok } from 'neverthrow';
import { createRoot } from 'solid-js';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getAll: vi.fn(),
  getPending: vi.fn(),
  projectSharing: false,
  userId: 'user-a',
}));

vi.mock('@app/lib/analytics', () => ({
  analytics: { track: vi.fn() },
}));
vi.mock('@core/constant/featureFlags', () => ({
  get ENABLE_PROJECT_SHARING() {
    return mocks.projectSharing;
  },
}));
vi.mock('@core/context/user', () => ({
  useUserId: () => () => mocks.userId,
}));
vi.mock('@queries/history/history', () => ({
  refetchHistory: vi.fn(),
  useUpsertToHistoryMutation: () => ({ mutate: vi.fn() }),
}));
vi.mock('@queries/preview/preview', () => ({
  setPreviewOnCreate: vi.fn(),
}));
vi.mock('@queries/utils', () => ({
  withCallbacks: (options: unknown) => options,
}));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: {
    projects: {
      getAll: mocks.getAll,
      getPending: mocks.getPending,
    },
  },
}));
vi.mock('./project-overview', () => ({
  invalidateProjectOverviews: vi.fn(),
}));

import { useProjectsQuery } from './projects';

const project = (
  id: string,
  createdAt: string,
  userId = 'user-a'
): Project => ({
  createdAt,
  deletedAt: null,
  id,
  name: id,
  parentId: null,
  type: 'project',
  updatedAt: createdAt,
  userId,
});

beforeEach(() => {
  queryClient.clear();
  mocks.getAll.mockReset();
  mocks.getPending.mockReset();
  mocks.projectSharing = false;
  mocks.userId = 'user-a';
});

async function readProjects(): Promise<Project[]> {
  let dispose!: () => void;
  let query!: ReturnType<typeof useProjectsQuery>;
  createRoot((rootDispose) => {
    dispose = rootDispose;
    QueryClientProvider({
      client: queryClient,
      get children() {
        query = useProjectsQuery();
        return null;
      },
    });
  });
  await vi.waitFor(() => expect(query.data).toBeDefined());
  const data = query.data!;
  dispose();
  return data;
}

describe('useProjectsQuery', () => {
  it('orders merged projects by creation time then id without mutating inputs', async () => {
    const normal = [
      project('project-b', '2026-08-30T00:00:00.000Z'),
      project('project-new', '2026-08-31T00:00:00.000Z', 'other-user'),
    ];
    const pending = [
      project('project-a', '2026-08-30T00:00:00.000Z'),
      project('project-old', '2026-08-29T00:00:00.000Z'),
    ];
    const normalBefore = structuredClone(normal);
    const pendingBefore = structuredClone(pending);
    mocks.getAll.mockResolvedValue(ok({ data: normal }));
    mocks.getPending.mockResolvedValue(ok({ data: pending }));

    const result = await readProjects();

    expect(result.map(({ id }) => id)).toEqual([
      'project-a',
      'project-b',
      'project-old',
    ]);
    expect(normal).toEqual(normalBefore);
    expect(pending).toEqual(pendingBefore);
  });

  it('keeps other owners when project sharing is enabled', async () => {
    mocks.projectSharing = true;
    mocks.getAll.mockResolvedValue(
      ok({
        data: [
          project('project-b', '2026-08-30T00:00:00.000Z'),
          project('project-a', '2026-08-30T00:00:00.000Z', 'other-user'),
        ],
      })
    );
    mocks.getPending.mockResolvedValue(
      ok({ data: [project('project-old', '2026-08-29T00:00:00.000Z')] })
    );

    const result = await readProjects();

    expect(result.map(({ id }) => id)).toEqual([
      'project-a',
      'project-b',
      'project-old',
    ]);
  });
});
