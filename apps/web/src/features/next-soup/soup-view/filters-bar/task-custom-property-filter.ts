import type {
  NumberRangeFilter,
  PropertyFilter,
} from '@app/features/next-soup/filters/filter-store';
import type { DueDateBucket } from '@app/features/next-soup/filters/filter-store/task-due-date';
import { SYSTEM_PROPERTY_IDS } from '@property/constants';
import { EntityType } from '@service-properties/generated/schemas/entityType';
import type { PropertyDefinitionResponse } from '@service-properties/generated/schemas/propertyDefinitionResponse';

export type TaskCustomProperty = {
  id: string;
  label: string;
  type: 'select' | 'boolean' | 'date' | 'number' | 'entity';
  /** ENTITY definitions retain their authoritative target and cardinality. */
  specificEntityType?: EntityType;
  isMultiSelect?: boolean;
  options: {
    id: string;
    label: string;
    value: string | boolean | DueDateBucket;
  }[];
};

const SYSTEM_IDS = new Set<string>(Object.values(SYSTEM_PROPERTY_IDS));

// v1 deliberately supports SELECT_STRING/SELECT_NUMBER options, BOOLEAN,
// DATE, finite NUMBER ranges, TAG through the existing Tags filter, and the
// concrete ENTITY targets below. STRING, LINK, and generic/source-less ENTITY
// have no safe query/picker contract yet and remain excluded.
// These are precisely the concrete targets with a bounded source in
// PropertyEntitySelector. Keep generic and source-less targets out of Soup.
const SELECTABLE_ENTITY_TYPES = new Set<EntityType>([
  'USER',
  'CHANNEL',
  'DOCUMENT',
  'PROJECT',
  'CHAT',
  'TASK',
]);

const DATE_OPTIONS: TaskCustomProperty['options'] = [
  { id: 'overdue', label: 'Overdue', value: 'overdue' },
  { id: 'today', label: 'Today', value: 'today' },
  { id: 'upcoming', label: 'Upcoming', value: 'upcoming' },
  { id: 'no-due', label: 'No due date', value: 'no-due' },
];

/** Shared task-only query arguments; callers retain their local enabled accessors. */
export const taskCustomPropertiesQueryArgs = () => ({
  scope: 'all' as const,
  includeOptions: true,
  forEntityType: EntityType.TASK,
});

/** Converts the authoritative list response to the small set Soup can filter. */
export function taskCustomProperties(
  responses: readonly PropertyDefinitionResponse[] | undefined
): TaskCustomProperty[] {
  const result: TaskCustomProperty[] = [];
  for (const response of responses ?? []) {
    const definition =
      'definition' in response ? response.definition : response;
    if (
      !definition ||
      typeof definition.id !== 'string' ||
      typeof definition.display_name !== 'string' ||
      definition.is_system ||
      SYSTEM_IDS.has(definition.id) ||
      (definition.data_type !== 'SELECT_STRING' &&
        definition.data_type !== 'SELECT_NUMBER' &&
        definition.data_type !== 'BOOLEAN' &&
        definition.data_type !== 'DATE' &&
        definition.data_type !== 'NUMBER' &&
        definition.data_type !== 'ENTITY')
    ) {
      continue;
    }

    if (definition.data_type === 'BOOLEAN') {
      result.push({
        id: definition.id,
        label: definition.display_name,
        type: 'boolean',
        options: [
          { id: 'true', label: 'True', value: true },
          { id: 'false', label: 'False', value: false },
        ],
      });
      continue;
    }

    if (definition.data_type === 'DATE') {
      result.push({
        id: definition.id,
        label: definition.display_name,
        type: 'date',
        options: DATE_OPTIONS,
      });
      continue;
    }

    if (definition.data_type === 'NUMBER') {
      result.push({
        id: definition.id,
        label: definition.display_name,
        type: 'number',
        options: [],
      });
      continue;
    }

    if (definition.data_type === 'ENTITY') {
      const specificEntityType = definition.specific_entity_type;
      if (
        !specificEntityType ||
        !SELECTABLE_ENTITY_TYPES.has(specificEntityType)
      )
        continue;
      result.push({
        id: definition.id,
        label: definition.display_name,
        type: 'entity',
        specificEntityType,
        isMultiSelect: definition.is_multi_select,
        options: [],
      });
      continue;
    }

    const expectedOptionValueType =
      definition.data_type === 'SELECT_STRING' ? 'string' : 'number';
    const options = (
      'property_options' in response ? response.property_options : []
    ).flatMap((option) => {
      const value = option?.value;
      if (
        typeof option?.id !== 'string' ||
        option.property_definition_id !== definition.id ||
        !value ||
        typeof value !== 'object' ||
        value.type !== expectedOptionValueType ||
        typeof value.value !== expectedOptionValueType
      ) {
        return [];
      }
      return [{ id: option.id, label: String(value.value), value: option.id }];
    });
    if (options.length === 0) continue;
    result.push({
      id: definition.id,
      label: definition.display_name,
      type: 'select',
      options,
    });
  }
  return result;
}

export function selectedTaskCustomPropertyValues(
  properties: readonly PropertyFilter[] | undefined,
  property: TaskCustomProperty
): string[] {
  if (property.type === 'number') return [];
  if (property.type === 'entity') {
    const selected = (properties ?? []).flatMap((filter) =>
      filter.propertyId === property.id &&
      filter.type === 'entity' &&
      typeof filter.value === 'string' &&
      filter.value.length > 0
        ? [filter.value]
        : []
    );
    const deduped = [...new Set(selected)];
    return property.isMultiSelect ? deduped : deduped.slice(0, 1);
  }
  const valid = new Set(property.options.map((option) => option.value));
  const selected = (properties ?? []).flatMap((filter) => {
    if (
      filter.propertyId !== property.id ||
      filter.type !== property.type ||
      !('value' in filter)
    )
      return [];
    return valid.has(filter.value) ? [String(filter.value)] : [];
  });
  return property.type === 'select' ? selected : selected.slice(0, 1);
}

