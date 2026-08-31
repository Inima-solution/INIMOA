import type {
  Client,
  GraphQLRequest,
  Operation,
  OperationContext,
  OperationResult,
} from '@urql/core';
import { createRoot, createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { makeSubject, onEnd, pipe } from 'wonka';

const graphqlClientState = vi.hoisted(() => ({
  current: undefined as Client | undefined,
}));

vi.mock('@service-storage/graphql-soup', () => ({
  getGraphqlSoupClient: () => {
    if (!graphqlClientState.current) throw new Error('GraphQL client not set');
    return graphqlClientState.current;
  },
}));

import { EntityActivityDocument } from '@service-storage/graphql/generated/graphql';
import { createEntityActivityQuery } from './entity';

const NIL_ENTITY_ID = '00000000-0000-0000-0000-000000000000';
const disposals: Array<() => void> = [];

type Execution = {
  document: unknown;
  variables: Record<string, unknown>;
  context: Partial<OperationContext>;
  next(data: unknown): void;
  readonly unsubscribed: boolean;
};

function makeFakeClient(): { client: Client; executions: Execution[] } {
  const executions: Execution[] = [];
  const execute = (
    document: unknown,
    variables: Record<string, unknown>,
    context: Partial<OperationContext> = {}
  ) => {
    const subject = makeSubject<OperationResult>();
    let unsubscribed = false;
    const operation = { kind: 'query', context } as Operation;
    executions.push({
      document,
      variables,
      context,
      next: (data) =>
        subject.next({
          operation,
          data,
          stale: false,
          hasNext: false,
        }),
      get unsubscribed() {
        return unsubscribed;
      },
    });
    return pipe(
      subject.source,
      onEnd(() => {
        unsubscribed = true;
      })
    );
  };

  return {
    executions,
    client: {
      executeQuery: (
        request: GraphQLRequest<unknown, Record<string, unknown>>,
        context?: Partial<OperationContext>
      ) => execute(request.query, request.variables, context),
    } as unknown as Client,
  };
}

const NIL_FILTERS = {
  calendarEventFilter: { literal: { id: NIL_ENTITY_ID } },
  documentFilter: { literal: { id: NIL_ENTITY_ID } },
  projectFilter: { literal: { projectIdSelf: NIL_ENTITY_ID } },
  chatFilter: { literal: { chatId: NIL_ENTITY_ID } },
  emailFilter: { tree: { literal: { threadId: NIL_ENTITY_ID } } },
  channelFilter: { literal: { channelId: NIL_ENTITY_ID } },
  channelThreadFilter: { literal: { threadId: NIL_ENTITY_ID } },
  callFilter: { literal: { callId: NIL_ENTITY_ID } },
  crmCompanyFilter: { literal: { id: NIL_ENTITY_ID } },
  foreignEntityFilter: { literal: { id: NIL_ENTITY_ID } },
};

function activity(id: string) {
  return {
    __typename: 'GraphqlActivityEvent',
    id,
    actorId: 'actor-1',
    subjectId: 'subject-1',
    entityType: 'PROJECT',
    entityId: 'project-1',
    occurredAt: '2026-09-01T00:00:00Z',
    action: { __typename: 'GraphqlActivityCreated' },
  };
}

afterEach(() => {
  for (const dispose of disposals.splice(0)) dispose();
  graphqlClientState.current = undefined;
});

describe('createEntityActivityQuery', () => {
  it('keeps the Project activity request bounded, selected, and refetchable', async () => {
    const fake = makeFakeClient();
    graphqlClientState.current = fake.client;
    const [entityType, setEntityType] = createSignal<'PROJECT' | 'USER'>(
      'PROJECT'
    );
    const [entityId, setEntityId] = createSignal('');
    const [enabled, setEnabled] = createSignal(false);
    let query!: ReturnType<typeof createEntityActivityQuery>;

    const dispose = createRoot((rootDispose) => {
      query = createEntityActivityQuery({ entityType, entityId, enabled });
      return rootDispose;
    });
    disposals.push(dispose);

    expect(query.isEnabled()).toBe(false);
    expect(fake.executions).toHaveLength(0);

    setEnabled(true);
    expect(query.isEnabled()).toBe(false);
    expect(fake.executions).toHaveLength(0);

    setEntityType('USER');
    setEntityId('user-1');
    expect(query.isEnabled()).toBe(false);
    expect(fake.executions).toHaveLength(0);

    setEntityId('project-1');
    setEntityType('PROJECT');
    expect(query.isEnabled()).toBe(true);
    expect(fake.executions).toHaveLength(1);
    expect(fake.executions[0]).toMatchObject({
      document: EntityActivityDocument,
      context: { requestPolicy: 'cache-and-network' },
      variables: {
        limit: 20,
        input: {
          initial: {
            limit: 1,
            expand: true,
            sortMethod: 'UPDATED_AT',
            emailView: 'ALL',
            filters: {
              ...NIL_FILTERS,
              projectFilter: { literal: { projectIdSelf: 'project-1' } },
            },
          },
        },
      },
    });

    const orderedRows = [activity('newest'), activity('oldest')];
    fake.executions[0]?.next({
      user: {
        id: 'user-1',
        soup: {
          items: [
            {
              __typename: 'GraphqlSoupDocument',
              id: 'other-entity',
              activity: [activity('other')],
            },
            {
              __typename: 'GraphqlSoupProject',
              id: 'project-1',
              activity: orderedRows,
            },
          ],
        },
      },
    });
    expect(query.result.data).toEqual(orderedRows);

    setEntityId('project-2');
    expect(fake.executions[0]?.unsubscribed).toBe(true);
    expect(fake.executions).toHaveLength(2);
    expect(query.result.data).toBeUndefined();
    fake.executions[1]?.next({
      user: {
        id: 'user-1',
        soup: {
          items: [
            { __typename: 'GraphqlSoupProject', id: 'other', activity: [] },
          ],
        },
      },
    });
    expect(query.result.data).toEqual([]);

    const retained = [activity('project-2-row')];
    fake.executions[1]?.next({
      user: {
        id: 'user-1',
        soup: {
          items: [
            {
              __typename: 'GraphqlSoupProject',
              id: 'project-2',
              activity: retained,
            },
          ],
        },
      },
    });
    const refetch = query.result.refetch();
    expect(fake.executions).toHaveLength(3);
    expect(fake.executions[2]?.variables).toBe(fake.executions[1]?.variables);
    expect(query.result.data).toEqual(retained);
    expect(query.result.isRefetching).toBe(true);
    fake.executions[2]?.next({
      user: {
        id: 'user-1',
        soup: {
          items: [
            {
              __typename: 'GraphqlSoupProject',
              id: 'project-2',
              activity: retained,
            },
          ],
        },
      },
    });
    await expect(refetch).resolves.toMatchObject({ data: retained });

    let overridden!: ReturnType<typeof createEntityActivityQuery>;
    const overrideDispose = createRoot((rootDispose) => {
      overridden = createEntityActivityQuery({
        entityType: () => 'PROJECT',
        entityId: () => 'project-override',
        enabled: () => true,
        limit: 3,
      });
      return rootDispose;
    });
    disposals.push(overrideDispose);
    expect(overridden.isEnabled()).toBe(true);
    expect(fake.executions).toHaveLength(4);
    expect(fake.executions[3]?.variables).toMatchObject({ limit: 3 });
  });
});
