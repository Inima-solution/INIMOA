import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  createChat: vi.fn(),
  openWithSplit: vi.fn(),
  storeChatStateImmediate: vi.fn(),
  toastFailure: vi.fn(),
}));

vi.mock('@app/signal/splitLayout', () => ({
  globalSplitManager: () => ({ openWithSplit: mocks.openWithSplit }),
}));
vi.mock('@core/component/AI/signal/pendingSend', () => ({
  setPendingSendData: vi.fn(),
}));
vi.mock('@core/component/LexicalMarkdown/plugins/mentions', () => ({
  INSERT_DOCUMENT_MENTION_COMMAND: {},
}));
vi.mock('@core/component/AI/util/storage', () => ({
  storeChatStateImmediate: mocks.storeChatStateImmediate,
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { failure: mocks.toastFailure },
}));
vi.mock('@core/constant/allBlocks', () => ({
  fileTypeToBlockName: (fileType: string | null | undefined) =>
    fileType ?? 'unknown',
}));
vi.mock('@core/util/create', () => ({
  createChat: mocks.createChat,
}));
vi.mock('@icon/wide-star', () => ({
  AnimatedStarIcon: () => null,
}));
vi.mock('@ui', () => ({
  Button: () => null,
}));

import { openChatWithAgent } from './ChatWithAgentButton';

describe('openChatWithAgent', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.createChat.mockResolvedValue({ chatId: 'chat-id' });
  });

  it('seeds and opens a new chat with a visible mention and attachment', async () => {
    await openChatWithAgent({
      type: 'document',
      id: 'document-id',
      name: 'Project plan',
      fileType: 'md',
    });

    expect(mocks.storeChatStateImmediate).toHaveBeenCalledWith('chat-id', {
      input:
        '<m-document-mention>{"documentId":"document-id","documentName":"Project plan","blockName":"md","blockParams":{}}</m-document-mention>',
      attachments: [{ entity_id: 'document-id', entity_type: 'document' }],
    });
    expect(mocks.openWithSplit).toHaveBeenCalledWith(
      { type: 'chat', id: 'chat-id' },
      { activate: true, preferNewSplit: true }
    );
  });

  it('creates an unscoped chat for a project and stores its canonical mention', async () => {
    await openChatWithAgent({
      type: 'project',
      id: 'project-id',
      name: 'Roadmap',
    });

    expect(mocks.createChat).toHaveBeenCalledTimes(1);
    expect(mocks.createChat).toHaveBeenCalledWith();
    expect(mocks.storeChatStateImmediate).toHaveBeenCalledWith('chat-id', {
      input:
        '<m-document-mention>{"documentId":"project-id","documentName":"Roadmap","blockName":"project","blockParams":{}}</m-document-mention>',
      attachments: [{ entity_id: 'project-id', entity_type: 'project' }],
    });
    expect(mocks.openWithSplit).toHaveBeenCalledTimes(1);
    expect(mocks.openWithSplit).toHaveBeenCalledWith(
      { type: 'chat', id: 'chat-id' },
      { activate: true, preferNewSplit: true }
    );
  });

  it('keeps project chat creation failure generic without seeding or opening', async () => {
    mocks.createChat.mockResolvedValue({ error: 'backend detail' });

    await openChatWithAgent({
      type: 'project',
      id: 'project-id',
      name: 'Roadmap',
    });

    expect(mocks.toastFailure).toHaveBeenCalledTimes(1);
    expect(mocks.toastFailure).toHaveBeenCalledWith('Unable to start chat');
    expect(mocks.storeChatStateImmediate).not.toHaveBeenCalled();
    expect(mocks.openWithSplit).not.toHaveBeenCalled();
  });
});