/** Replaces exactly one definition's values without changing unrelated filters. */
export function replaceTaskCustomPropertyValues(
  properties: readonly PropertyFilter[] | undefined,
  property: TaskCustomProperty,
  valueIds: readonly string[]
): PropertyFilter[] {
  if (property.type === 'number') return properties ? [...properties] : [];
  if (property.type === 'entity') {
    const next = (properties ?? []).filter(
      (filter) =>
        !(
          filter.propertyId === property.id &&
          filter.type === 'entity' &&
          typeof filter.value === 'string' &&
          filter.value.length > 0
        )
    );
    const ids = property.isMultiSelect ? valueIds : valueIds.slice(0, 1);
    for (const id of new Set(ids)) {
      if (id.length > 0) {
        next.push({ propertyId: property.id, type: 'entity', value: id });
      }
    }
    return next;
  }
  const values = new Map(
    property.options.map((option) => [option.id, option.value])
  );
  const isRecognized = (filter: PropertyFilter) =>
    filter.propertyId === property.id &&
    filter.type === property.type &&
    'value' in filter &&
    values.has(String(filter.value)) &&
    values.get(String(filter.value)) === filter.value;
  const next = (properties ?? []).filter((filter) => !isRecognized(filter));
  const nextValueIds =
    property.type === 'select' ? valueIds : valueIds.slice(0, 1);
  for (const valueId of new Set(nextValueIds)) {
    const value = values.get(valueId);
    if (value !== undefined) {
      next.push({
        propertyId: property.id,
        type: property.type,
        value,
      } as PropertyFilter);
    }
  }
  return next;
}

export function validateNumberRange(
  range: NumberRangeFilter
): string | undefined {
  const lower = range.gt ?? range.gte;
  const upper = range.lt ?? range.lte;
  if (
    (range.gt !== undefined && range.gte !== undefined) ||
    (range.lt !== undefined && range.lte !== undefined) ||
    !Object.values(range).every(
      (value) => value === undefined || Number.isFinite(value)
    )
  ) {
    return 'Enter finite bounds using one operator on each side.';
  }
  if (lower === undefined && upper === undefined) {
    return 'Enter a lower or upper bound.';
  }
  if (lower !== undefined && upper !== undefined) {
    if (lower > upper)
      return 'The lower bound must not exceed the upper bound.';
    if (lower === upper && (range.gt !== undefined || range.lt !== undefined)) {
      return 'Equal bounds must both be inclusive.';
    }
  }
  return undefined;
}

export function numberRangeForProperty(
  properties: readonly PropertyFilter[] | undefined,
  property: TaskCustomProperty
): Extract<PropertyFilter, { type: 'number' }> | undefined {
  if (property.type !== 'number') return;
  const filter = (properties ?? []).find(
    (candidate): candidate is Extract<PropertyFilter, { type: 'number' }> =>
      candidate.propertyId === property.id && candidate.type === 'number'
  );
  return filter && !validateNumberRange(filter.range) ? filter : undefined;
}

export function replaceTaskNumberRange(
  properties: readonly PropertyFilter[] | undefined,
  property: TaskCustomProperty,
  range: NumberRangeFilter | undefined,
  exclude = false
): PropertyFilter[] {
  if (property.type !== 'number' || (range && validateNumberRange(range))) {
    return properties ? [...properties] : [];
  }
  const next = (properties ?? []).filter(
    (filter) => !(filter.propertyId === property.id && filter.type === 'number')
  );
  if (!range) return next;
  next.push({
    propertyId: property.id,
    type: 'number',
    range,
    ...(exclude ? { exclude: true } : {}),
  });
  return next;
}

/** Boolean and Date properties are radio-like; select properties retain OR semantics. */
export function toggleTaskCustomPropertyValue(
  selectedValueIds: readonly string[],
  property: TaskCustomProperty,
  valueId: string
): string[] {
  if (property.type !== 'select' && property.type !== 'entity') {
    return selectedValueIds.includes(valueId) ? [] : [valueId];
  }
  if (property.type === 'entity' && !property.isMultiSelect) {
    return selectedValueIds.includes(valueId) ? [] : [valueId];
  }
  return selectedValueIds.includes(valueId)
    ? selectedValueIds.filter((id) => id !== valueId)
    : [...selectedValueIds, valueId];
}

/** Whether this is a custom-property filter which cannot safely be named. */
export function isUnavailableTaskCustomProperty(
  filter: PropertyFilter,
  properties: readonly TaskCustomProperty[]
): boolean {
  if (SYSTEM_IDS.has(filter.propertyId)) return false;
  const known = properties.find(
    (property) => property.id === filter.propertyId
  );
  if (!known) return true;
  if (known.type === 'number') {
    return (
      filter.type !== 'number' ||
      validateNumberRange(filter.range) !== undefined
    );
  }
  if (known.type === 'entity') {
    return (
      filter.type !== 'entity' ||
      typeof filter.value !== 'string' ||
      filter.value.length === 0
    );
  }
  if (!('value' in filter)) return true;
  return !known.options.some(
    (option) => option.value === filter.value && known.type === filter.type
  );
}

/** Removes only the active entries currently classified as unavailable. */
export function removeUnavailableTaskCustomProperties(
  filters: readonly PropertyFilter[],
  properties: readonly TaskCustomProperty[]
): PropertyFilter[] {
  return filters.filter(
    (filter) => !isUnavailableTaskCustomProperty(filter, properties)
  );
}
