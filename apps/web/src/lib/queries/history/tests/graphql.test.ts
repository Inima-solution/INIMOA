import type {
  CacheHost,
  ReadRecordsByKeysArgs,
  SearchCacheArgs,
  SearchCachePage,
} from '@graphql-cache/index';
import { INITIAL_CACHE_REVISION } from '@graphql-cache/index';
import { describe, expect, it, vi } from 'vitest';
import { readCachedGraphqlHistoryItems } from '../graphql';

function cacheHost(
  search: (args: SearchCacheArgs) => Promise<SearchCachePage>,
  readRecordsByKeys: (
    args: ReadRecordsByKeysArgs
  ) => Promise<Array<{ recordKey: string; record: unknown }>>
): Pick<CacheHost, 'search' | 'readRecordsByKeys'> {
  return {
    search,
    readRecordsByKeys: async (args) => ({
      revision: INITIAL_CACHE_REVISION,
      records: await readRecordsByKeys(args),
    }),
  };
}

describe('cached GraphQL history', () => {
  it('browses the indexed recent projection and materializes only final keys', async () => {
    const search = vi.fn(
      async (): Promise<SearchCachePage> => ({
        documents: [
          {
            profile: 'quick-access-v1',
            recordKey: 'GraphqlSoupDocument:document-newest',
            bucket: 'note',
            searchText: 'newest document',
            timestampMs: Date.parse('2025-01-05T00:00:00.000Z'),
            sourceHash: 'one',
          },
          {
            profile: 'quick-access-v1',
            recordKey: 'GraphqlSoupChat:chat-middle',
            bucket: 'chat',
            searchText: 'middle chat',
            timestampMs: Date.parse('2025-01-04T00:00:00.000Z'),
            sourceHash: 'two',
          },
          {
            profile: 'quick-access-v1',
            recordKey: 'GraphqlSoupDocument:document-task',
            bucket: 'task',
            searchText: 'task document',
            timestampMs: Date.parse('2025-01-03T00:00:00.000Z'),
            sourceHash: 'three',
          },
          {
            profile: 'quick-access-v1',
            recordKey: 'GraphqlSoupDocument:decision-latest',
            bucket: 'decision',
            searchText: 'architecture decision',
            timestampMs: Date.parse('2025-01-02T00:00:00.000Z'),
            sourceHash: 'four',
          },
        ],
        nextCursor: null,
      })
    );
    const readRecordsByKeys = vi.fn(async (args: ReadRecordsByKeysArgs) =>
      args.keys.map((recordKey) => {
        const isChat = recordKey === 'GraphqlSoupChat:chat-middle';
        const isTask = recordKey === 'GraphqlSoupDocument:document-task';
        const isDecision = recordKey === 'GraphqlSoupDocument:decision-latest';
        return {
          recordKey,
          record: {
            __typename: isChat ? 'GraphqlSoupChat' : 'GraphqlSoupDocument',
            name:
              recordKey === 'GraphqlSoupDocument:document-newest'
                ? 'Newest Document'
                : isChat
                  ? 'Middle Chat'
                  : 'Task Document',
            ownerId: isChat ? 'chat-owner' : 'document-owner',
            ...(!isChat && {
              projectId: isDecision ? 'project-1' : null,
            }),
            createdAt: isChat
              ? '2025-01-01T00:00:00.000Z'
              : '2024-12-31T00:00:00.000Z',
            ...(!isChat && {
              subType: isTask
                ? { __typename: 'GraphqlTaskSubType', isCompleted: true }
                : isDecision
                  ? { __typename: 'GraphqlDecisionSubType' }
                  : null,
            }),
          },
        };
      })
    );

    const result = await readCachedGraphqlHistoryItems(
      cacheHost(search, readRecordsByKeys)
    );

    expect(search).toHaveBeenCalledWith({
      profile: 'quick-access-v1',
      buckets: [
        'document',
        'note',
        'task',
        'snippet',
        'skill',
        'decision',
        'chat',
        'project',
      ],
      query: '',
      limit: 500,
    });
    expect(result.map(({ id }) => id)).toEqual([
      'document-newest',
      'chat-middle',
      'document-task',
      'decision-latest',
    ]);
    expect(result[0]).toMatchObject({
      ownerId: 'document-owner',
      createdAt: '2024-12-31T00:00:00.000Z',
    });
    expect(result[2]).toMatchObject({
      type: 'document',
      fileType: 'md',
      ownerId: 'document-owner',
      createdAt: '2024-12-31T00:00:00.000Z',
      subType: { type: 'task', is_completed: true },
    });
    expect(result[3]).toMatchObject({
      type: 'document',
      fileType: 'md',
      projectId: 'project-1',
      subType: { type: 'decision' },
    });
    expect(readRecordsByKeys).toHaveBeenCalledTimes(2);
    expect(
      readRecordsByKeys.mock.calls.map(([{ fragmentName }]) => fragmentName)
    ).toEqual(['GraphqlDocumentQuickAccessName', 'GraphqlChatQuickAccessName']);
    for (const [{ document }] of readRecordsByKeys.mock.calls) {
      expect(document).toMatch(/QuickAccessName on GraphqlSoup/);
      expect(document).toMatch(/name/);
    }
  });

  it('omits keys whose minimal name projection is incomplete', async () => {
    const search = vi.fn(
      async (): Promise<SearchCachePage> => ({
        documents: [
          {
            profile: 'quick-access-v1',
            recordKey: 'GraphqlSoupDocument:missing-name',
            bucket: 'document',
            searchText: 'fallback text',
            timestampMs: 1,
            sourceHash: 'hash',
          },
        ],
        nextCursor: null,
      })
    );
    const readRecordsByKeys = vi.fn(async () => []);

    await expect(
      readCachedGraphqlHistoryItems(cacheHost(search, readRecordsByKeys))
    ).resolves.toEqual([]);
  });
});
