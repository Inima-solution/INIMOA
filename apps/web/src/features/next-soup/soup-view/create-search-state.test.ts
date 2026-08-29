import { SYSTEM_PROPERTY_IDS } from '@property/constants';
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

const withFixedLocalTime = (
  timezone: string,
  instant: string,
  test: () => void
) => {
  const previousTimezone = process.env.TZ;
  process.env.TZ = timezone;
  vi.useFakeTimers();
  vi.setSystemTime(new Date(instant));

  try {
    test();
  } finally {
    vi.useRealTimers();
    if (previousTimezone === undefined) {
      delete process.env.TZ;
    } else {
      process.env.TZ = previousTimezone;
    }
  }
};

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

  it('serializes all Due Date buckets as TASK-only date range filters', () => {
    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      expect(
        includePropertiesToFilters([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'overdue',
          },
        ])
      ).toEqual([
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          entity_type: 'TASK',
          date_range: { lt: '2026-08-29T15:00:00.000Z' },
        },
      ]);
      expect(
        includePropertiesToFilters([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'today',
          },
        ])
      ).toEqual([
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          entity_type: 'TASK',
          date_range: {
            gte: '2026-08-29T15:00:00.000Z',
            lt: '2026-08-30T15:00:00.000Z',
          },
        },
      ]);
      expect(
        includePropertiesToFilters([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'upcoming',
          },
        ])
      ).toEqual([
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          entity_type: 'TASK',
          date_range: { gte: '2026-08-30T15:00:00.000Z' },
        },
      ]);
      expect(
        includePropertiesToFilters([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'no-due',
          },
        ])
      ).toEqual([
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          entity_type: 'TASK',
          date_range: { exclude: true },
        },
      ]);
    });
  });

  it('fails closed for a Date filter on a non-Due Date property', () => {
    expect(() =>
      includePropertiesToFilters([
        { propertyId: 'not-due-date', type: 'date', value: 'today' },
      ])
    ).toThrow('Invalid Due Date property filter group');
  });

  it('serializes mixed and duplicate Due Date filters as separate AND clauses', () => {
    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      expect(
        includePropertiesToFilters([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'today',
          },
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'select',
            value: 'option-1',
          },
        ])
      ).toEqual([
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          entity_type: 'TASK',
          date_range: {
            gte: '2026-08-29T15:00:00.000Z',
            lt: '2026-08-30T15:00:00.000Z',
          },
        },
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          option_ids: ['option-1'],
        },
      ]);
      expect(
        includePropertiesToFilters([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'today',
          },
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'upcoming',
          },
        ])
      ).toEqual([
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          entity_type: 'TASK',
          date_range: {
            gte: '2026-08-29T15:00:00.000Z',
            lt: '2026-08-30T15:00:00.000Z',
          },
        },
        {
          property_definition_id: SYSTEM_PROPERTY_IDS.DUE_DATE,
          entity_type: 'TASK',
          date_range: { gte: '2026-08-30T15:00:00.000Z' },
        },
      ]);
    });
  });
});
