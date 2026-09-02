import { PROPERTY_OPTION_IDS, SYSTEM_PROPERTY_IDS } from '@property/constants';
import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  createDecision: vi.fn(),
  createTask: vi.fn(),
  invalidateProjectOverviews: vi.fn(async () => {}),
  invalidateUserQuota: vi.fn(),
  refetchSoupEntity: vi.fn(),
  seedDocumentLoadBundle: vi.fn(),
  setPreviewOnCreate: vi.fn(),
  track: vi.fn(),
}));

vi.mock('@app/lib/analytics', () => ({
  analytics: { track: mocks.track },
}));
vi.mock('@block-chat/definition', () => ({
  DEFAULT_CHAT_NAME: 'New chat',
}));
vi.mock('@queries/auth', () => ({
  invalidateUserQuota: mocks.invalidateUserQuota,
}));
vi.mock('@queries/history/history', () => ({
  postNewHistoryItem: vi.fn(),
}));
vi.mock('@queries/preview/preview', () => ({
  setPreviewOnCreate: mocks.setPreviewOnCreate,
}));
vi.mock('@queries/soup/cache', () => ({
  initSoupNormalizer: vi.fn(),
  refetchSoupEntity: mocks.refetchSoupEntity,
}));
vi.mock('@queries/storage/documentLoad/documentLoadBundle', () => ({
  seedDocumentLoadBundle: mocks.seedDocumentLoadBundle,
}));
vi.mock('@queries/storage/project-overview', () => ({
  invalidateProjectOverviews: mocks.invalidateProjectOverviews,
}));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: {
    createDecision: mocks.createDecision,
    createTask: mocks.createTask,
  },
}));

import { createDecision, createTask } from './create';

const createdTask = {
  documentId: 'task-project-a',
  documentMetadata: { id: 'task-project-a' },
  token: 'task-token',
  initialSnapshot: undefined,
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.invalidateProjectOverviews.mockResolvedValue(undefined);
});

describe('createTask project overview invalidation', () => {
  it('creates a project Task with the canonical default status and invalidates only that overview after success', async () => {
    mocks.createTask.mockResolvedValueOnce(ok(createdTask));

    await expect(
      createTask({
        title: 'Project task',
        content: 'Task body',
        projectId: 'project-a',
      })
    ).resolves.toBe(createdTask.documentId);

    expect(mocks.createTask).toHaveBeenCalledOnce();
    expect(mocks.createTask).toHaveBeenCalledWith({
      taskName: 'Project task',
      markdown: 'Task body',
      projectId: 'project-a',
      propertyValues: [
        {
          propertyId: SYSTEM_PROPERTY_IDS.STATUS,
          value: {
            type: 'select_option',
            option_id: PROPERTY_OPTION_IDS.STATUS.NOT_STARTED,
          },
        },
      ],
    });
    expect(mocks.seedDocumentLoadBundle).toHaveBeenCalledOnce();
    expect(mocks.seedDocumentLoadBundle).toHaveBeenCalledWith(
      createdTask.documentId,
      {
        documentMetadata: createdTask.documentMetadata,
        userAccessLevel: 'owner',
        token: createdTask.token,
      }
    );
    expect(mocks.refetchSoupEntity).toHaveBeenCalledTimes(1);
    expect(mocks.refetchSoupEntity).toHaveBeenCalledWith(
      createdTask.documentId,
      'document',
      { ownTouch: true }
    );
    expect(mocks.invalidateProjectOverviews).toHaveBeenCalledTimes(1);
    expect(mocks.invalidateProjectOverviews).toHaveBeenCalledWith('project-a');
    expect(mocks.invalidateProjectOverviews).not.toHaveBeenCalledWith(
      'project-b'
    );

    const createOrder = mocks.createTask.mock.invocationCallOrder[0];
    expect(
      mocks.seedDocumentLoadBundle.mock.invocationCallOrder[0]
    ).toBeGreaterThan(createOrder);
    expect(mocks.refetchSoupEntity.mock.invocationCallOrder[0]).toBeGreaterThan(
      createOrder
    );
    expect(
      mocks.invalidateProjectOverviews.mock.invocationCallOrder[0]
    ).toBeGreaterThan(createOrder);
  });

  it('returns the Task id for a personal Task without invalidating any project overview', async () => {
    mocks.createTask.mockResolvedValueOnce(ok(createdTask));

    await expect(createTask({ title: 'Personal task' })).resolves.toBe(
      createdTask.documentId
    );

    expect(mocks.invalidateProjectOverviews).not.toHaveBeenCalled();
    expect(mocks.seedDocumentLoadBundle).toHaveBeenCalledOnce();
    expect(mocks.refetchSoupEntity).toHaveBeenCalledOnce();
  });

  it('returns undefined and does not start create side effects when the service rejects the Task', async () => {
    mocks.createTask.mockResolvedValueOnce(
      err([{ code: 'FORBIDDEN', message: 'missing access' }])
    );

    await expect(
      createTask({ title: 'Rejected task', projectId: 'project-a' })
    ).resolves.toBeUndefined();

    expect(mocks.invalidateProjectOverviews).not.toHaveBeenCalled();
    expect(mocks.seedDocumentLoadBundle).not.toHaveBeenCalled();
    expect(mocks.refetchSoupEntity).not.toHaveBeenCalled();
    expect(mocks.setPreviewOnCreate).not.toHaveBeenCalled();
  });
});

describe('createDecision', () => {
  it('creates a project-scoped Decision and seeds the Decision preview', async () => {
    mocks.createDecision.mockResolvedValueOnce(
      ok({ documentId: 'decision-project-a' })
    );

    await expect(
      createDecision({
        title: 'Adopt event sourcing',
        content: '# Context',
        projectId: 'project-a',
        source: 'project-overview',
      })
    ).resolves.toBe('decision-project-a');

    expect(mocks.createDecision).toHaveBeenCalledWith({
      decisionName: 'Adopt event sourcing',
      markdown: '# Context',
      projectId: 'project-a',
    });
    expect(mocks.setPreviewOnCreate).toHaveBeenCalledWith({
      itemId: 'decision-project-a',
      itemType: 'document',
      name: 'Adopt event sourcing',
      fileType: 'md',
      subType: { type: 'decision' },
    });
    expect(mocks.refetchSoupEntity).toHaveBeenCalledWith(
      'decision-project-a',
      'document',
      { ownTouch: true }
    );
    expect(mocks.invalidateProjectOverviews).toHaveBeenCalledWith('project-a');
    expect(mocks.track).toHaveBeenCalledWith('create_entity', {
      entityType: 'decision',
      entityId: 'decision-project-a',
      projectId: 'project-a',
      source: 'project-overview',
    });
  });

  it('does not seed client state when Decision creation fails', async () => {
    mocks.createDecision.mockResolvedValueOnce(
      err([{ code: 'FORBIDDEN', message: 'missing access' }])
    );

    await expect(
      createDecision({ title: 'Denied', projectId: 'project-a' })
    ).resolves.toBeUndefined();

    expect(mocks.setPreviewOnCreate).not.toHaveBeenCalled();
    expect(mocks.refetchSoupEntity).not.toHaveBeenCalled();
    expect(mocks.invalidateProjectOverviews).not.toHaveBeenCalled();
    expect(mocks.track).not.toHaveBeenCalled();
  });
});
