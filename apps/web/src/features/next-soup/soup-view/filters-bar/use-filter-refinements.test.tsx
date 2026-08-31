import type { PropertyFilter } from '@app/features/next-soup/filters/filter-store';
import type { QuickAccessItem } from '@core/context/quickAccess/types';
import { describe, expect, it, vi } from 'vitest';
import type { TaskCustomProperty } from './task-custom-property-filter';
import {
  isUnavailableTaskEntityPropertyFilter,
  partitionTaskEntityPropertyFilters,
  removeUnavailableTaskEntityPropertyFilters,
} from './use-filter-refinements';

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

const people: TaskCustomProperty = {
  id: 'reviewer',
  label: 'Reviewer',
  type: 'entity',
  specificEntityType: 'USER',
  isMultiSelect: true,
  options: [],
};

const status: TaskCustomProperty = {
  id: 'status',
  label: 'Status',
  type: 'select',
  options: [{ id: 'open', label: 'Open', value: 'open' }],
};

const person = (id: string, name: string): QuickAccessItem =>
  ({
    id,
    kind: 'user',
    bucket: 'person',
    data: { id, name, email: `${id}@example.test` },
  }) as QuickAccessItem;

describe('typed entity custom-property refinements', () => {
  it('uses the existing quickAccess cache once per selected id and exposes only resolved labels', () => {
    const getById = vi.fn((id: string) =>
      id === 'user-1' ? person(id, 'Ada') : undefined
    );

    expect(
      partitionTaskEntityPropertyFilters(['user-1', 'missing'], people, getById)
    ).toEqual({
      values: [{ id: 'user-1', label: 'Ada' }],
      unavailable: true,
    });
    expect(getById).toHaveBeenCalledTimes(2);
    expect(getById).toHaveBeenNthCalledWith(1, 'user-1');
    expect(getById).toHaveBeenNthCalledWith(2, 'missing');
  });

  it('reclassifies a bounded shared-source miss after its reactive lookup updates', () => {
    let items = new Map<string, QuickAccessItem>();
    const getById = (id: string) => items.get(id);

    expect(
      partitionTaskEntityPropertyFilters(['user-1'], people, getById)
    ).toEqual({ values: [], unavailable: true });

    items = new Map([['user-1', person('user-1', 'Ada')]]);
    expect(
      partitionTaskEntityPropertyFilters(['user-1'], people, getById)
    ).toEqual({
      values: [{ id: 'user-1', label: 'Ada' }],
      unavailable: false,
    });
  });

  it('partitions malformed, missing, and wrong-target refs as unnamed unavailable entries', () => {
    const project = {
      id: 'project-1',
      kind: 'entity',
      bucket: 'project',
      data: { id: 'project-1', type: 'project', name: 'Private project' },
    } as QuickAccessItem;
    const getById = (id: string) => (id === 'project-1' ? project : undefined);
    const filters = [
      { propertyId: 'reviewer', type: 'entity', value: 'project-1' },
      { propertyId: 'reviewer', type: 'entity', value: '' },
      { propertyId: 'reviewer', type: 'entity', value: 'deleted' },
    ] as PropertyFilter[];

    const partition = partitionTaskEntityPropertyFilters(
      ['project-1', '', 'deleted'],
      people,
      getById
    );
    expect(partition).toEqual({ values: [], unavailable: true });
    expect(JSON.stringify(partition)).not.toContain('project-1');
    expect(JSON.stringify(partition)).not.toContain('deleted');

    expect(
      filters.every((filter) =>
        isUnavailableTaskEntityPropertyFilter(filter, [people], getById)
      )
    ).toBe(true);
    expect(
      removeUnavailableTaskEntityPropertyFilters(
        [
          ...filters,
          { propertyId: 'status', type: 'select', value: 'open' },
          { propertyId: 'reviewer', type: 'entity', value: 'user-1' },
        ] as PropertyFilter[],
        [people, status],
        (id) => (id === 'user-1' ? person(id, 'Ada') : getById(id))
      )
    ).toEqual([
      { propertyId: 'status', type: 'select', value: 'open' },
      { propertyId: 'reviewer', type: 'entity', value: 'user-1' },
    ]);
  });
});
