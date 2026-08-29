import { soupPropertyToProperty } from '@entity/extractors-property/property-helpers';
import { PROPERTY_OPTION_IDS, SYSTEM_PROPERTY_IDS } from '@property/constants';
import type {
  EntityPropertyWithDefinition,
  EntityReference,
  PropertyApiValues,
} from '@property/types';
import { mapGraphqlProperties } from '@service-storage/graphql-soup';
import { describe, expect, it } from 'vitest';
import { entityPropertyFromApi, propertyValueToApi } from './converters';

type TaskPropertyFixture = {
  name: string;
  definitionId: string;
  dataType: 'ENTITY' | 'SELECT_STRING' | 'DATE' | 'NUMBER';
  isMultiSelect: boolean;
  apiValue: unknown;
  values: PropertyApiValues;
  request: Record<string, unknown>;
  graphqlValue: unknown;
};

const references: EntityReference[] = [
  { entity_type: 'USER', entity_id: 'user-2' },
  { entity_type: 'USER', entity_id: 'user-1' },
];

const taskPropertyFixtures: readonly TaskPropertyFixture[] = [
  {
    name: 'Assignees',
    definitionId: SYSTEM_PROPERTY_IDS.ASSIGNEES,
    dataType: 'ENTITY',
    isMultiSelect: true,
    apiValue: { type: 'EntityReference', value: references },
    values: { valueType: 'ENTITY', refs: references },
    request: { type: 'multi_entity_reference', references },
    graphqlValue: {
      __typename: 'GraphqlEntityReferencePropertyValue',
      references: references.map((reference) => ({
        entityType: reference.entity_type,
        entityId: reference.entity_id,
      })),
    },
  },
  {
    name: 'Status',
    definitionId: SYSTEM_PROPERTY_IDS.STATUS,
    dataType: 'SELECT_STRING',
    isMultiSelect: false,
    apiValue: {
      type: 'SelectOption',
      value: [PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS],
    },
    values: {
      valueType: 'SELECT_STRING',
      values: [PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS],
    },
    request: {
      type: 'select_option',
      option_id: PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS,
    },
    graphqlValue: {
      __typename: 'GraphqlSelectOptionPropertyValue',
      optionIds: [PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS],
    },
  },
  {
    name: 'Priority',
    definitionId: SYSTEM_PROPERTY_IDS.PRIORITY,
    dataType: 'SELECT_STRING',
    isMultiSelect: false,
    apiValue: {
      type: 'SelectOption',
      value: [PROPERTY_OPTION_IDS.PRIORITY.HIGH],
    },
    values: {
      valueType: 'SELECT_STRING',
      values: [PROPERTY_OPTION_IDS.PRIORITY.HIGH],
    },
    request: {
      type: 'select_option',
      option_id: PROPERTY_OPTION_IDS.PRIORITY.HIGH,
    },
    graphqlValue: {
      __typename: 'GraphqlSelectOptionPropertyValue',
      optionIds: [PROPERTY_OPTION_IDS.PRIORITY.HIGH],
    },
  },
  {
    name: 'Due Date',
    definitionId: SYSTEM_PROPERTY_IDS.DUE_DATE,
    dataType: 'DATE',
    isMultiSelect: false,
    apiValue: { type: 'Date', value: '2026-08-29T12:34:56.000Z' },
    values: { valueType: 'DATE', value: new Date('2026-08-29T12:34:56.000Z') },
    request: { type: 'date', value: '2026-08-29T12:34:56.000Z' },
    graphqlValue: {
      __typename: 'GraphqlDatePropertyValue',
      dateValue: '2026-08-29T12:34:56.000Z',
    },
  },
  {
    name: 'Parent Task',
    definitionId: SYSTEM_PROPERTY_IDS.PARENT_TASK,
    dataType: 'ENTITY',
    isMultiSelect: false,
    apiValue: {
      type: 'EntityReference',
      value: [{ entity_type: 'TASK', entity_id: 'parent-task' }],
    },
    values: {
      valueType: 'ENTITY',
      refs: [{ entity_type: 'TASK', entity_id: 'parent-task' }],
    },
    request: {
      type: 'entity_reference',
      reference: { entity_type: 'TASK', entity_id: 'parent-task' },
    },
    graphqlValue: {
      __typename: 'GraphqlEntityReferencePropertyValue',
      references: [{ entityType: 'TASK', entityId: 'parent-task' }],
    },
  },
  {
    name: 'Subtasks',
    definitionId: SYSTEM_PROPERTY_IDS.SUBTASKS,
    dataType: 'ENTITY',
    isMultiSelect: true,
    apiValue: {
      type: 'EntityReference',
      value: [
        { entity_type: 'TASK', entity_id: 'subtask-2' },
        { entity_type: 'TASK', entity_id: 'subtask-1' },
      ],
    },
    values: {
      valueType: 'ENTITY',
      refs: [
        { entity_type: 'TASK', entity_id: 'subtask-2' },
        { entity_type: 'TASK', entity_id: 'subtask-1' },
      ],
    },
    request: {
      type: 'multi_entity_reference',
      references: [
        { entity_type: 'TASK', entity_id: 'subtask-2' },
        { entity_type: 'TASK', entity_id: 'subtask-1' },
      ],
    },
    graphqlValue: {
      __typename: 'GraphqlEntityReferencePropertyValue',
      references: [
        { entityType: 'TASK', entityId: 'subtask-2' },
        { entityType: 'TASK', entityId: 'subtask-1' },
      ],
    },
  },
  {
    name: 'Depends On',
    definitionId: SYSTEM_PROPERTY_IDS.DEPENDS_ON,
    dataType: 'ENTITY',
    isMultiSelect: true,
    apiValue: {
      type: 'EntityReference',
      value: [
        { entity_type: 'TASK', entity_id: 'dependency-2' },
        { entity_type: 'TASK', entity_id: 'dependency-1' },
      ],
    },
    values: {
      valueType: 'ENTITY',
      refs: [
        { entity_type: 'TASK', entity_id: 'dependency-2' },
        { entity_type: 'TASK', entity_id: 'dependency-1' },
      ],
    },
    request: {
      type: 'multi_entity_reference',
      references: [
        { entity_type: 'TASK', entity_id: 'dependency-2' },
        { entity_type: 'TASK', entity_id: 'dependency-1' },
      ],
    },
    graphqlValue: {
      __typename: 'GraphqlEntityReferencePropertyValue',
      references: [
        { entityType: 'TASK', entityId: 'dependency-2' },
        { entityType: 'TASK', entityId: 'dependency-1' },
      ],
    },
  },
  {
    name: 'Effort',
    definitionId: SYSTEM_PROPERTY_IDS.EFFORT,
    dataType: 'SELECT_STRING',
    isMultiSelect: false,
    apiValue: {
      type: 'SelectOption',
      value: ['00000001-0000-0000-0008-000000000003'],
    },
    values: {
      valueType: 'SELECT_STRING',
      values: ['00000001-0000-0000-0008-000000000003'],
    },
    request: {
      type: 'select_option',
      option_id: '00000001-0000-0000-0008-000000000003',
    },
    graphqlValue: {
      __typename: 'GraphqlSelectOptionPropertyValue',
      optionIds: ['00000001-0000-0000-0008-000000000003'],
    },
  },
  {
    name: 'Story Points',
    definitionId: SYSTEM_PROPERTY_IDS.STORY_POINTS,
    dataType: 'NUMBER',
    isMultiSelect: false,
    apiValue: { type: 'Number', value: 8 },
    values: { valueType: 'NUMBER', value: 8 },
    request: { type: 'number', value: 8 },
    graphqlValue: {
      __typename: 'GraphqlNumberPropertyValue',
      numberValue: 8,
    },
  },
  {
    name: 'Relevant Documents',
    definitionId: SYSTEM_PROPERTY_IDS.RELEVANT_DOCUMENTS,
    dataType: 'ENTITY',
    isMultiSelect: true,
    apiValue: {
      type: 'EntityReference',
      value: [
        { entity_type: 'DOCUMENT', entity_id: 'document-2' },
        { entity_type: 'DOCUMENT', entity_id: 'document-1' },
      ],
    },
    values: {
      valueType: 'ENTITY',
      refs: [
        { entity_type: 'DOCUMENT', entity_id: 'document-2' },
        { entity_type: 'DOCUMENT', entity_id: 'document-1' },
      ],
    },
    request: {
      type: 'multi_entity_reference',
      references: [
        { entity_type: 'DOCUMENT', entity_id: 'document-2' },
        { entity_type: 'DOCUMENT', entity_id: 'document-1' },
      ],
    },
    graphqlValue: {
      __typename: 'GraphqlEntityReferencePropertyValue',
      references: [
        { entityType: 'DOCUMENT', entityId: 'document-2' },
        { entityType: 'DOCUMENT', entityId: 'document-1' },
      ],
    },
  },
];

