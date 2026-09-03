/** @vitest-environment jsdom */

import {
  compileToAst,
  queryStateFrom,
} from '@app/features/next-soup/filters/filter-store';
import { makeGraphqlSoupInput } from '@queries/soup/graphql/ast';
import { cleanup, render } from '@solidjs/testing-library';
import type { ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const NIL_UUID = '00000000-0000-0000-0000-000000000000';

const mocks = vi.hoisted(() => ({
  membership: vi.fn(() => true),
  soupConfig: undefined as Record<string, unknown> | undefined,
  viewProps: undefined as Record<string, unknown> | undefined,
  listProps: undefined as Record<string, unknown> | undefined,
}));

vi.mock('@app/features/next-soup/create-soup-state', () => ({
  createSoupState: (config: Record<string, unknown>) => {
    mocks.soupConfig = config;
    return { id: 'decision-soup' };
  },
}));

vi.mock(
  '@app/features/next-soup/filters/query-filters',
  async (importOriginal) => ({
    ...(await importOriginal()),
    soupItemMatchesProjectMembership: mocks.membership,
  })
);

vi.mock('@app/features/next-soup/soup-context', () => ({
  SoupContextProvider: (props: ParentProps) => props.children,
}));

vi.mock('@app/features/next-soup/soup-view/soup-view-context', () => ({
  SoupViewContextProvider: (props: ParentProps<Record<string, unknown>>) => {
    mocks.viewProps = props;
    return props.children;
  },
}));

vi.mock('@app/features/next-soup/soup-view/soup-view', () => ({
  SoupViewList: (props: Record<string, unknown>) => {
    mocks.listProps = props;
    return <div data-testid="decision-soup-list" />;
  },
}));

import {
  buildProjectDecisionQuery,
  ProjectDecisionList,
} from './ProjectDecisionList';

beforeEach(() => {
  mocks.membership.mockClear();
  mocks.soupConfig = undefined;
  mocks.viewProps = undefined;
  mocks.listProps = undefined;
});

afterEach(cleanup);

describe('ProjectDecisionList', () => {
  it('builds an exact project-and-Decision document query and closes every other Soup branch', () => {
    const query = buildProjectDecisionQuery('project-1');
    const ast = compileToAst(queryStateFrom(query));

    expect(ast.df).toEqual({
      '&': [{ l: { pid: 'project-1' } }, { l: { dst: 'decision' } }],
    });

    const otherBranches = Object.entries(ast).filter(([key]) => key !== 'df');
    expect(otherBranches).toHaveLength(9);
    for (const [, branch] of otherBranches) {
      expect(JSON.stringify(branch)).toContain(NIL_UUID);
    }

    const firstPage = makeGraphqlSoupInput({
      params: {
        limit: 20,
        sort_method: 'updated_at',
        sort_direction: 'desc',
      },
      body: ast,
    });
    expect(firstPage).toMatchObject({
      initial: {
        limit: 20,
        expand: true,
        sortMethod: 'UPDATED_AT',
        sortDirection: 'DESC',
        filters: {
          documentFilter: {
            and: {
              left: { literal: { projectId: 'project-1' } },
              right: { literal: { subType: 'DECISION' } },
            },
          },
        },
      },
    });

    expect(
      makeGraphqlSoupInput({
        params: {
          limit: 20,
          sort_method: 'updated_at',
          sort_direction: 'desc',
        },
        body: ast,
        cursor: 'opaque-page-2',
      })
    ).toEqual({
      continuation: {
        cursor: 'opaque-page-2',
        expand: true,
        emailView: undefined,
        sortDirection: 'DESC',
      },
    });
  });

  it('uses a dedicated active provider and the normal paginated Soup list', () => {
    const view = render(() => (
      <ProjectDecisionList projectId="project-1" scopeId="scope-1" />
    ));

    expect(view.getByTestId('decision-soup-list')).toBeTruthy();
    expect(mocks.soupConfig).toMatchObject({
      initialPredicates: { and: ['project-decisions'] },
    });
    expect(mocks.viewProps).toMatchObject({
      entryStateNamespace: 'project-decisions',
      initialEnabled: true,
      initialQuery: buildProjectDecisionQuery('project-1'),
      soup: { id: 'decision-soup' },
    });
    expect(mocks.listProps).toMatchObject({
      customScrollbarHidden: true,
      scopeId: 'scope-1',
    });

    const membership = mocks.viewProps?.itemMembershipFilter as
      | ((item: unknown) => boolean)
      | undefined;
    expect(membership?.({ id: 'decision-1' })).toBe(true);
    expect(mocks.membership).toHaveBeenCalledWith(
      { id: 'decision-1' },
      'project-1'
    );

    const config = mocks.soupConfig as {
      predicateConfigs: Array<{
        predicate: (entity: unknown) => boolean;
      }>;
    };
    expect(
      config.predicateConfigs[0]?.predicate({
        id: 'decision-1',
        type: 'document',
        fileType: 'md',
        projectId: 'project-1',
        subType: { type: 'decision' },
      })
    ).toBe(true);
    expect(
      config.predicateConfigs[0]?.predicate({
        id: 'note-1',
        type: 'document',
        fileType: 'md',
        projectId: 'project-1',
        subType: null,
      })
    ).toBe(false);
  });
});
