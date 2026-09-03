import { describe, expect, it } from 'vitest';
import {
  selectSoupViewInitialValue,
  soupViewEntryStateKey,
} from './soup-view-entry-state';

describe('Soup view entry-state namespaces', () => {
  const ordinaryQuery = { include: { projectId: ['project-1'] } };
  const decisionQuery = {
    documentWhere: { include: { subType: ['decision'] } },
  };

  it('ignores ordinary history when a Decisions provider mounts after back navigation', () => {
    const entryState = { 'search.filters': ordinaryQuery };

    expect(
      selectSoupViewInitialValue({
        entryState,
        key: 'search.filters',
        namespace: 'project-decisions',
        initialValue: decisionQuery,
      })
    ).toBe(decisionQuery);
  });

  it('restores each mode only from its own history key in both directions', () => {
    const entryState = {
      'search.filters': ordinaryQuery,
      'project-decisions.search.filters': decisionQuery,
    };

    expect(
      selectSoupViewInitialValue({
        entryState,
        key: 'search.filters',
        initialValue: ordinaryQuery,
      })
    ).toBe(ordinaryQuery);
    expect(
      selectSoupViewInitialValue({
        entryState,
        key: 'search.filters',
        namespace: 'project-decisions',
        initialValue: decisionQuery,
      })
    ).toBe(decisionQuery);
    expect(soupViewEntryStateKey('search.text', 'project-decisions')).toBe(
      'project-decisions.search.text'
    );
  });
});