function restFixture(
  fixture: TaskPropertyFixture
): EntityPropertyWithDefinition {
  return {
    property: {
      id: `property-${fixture.definitionId}`,
      property_definition_id: fixture.definitionId,
      created_at: '2026-08-29T00:00:00.000Z',
      updated_at: '2026-08-29T00:00:00.000Z',
    },
    definition: {
      id: fixture.definitionId,
      display_name: fixture.name,
      data_type: fixture.dataType,
      is_multi_select: fixture.isMultiSelect,
      is_metadata: false,
      is_system: true,
      owner: { scope: 'system' },
      created_at: '2026-08-29T00:00:00.000Z',
      updated_at: '2026-08-29T00:00:00.000Z',
    },
    value: fixture.apiValue,
  } as EntityPropertyWithDefinition;
}

describe('canonical Task system properties', () => {
  it('preserves all ten REST response values and outbound request shapes', () => {
    expect(taskPropertyFixtures).toHaveLength(10);

    for (const fixture of taskPropertyFixtures) {
      const converted = entityPropertyFromApi(restFixture(fixture));
      expect(converted.propertyDefinitionId, fixture.name).toBe(
        fixture.definitionId
      );
      expect(converted.isMultiSelect, fixture.name).toBe(fixture.isMultiSelect);
      expect(
        propertyValueToApi(fixture.values, fixture.isMultiSelect),
        fixture.name
      ).toEqual(fixture.request);

      if (fixture.values.valueType === 'DATE') {
        expect(converted.value, fixture.name).toEqual(fixture.values.value);
      } else if (fixture.values.valueType === 'ENTITY') {
        expect(converted.value, fixture.name).toEqual(fixture.values.refs);
      } else if (fixture.values.valueType === 'SELECT_STRING') {
        expect(converted.value, fixture.name).toEqual(fixture.values.values);
      } else {
        expect(converted.value, fixture.name).toEqual(fixture.request.value);
      }
    }
  });

  it('projects the same ten properties through the real GraphQL Soup mapper', () => {
    const mapped = mapGraphqlProperties(
      taskPropertyFixtures.map(
        (fixture) =>
          ({
            id: `property-${fixture.definitionId}`,
            propertyDefinitionId: fixture.definitionId,
            displayName: fixture.name,
            dataType: fixture.dataType,
            isMultiSelect: fixture.isMultiSelect,
            isMetadata: false,
            isSystem: true,
            specificEntityType: null,
            value: fixture.graphqlValue,
          }) as never
      )
    ).map(soupPropertyToProperty);

    expect(mapped.map((property) => property.propertyDefinitionId)).toEqual(
      taskPropertyFixtures.map((fixture) => fixture.definitionId)
    );
    expect(mapped.map((property) => property.value)).toEqual(
      taskPropertyFixtures.map((fixture) => {
        if (fixture.values.valueType === 'DATE') return fixture.values.value;
        if (fixture.values.valueType === 'ENTITY')
          return fixture.values.refs?.map((reference) => ({
            ...reference,
            specific_message_id: undefined,
          }));
        if (fixture.values.valueType === 'SELECT_STRING')
          return fixture.values.values;
        return fixture.request.value;
      })
    );
  });
});
