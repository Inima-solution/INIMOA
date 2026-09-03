import type { PropertyFilter } from '@app/features/next-soup/filters/filter-store';
import type { PropertyDefinitionResponse } from '@service-properties/generated/schemas/propertyDefinitionResponse';
import { describe, expect, it } from 'vitest';
import {
  isUnavailableTaskCustomProperty,
  numberRangeForProperty,
  removeUnavailableTaskCustomProperties,
  replaceTaskCustomPropertyValues,
  replaceTaskNumberRange,
  selectedTaskCustomPropertyValues,
  taskCustomProperties,
  taskCustomPropertiesQueryArgs,
  toggleTaskCustomPropertyValue,
  validateNumberRange,
} from './task-custom-property-filter';

const definition = (
  id: string,
  data_type: string,
  options: {
    id: string;
    property_definition_id?: string;
    value: unknown;
  }[] = [],
  is_system = false,
  specific_entity_type: string | null = null,
  is_multi_select = true
) =>
  ({
    definition: {
      id,
      display_name: id === 'select' ? 'Client status' : id,
      data_type,
      is_system,
      is_metadata: false,
      is_multi_select,
      specific_entity_type,
      owner: 'TEAM',
      created_at: '',
      updated_at: '',
    },
    property_options: options.map((option, index) => ({
      ...option,
      property_definition_id: option.property_definition_id ?? id,
      display_order: index,
      created_at: '',
      updated_at: '',
    })),
  }) as unknown as PropertyDefinitionResponse;

