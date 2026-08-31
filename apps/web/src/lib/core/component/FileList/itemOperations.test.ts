import type { AccessLevel } from '@service-storage/generated/schemas/accessLevel';
import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  accessLevel: 'owner' as AccessLevel | undefined,
  deleteChat: vi.fn(),
  deleteDocument: vi.fn(),
  editChatProject: vi.fn(),
  editDocument: vi.fn(),
  getChat: vi.fn(),
  getDocumentMetadata: vi.fn(),
  getProject: vi.fn(),
  projectDelete: vi.fn(),
  projectEdit: vi.fn(),
  projectRevertDelete: vi.fn(),
  refetchResources: vi.fn(),
  removeHistoryItem: vi.fn(),
  revertChatDelete: vi.fn(),
  revertDocumentDelete: vi.fn(),
  track: vi.fn(),
}));

vi.mock('@app/lib/analytics', () => ({
  analytics: { track: mocks.track },
}));
vi.mock('@core/constant/PaywallState', () => ({
  usePaywallState: vi.fn(),
}));
vi.mock('@core/util/handlePaymentError', () => ({
  isPaymentError: vi.fn(),
}));
vi.mock('@queries/history/history', () => ({
  removeHistoryItem: mocks.removeHistoryItem,
}));
vi.mock('@queries/storage/deleted', () => ({
  getDeletedTree: vi.fn(),
  optimisticallyRemoveDeletedItem: vi.fn(),
}));
vi.mock('@service-call/client', () => ({ callServiceClient: {} }));
vi.mock('@service-cognition/client', () => ({
  cognitionApiServiceClient: {
    deleteChat: mocks.deleteChat,
    editChatProject: mocks.editChatProject,
    getChat: mocks.getChat,
    revertDeleteChat: mocks.revertChatDelete,
  },
}));
vi.mock('@service-email/client', () => ({ emailClient: {} }));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: {
    deleteDocument: mocks.deleteDocument,
    editDocument: mocks.editDocument,
    getDocumentMetadata: mocks.getDocumentMetadata,
    projects: {
      delete: mocks.projectDelete,
      edit: mocks.projectEdit,
      getProject: mocks.getProject,
      revertDelete: mocks.projectRevertDelete,
    },
    revertDocumentDelete: mocks.revertDocumentDelete,
  },
}));
vi.mock('@service-storage/util/refetchResources', () => ({
  refetchResources: mocks.refetchResources,
}));

import { deleteItem, moveToFolder, revertDelete } from './itemOperations';

const itemTypes = ['document', 'project', 'chat'] as const;
const failure = err([{ code: 'FORBIDDEN', message: 'not allowed' }]);
const moveMethods = [
  mocks.editDocument,
  mocks.projectEdit,
  mocks.editChatProject,
];
const deleteMethods = [
  mocks.deleteDocument,
  mocks.projectDelete,
  mocks.deleteChat,
];
const restoreMethods = [
  mocks.revertDocumentDelete,
  mocks.projectRevertDelete,
  mocks.revertChatDelete,
];

function expectOnlyCanonicalTransport(
  calledMethod: ReturnType<typeof vi.fn>,
  operationMethods: ReturnType<typeof vi.fn>[]
) {
  expect(calledMethod).toHaveBeenCalledOnce();
  for (const method of operationMethods) {
    if (method !== calledMethod) expect(method).not.toHaveBeenCalled();
  }
}

function setAccessMetadata(accessLevel: AccessLevel | undefined) {
  mocks.accessLevel = accessLevel;
  mocks.getDocumentMetadata.mockResolvedValue(
    ok({ userAccessLevel: mocks.accessLevel })
  );
  mocks.getProject.mockResolvedValue(
    ok({ userAccessLevel: mocks.accessLevel })
  );
  mocks.getChat.mockResolvedValue(ok({ userAccessLevel: mocks.accessLevel }));
}

function expectNoMutationOrSuccessSideEffects() {
  expect(mocks.editDocument).not.toHaveBeenCalled();
  expect(mocks.projectEdit).not.toHaveBeenCalled();
  expect(mocks.editChatProject).not.toHaveBeenCalled();
  expect(mocks.track).not.toHaveBeenCalled();
  expect(mocks.refetchResources).not.toHaveBeenCalled();
}

beforeEach(() => {
  vi.clearAllMocks();
  setAccessMetadata('owner');
  for (const method of [
    mocks.deleteChat,
    mocks.deleteDocument,
    mocks.editChatProject,
    mocks.editDocument,
    mocks.projectDelete,
    mocks.projectEdit,
    mocks.projectRevertDelete,
    mocks.revertChatDelete,
    mocks.revertDocumentDelete,
  ]) {
    method.mockResolvedValue(ok(undefined));
  }
  mocks.removeHistoryItem.mockResolvedValue(true);
});

