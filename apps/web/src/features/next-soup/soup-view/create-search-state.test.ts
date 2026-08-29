import { describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  Object.defineProperty(globalThis, 'WebSocket', {
    configurable: true,
    value: class {
      close() {}
      send() {}
      addEventListener() {}
      removeEventListener() {}
    },
  });
});

vi.mock('@app/features/next-soup/search-utils', () => ({
  createSoupFreshSearch: () => ({}),
  intersectEntityPools: () => [],
  nameFuzzySearchFilter: () => [],
}));
vi.mock('@app/features/next-soup/search-context', () => ({
  useSearchContext: () => ({}),
}));
vi.mock('@components/app/GlobalAppState', () => ({
  useGlobalNotificationSource: () => undefined,
}));
vi.mock('@core/context/user', () => ({ useUserId: () => () => undefined }));
vi.mock('@queries/soup/search', () => ({
  useSearchSoupQuery: () => ({}),
  validateSearchServiceText: () => true,
}));

import { includePropertiesToFilters } from './create-search-state';

describe('includePropertiesToFilters', () => {
  it('serializes boolean properties while preserving select and entity groups', () => {
    expect(
      includePropertiesToFilters([
        { propertyId: 'select', type: 'select', value: 'option-1' },
        { propertyId: 'entity', type: 'entity', value: 'entity-1' },
        { propertyId: 'boolean', type: 'boolean', value: true },
        { propertyId: 'select', type: 'select', value: 'option-2' },
      ])
    ).toEqual([
      {
        property_definition_id: 'select',
        option_ids: ['option-1', 'option-2'],
      },
      { property_definition_id: 'entity', entity_ids: ['entity-1'] },
      { property_definition_id: 'boolean', boolean_value: true },
    ]);
  });
});