describe('task custom property filter state', () => {
  const properties = () =>
    taskCustomProperties([
      definition('select', 'SELECT_STRING', [
        {
          id: 'first',
          property_definition_id: 'select',
          value: { type: 'string', value: 'First' },
        },
        {
          id: 'second',
          property_definition_id: 'select',
          value: { type: 'string', value: 'Second' },
        },
      ]),
      definition('bool', 'BOOLEAN'),
      definition('tag', 'TAG'),
      definition('entity', 'ENTITY'),
      definition('string', 'STRING'),
      definition('number', 'NUMBER'),
      definition('link', 'LINK'),
      definition('date', 'DATE'),
      definition('00000001-0000-0000-0000-000000000002', 'SELECT_STRING'),
      definition('system', 'SELECT_STRING', [], true),
    ]);

  it('keeps only supported non-system definitions and authoritative option order', () => {
    expect(properties()).toEqual([
      {
        id: 'select',
        label: 'Client status',
        type: 'select',
        options: [
          { id: 'first', label: 'First', value: 'first' },
          { id: 'second', label: 'Second', value: 'second' },
        ],
      },
      {
        id: 'bool',
        label: 'bool',
        type: 'boolean',
        options: [
          { id: 'true', label: 'True', value: true },
          { id: 'false', label: 'False', value: false },
        ],
      },
      {
        id: 'number',
        label: 'number',
        type: 'number',
        options: [],
      },
      {
        id: 'date',
        label: 'date',
        type: 'date',
        options: [
          { id: 'overdue', label: 'Overdue', value: 'overdue' },
          { id: 'today', label: 'Today', value: 'today' },
          { id: 'upcoming', label: 'Upcoming', value: 'upcoming' },
          { id: 'no-due', label: 'No due date', value: 'no-due' },
        ],
      },
    ]);
  });

  it('omits empty and malformed select definitions without changing definition order', () => {
    expect(
      taskCustomProperties([
        definition('empty', 'SELECT_STRING'),
        definition('malformed', 'SELECT_NUMBER', [
          {
            id: 'bad',
            property_definition_id: 'malformed',
            value: { type: 'string', value: 'wrong kind' },
          },
        ]),
        definition('valid', 'SELECT_NUMBER', [
          {
            id: 'one',
            property_definition_id: 'valid',
            value: { type: 'number', value: 1 },
          },
          {
            id: 'two',
            property_definition_id: 'valid',
            value: { type: 'number', value: 2 },
          },
        ]),
        definition('cross-definition', 'SELECT_STRING', [
          {
            id: 'foreign',
            property_definition_id: 'another-definition',
            value: { type: 'string', value: 'Foreign option' },
          },
        ]),
      ])
    ).toEqual([
      {
        id: 'valid',
        label: 'valid',
        type: 'select',
        options: [
          { id: 'one', label: '1', value: 'one' },
          { id: 'two', label: '2', value: 'two' },
        ],
      },
    ]);
  });

  it('keeps only quickAccess-backed typed entity definitions', () => {
    expect(
      taskCustomProperties([
        definition('person', 'ENTITY', [], false, 'USER', true),
        definition('project', 'ENTITY', [], false, 'PROJECT', false),
        definition('generic', 'ENTITY'),
        definition('thread', 'ENTITY', [], false, 'THREAD'),
        definition('company', 'ENTITY', [], false, 'COMPANY'),
        definition('call', 'ENTITY', [], false, 'CALL_RECORD'),
        definition('calendar', 'ENTITY', [], false, 'CALENDAR_EVENT'),
      ])
    ).toEqual([
      {
        id: 'person',
        label: 'person',
        type: 'entity',
        specificEntityType: 'USER',
        isMultiSelect: true,
        options: [],
      },
      {
        id: 'project',
        label: 'project',
        type: 'entity',
        specificEntityType: 'PROJECT',
        isMultiSelect: false,
        options: [],
      },
    ]);
  });

  it('dedupes typed entity selections and keeps their definition cardinality', () => {
    const [people, project] = taskCustomProperties([
      definition('person', 'ENTITY', [], false, 'USER', true),
      definition('project', 'ENTITY', [], false, 'PROJECT', false),
    ]);
    const existing = [
      { propertyId: 'person', type: 'entity', value: 'user-2' },
      { propertyId: 'person', type: 'entity', value: 'user-1' },
      { propertyId: 'person', type: 'entity', value: 'user-2' },
      { propertyId: 'other', type: 'select', value: 'keep' },
    ] as PropertyFilter[];
    expect(selectedTaskCustomPropertyValues(existing, people!)).toEqual([
      'user-2',
      'user-1',
    ]);
    expect(
      replaceTaskCustomPropertyValues(existing, people!, [
        'user-1',
        'user-1',
        'user-3',
      ])
    ).toEqual([
      { propertyId: 'other', type: 'select', value: 'keep' },
      { propertyId: 'person', type: 'entity', value: 'user-1' },
      { propertyId: 'person', type: 'entity', value: 'user-3' },
    ]);
    expect(
      replaceTaskCustomPropertyValues([], project!, ['project-2', 'project-1'])
    ).toEqual([{ propertyId: 'project', type: 'entity', value: 'project-2' }]);
  });

  it('preserves selected order and only replaces the targeted property', () => {
    const [select] = properties();
    const existing: PropertyFilter[] = [
      { propertyId: 'built-in', type: 'select', value: 'keep' },
      { propertyId: 'select', type: 'select', value: 'second' },
      { propertyId: 'other-custom', type: 'boolean', value: true },
      { propertyId: 'select', type: 'select', value: 'first' },
    ];
    expect(selectedTaskCustomPropertyValues(existing, select!)).toEqual([
      'second',
      'first',
    ]);
    expect(
      replaceTaskCustomPropertyValues(existing, select!, ['first', 'second'])
    ).toEqual([
      { propertyId: 'built-in', type: 'select', value: 'keep' },
      { propertyId: 'other-custom', type: 'boolean', value: true },
      { propertyId: 'select', type: 'select', value: 'first' },
      { propertyId: 'select', type: 'select', value: 'second' },
    ]);
  });

  it('replaces only recognized entries while preserving unavailable, wrong-type, and unrelated filters', () => {
    const [select] = properties();
    const existing = [
      { propertyId: 'built-in', type: 'select', value: 'keep' },
      { propertyId: 'select', type: 'select', value: 'second' },
      { propertyId: 'select', type: 'boolean', value: true },
      { propertyId: 'select', type: 'select', value: 'removed-option' },
      { propertyId: 'other-custom', type: 'boolean', value: false },
      { propertyId: 'select', type: 'select', value: 'first' },
    ] as PropertyFilter[];

    expect(
      replaceTaskCustomPropertyValues(existing, select!, ['first'])
    ).toEqual([
      { propertyId: 'built-in', type: 'select', value: 'keep' },
      { propertyId: 'select', type: 'boolean', value: true },
      { propertyId: 'select', type: 'select', value: 'removed-option' },
      { propertyId: 'other-custom', type: 'boolean', value: false },
      { propertyId: 'select', type: 'select', value: 'first' },
    ]);
  });

  it('keeps booleans and dates single-select while select values retain OR semantics', () => {
    const [, boolean, , date] = properties();
    const [select] = properties();
    expect(toggleTaskCustomPropertyValue(['true'], boolean!, 'false')).toEqual([
      'false',
    ]);
    expect(toggleTaskCustomPropertyValue(['true'], boolean!, 'true')).toEqual(
      []
    );
    expect(toggleTaskCustomPropertyValue(['first'], select!, 'second')).toEqual(
      ['first', 'second']
    );
    expect(toggleTaskCustomPropertyValue(['today'], date!, 'upcoming')).toEqual(
      ['upcoming']
    );
    expect(
      replaceTaskCustomPropertyValues(
        [{ propertyId: 'bool', type: 'boolean', value: true }],
        boolean!,
        ['false', 'true']
      )
    ).toEqual([{ propertyId: 'bool', type: 'boolean', value: false }]);
    expect(
      replaceTaskCustomPropertyValues(
        [
          { propertyId: 'date', type: 'date', value: 'today' },
          { propertyId: 'date', type: 'date', value: 'upcoming' },
        ],
        date!,
        ['no-due', 'today']
      )
    ).toEqual([{ propertyId: 'date', type: 'date', value: 'no-due' }]);
  });

  it('validates and replaces only finite Number ranges without exposing malformed state', () => {
    const number = properties().find((property) => property.type === 'number')!;
    expect(validateNumberRange({ gte: 2, lt: 4 })).toBeUndefined();
    expect(validateNumberRange({ gt: 4, lte: 4 })).toMatch(/Equal bounds/);
    expect(validateNumberRange({})).toMatch(/lower or upper/);
    expect(
      replaceTaskNumberRange(
        [{ propertyId: 'keep', type: 'select', value: 'x' }],
        number,
        { gte: 2, lt: 4 },
        true
      )
    ).toEqual([
      { propertyId: 'keep', type: 'select', value: 'x' },
      {
        propertyId: 'number',
        type: 'number',
        range: { gte: 2, lt: 4 },
        exclude: true,
      },
    ]);
    expect(
      numberRangeForProperty(
        [{ propertyId: 'number', type: 'number', range: {} }],
        number
      )
    ).toBeUndefined();
    const existing = [
      {
        propertyId: 'number',
        type: 'number' as const,
        range: { gte: 2 },
      },
    ];
    expect(replaceTaskNumberRange(existing, number, {})).toEqual(existing);
    expect(
      replaceTaskNumberRange(existing, number, {
        gte: Number.POSITIVE_INFINITY,
      })
    ).toEqual(existing);
    expect(
      replaceTaskNumberRange(
        [{ propertyId: 'keep', type: 'select', value: 'x' }, ...existing],
        number,
        undefined
      )
    ).toEqual([{ propertyId: 'keep', type: 'select', value: 'x' }]);
  });

  it('keeps unknown or removed values fail-closed without exposing IDs', () => {
    const [select] = properties();
    expect(
      isUnavailableTaskCustomProperty(
        { propertyId: 'inaccessible-id', type: 'select', value: 'secret' },
        properties()
      )
    ).toBe(true);
    expect(
      isUnavailableTaskCustomProperty(
        { propertyId: select!.id, type: 'select', value: 'removed-option' },
        properties()
      )
    ).toBe(true);
    expect(
      isUnavailableTaskCustomProperty(
        {
          propertyId: '00000001-0000-0000-0000-000000000002',
          type: 'select',
          value: 'status',
        },
        properties()
      )
    ).toBe(false);
  });

  it('classifies inaccessible, unsupported, and wrong-type entries as unavailable and removes only those entries', () => {
    const [select] = properties();
    const filters = [
      { propertyId: 'inaccessible', type: 'select', value: 'hidden' },
      { propertyId: 'tag', type: 'select', value: 'tag-option' },
      { propertyId: select!.id, type: 'boolean', value: true },
      { propertyId: select!.id, type: 'select', value: 'first' },
      {
        propertyId: '00000001-0000-0000-0000-000000000002',
        type: 'select',
        value: 'system',
      },
    ] as PropertyFilter[];

    expect(
      filters.map((filter) =>
        isUnavailableTaskCustomProperty(filter, properties())
      )
    ).toEqual([true, true, true, false, false]);
    expect(
      removeUnavailableTaskCustomProperties(filters, properties())
    ).toEqual([
      { propertyId: select!.id, type: 'select', value: 'first' },
      {
        propertyId: '00000001-0000-0000-0000-000000000002',
        type: 'select',
        value: 'system',
      },
    ]);
  });

  it('uses the same task-only property query parameters at every observer', () => {
    expect(taskCustomPropertiesQueryArgs()).toEqual({
      scope: 'all',
      includeOptions: true,
      forEntityType: 'TASK',
    });
  });
});