describe('itemOperations shared lifecycle transports', () => {
  it.each([
    ['document', 'document-1', 'folder-1', 'owner'],
    ['project', 'project-1', null, 'edit'],
    ['chat', 'chat-1', null, 'owner'],
  ] as const)(
    'moves editable %s with its canonical payload and refreshes once',
    async (itemType, id, folderId, accessLevel) => {
      setAccessMetadata(accessLevel);
      await expect(moveToFolder({ itemType, id, folderId })).resolves.toBe(
        true
      );

      if (itemType === 'document') {
        expect(mocks.editDocument).toHaveBeenCalledWith({
          documentId: id,
          projectId: folderId ?? '',
        });
        expectOnlyCanonicalTransport(mocks.editDocument, moveMethods);
      } else if (itemType === 'project') {
        expect(mocks.projectEdit).toHaveBeenCalledWith({
          id,
          projectParentId: '',
        });
        expectOnlyCanonicalTransport(mocks.projectEdit, moveMethods);
      } else {
        expect(mocks.editChatProject).toHaveBeenCalledWith({
          chat_id: id,
          project_id: '',
        });
        expectOnlyCanonicalTransport(mocks.editChatProject, moveMethods);
      }

      expect(mocks.track).toHaveBeenCalledOnce();
      expect(mocks.track).toHaveBeenCalledWith('update_entity', {
        entityType: itemType,
        entityId: id,
        property: 'parent_project',
        newProjectId: folderId,
      });
      expect(mocks.refetchResources).toHaveBeenCalledOnce();
    }
  );

  it.each([undefined, 'view'] as const)(
    'does not move missing or read-only access',
    async (accessLevel) => {
      setAccessMetadata(accessLevel);

      await expect(
        moveToFolder({
          itemType: 'document',
          id: 'document-1',
          folderId: 'p-1',
        })
      ).resolves.toBe(false);

      expectNoMutationOrSuccessSideEffects();
    }
  );

  it.each(itemTypes)(
    'does not refresh or track when %s move transport returns Err',
    async (itemType) => {
      if (itemType === 'document')
        mocks.editDocument.mockResolvedValueOnce(failure);
      if (itemType === 'project')
        mocks.projectEdit.mockResolvedValueOnce(failure);
      if (itemType === 'chat')
        mocks.editChatProject.mockResolvedValueOnce(failure);

      await expect(
        moveToFolder({ itemType, id: `${itemType}-1`, folderId: 'p-1' })
      ).resolves.toBe(false);

      if (itemType === 'document')
        expectOnlyCanonicalTransport(mocks.editDocument, moveMethods);
      if (itemType === 'project')
        expectOnlyCanonicalTransport(mocks.projectEdit, moveMethods);
      if (itemType === 'chat')
        expectOnlyCanonicalTransport(mocks.editChatProject, moveMethods);
      expect(mocks.track).not.toHaveBeenCalled();
      expect(mocks.refetchResources).not.toHaveBeenCalled();
    }
  );

  it.each(itemTypes)(
    'soft-deletes owner %s only through its canonical backend and refreshes once',
    async (itemType) => {
      const id = `${itemType}-1`;
      await expect(deleteItem({ itemType, id })).resolves.toBe(true);

      if (itemType === 'document')
        expect(mocks.deleteDocument).toHaveBeenCalledWith({ documentId: id });
      if (itemType === 'project')
        expect(mocks.projectDelete).toHaveBeenCalledWith({ id });
      if (itemType === 'chat')
        expect(mocks.deleteChat).toHaveBeenCalledWith({ chat_id: id });
      if (itemType === 'document')
        expectOnlyCanonicalTransport(mocks.deleteDocument, deleteMethods);
      if (itemType === 'project')
        expectOnlyCanonicalTransport(mocks.projectDelete, deleteMethods);
      if (itemType === 'chat')
        expectOnlyCanonicalTransport(mocks.deleteChat, deleteMethods);
      expect(mocks.removeHistoryItem).not.toHaveBeenCalled();
      expect(mocks.track).toHaveBeenCalledWith('delete_entity', {
        entityType: itemType,
        entityId: id,
        deleteType: 'soft',
      });
      expect(mocks.refetchResources).toHaveBeenCalledOnce();
    }
  );

  it.each(itemTypes)(
    'does not refresh or track when owner %s soft-delete returns Err',
    async (itemType) => {
      if (itemType === 'document')
        mocks.deleteDocument.mockResolvedValueOnce(failure);
      if (itemType === 'project')
        mocks.projectDelete.mockResolvedValueOnce(failure);
      if (itemType === 'chat') mocks.deleteChat.mockResolvedValueOnce(failure);

      await expect(deleteItem({ itemType, id: `${itemType}-1` })).resolves.toBe(
        false
      );

      if (itemType === 'document')
        expectOnlyCanonicalTransport(mocks.deleteDocument, deleteMethods);
      if (itemType === 'project')
        expectOnlyCanonicalTransport(mocks.projectDelete, deleteMethods);
      if (itemType === 'chat')
        expectOnlyCanonicalTransport(mocks.deleteChat, deleteMethods);
      expect(mocks.removeHistoryItem).not.toHaveBeenCalled();
      expect(mocks.track).not.toHaveBeenCalled();
      expect(mocks.refetchResources).not.toHaveBeenCalled();
    }
  );

  it.each(itemTypes)(
    'removes non-owner %s from history and refreshes only after removal',
    async (itemType) => {
      setAccessMetadata('view');
      const id = `${itemType}-1`;
      await expect(deleteItem({ itemType, id })).resolves.toBe(true);

      expect(mocks.removeHistoryItem).toHaveBeenCalledWith(itemType, id);
      expect(mocks.deleteDocument).not.toHaveBeenCalled();
      expect(mocks.projectDelete).not.toHaveBeenCalled();
      expect(mocks.deleteChat).not.toHaveBeenCalled();
      expect(mocks.refetchResources).toHaveBeenCalledOnce();
    }
  );

  it.each(itemTypes)(
    'does not refresh non-owner %s when history removal fails',
    async (itemType) => {
      setAccessMetadata('view');
      mocks.removeHistoryItem.mockResolvedValueOnce(false);

      await expect(deleteItem({ itemType, id: `${itemType}-1` })).resolves.toBe(
        false
      );

      expect(mocks.track).not.toHaveBeenCalled();
      expect(mocks.refetchResources).not.toHaveBeenCalled();
    }
  );

  it.each(itemTypes)(
    'restores %s only through its canonical backend and refreshes once',
    async (itemType) => {
      const id = `${itemType}-1`;
      await expect(revertDelete({ itemType, id })).resolves.toBe(true);

      if (itemType === 'document')
        expect(mocks.revertDocumentDelete).toHaveBeenCalledWith({
          documentId: id,
        });
      if (itemType === 'project')
        expect(mocks.projectRevertDelete).toHaveBeenCalledWith({ id });
      if (itemType === 'chat')
        expect(mocks.revertChatDelete).toHaveBeenCalledWith({ chat_id: id });
      if (itemType === 'document')
        expectOnlyCanonicalTransport(
          mocks.revertDocumentDelete,
          restoreMethods
        );
      if (itemType === 'project')
        expectOnlyCanonicalTransport(mocks.projectRevertDelete, restoreMethods);
      if (itemType === 'chat')
        expectOnlyCanonicalTransport(mocks.revertChatDelete, restoreMethods);
      expect(mocks.refetchResources).toHaveBeenCalledOnce();
      expect(mocks.track).not.toHaveBeenCalled();
    }
  );

  it.each(itemTypes)(
    'does not refresh when %s restore transport returns Err',
    async (itemType) => {
      if (itemType === 'document')
        mocks.revertDocumentDelete.mockResolvedValueOnce(failure);
      if (itemType === 'project')
        mocks.projectRevertDelete.mockResolvedValueOnce(failure);
      if (itemType === 'chat')
        mocks.revertChatDelete.mockResolvedValueOnce(failure);

      await expect(
        revertDelete({ itemType, id: `${itemType}-1` })
      ).resolves.toBe(false);

      if (itemType === 'document')
        expectOnlyCanonicalTransport(
          mocks.revertDocumentDelete,
          restoreMethods
        );
      if (itemType === 'project')
        expectOnlyCanonicalTransport(mocks.projectRevertDelete, restoreMethods);
      if (itemType === 'chat')
        expectOnlyCanonicalTransport(mocks.revertChatDelete, restoreMethods);
      expect(mocks.refetchResources).not.toHaveBeenCalled();
    }
  );

  it('rejects unsupported restore without a transport or refresh', async () => {
    await expect(
      revertDelete({ itemType: 'email', id: 'email-1' })
    ).resolves.toBe(false);

    expect(mocks.revertDocumentDelete).not.toHaveBeenCalled();
    expect(mocks.projectRevertDelete).not.toHaveBeenCalled();
    expect(mocks.revertChatDelete).not.toHaveBeenCalled();
    expect(mocks.refetchResources).not.toHaveBeenCalled();
  });
});
