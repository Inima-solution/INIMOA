import {
  createSoupState,
  type SoupState,
} from '@app/features/next-soup/create-soup-state';
import {
  defineQueryFilters,
  type Query,
} from '@app/features/next-soup/filters/filter-store';
import { soupItemMatchesProjectMembership } from '@app/features/next-soup/filters/query-filters';
import { SoupContextProvider } from '@app/features/next-soup/soup-context';
import { SoupViewList } from '@app/features/next-soup/soup-view/soup-view';
import { SoupViewContextProvider } from '@app/features/next-soup/soup-view/soup-view-context';
import { isDecisionEntity } from '@entity/types/entity';

/**
 * The server-owned Decision list query. `defineQueryFilters` closes every
 * non-document Soup branch with its nil identifier, while `documentWhere`
 * keeps project membership and subtype in one document-only expression.
 */
export function buildProjectDecisionQuery(projectId: string): Query {
  return defineQueryFilters({
    documentWhere: {
      op: 'and',
      clauses: [
        { include: { projectId: [projectId] } },
        { include: { subType: ['decision'] } },
      ],
    },
  });
}

function createProjectDecisionSoup(): SoupState {
  return createSoupState({
    initialPredicates: { and: ['project-decisions'] },
    predicateConfigs: [
      {
        id: 'project-decisions',
        predicate: isDecisionEntity,
      },
    ],
  });
}

export function ProjectDecisionList(props: {
  projectId: string;
  scopeId: string;
}) {
  const soup = createProjectDecisionSoup();

  return (
    <SoupContextProvider soup={soup}>
      <SoupViewContextProvider
        soup={soup}
        entryStateNamespace="project-decisions"
        initialEnabled
        initialQuery={buildProjectDecisionQuery(props.projectId)}
        itemMembershipFilter={(item) =>
          soupItemMatchesProjectMembership(item, props.projectId)
        }
      >
        <SoupViewList customScrollbarHidden scopeId={props.scopeId} />
      </SoupViewContextProvider>
    </SoupContextProvider>
  );
}
