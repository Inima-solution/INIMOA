import type { FilterID } from '@app/features/next-soup/filters';
import type { JSX } from 'solid-js';

export type FilterOption = {
  id: FilterID;
  label: string;
  icon?: () => JSX.Element;
};

export type FilterCategory = {
  id: string;
  label: string;
  /** Plural form for multi-value chip display (e.g., 'Types', 'Statuses') */
  labelPlural?: string;
  options: FilterOption[];
  multiple?: boolean;
};

export type SingleSelectFilterPlan<TId extends string> = {
  deactivate: TId[];
  activate?: TId;
};

/**
 * Produces an atomic selection change for a single-value category. Selecting
 * the active value clears it; selecting a sibling clears every active value
 * in the category before activating the sibling.
 */
export function getSingleSelectFilterPlan<TId extends string>(
  category: { options: readonly { id: TId }[] },
  optionId: TId,
  isActive: (id: TId) => boolean
): SingleSelectFilterPlan<TId> {
  const activeIds = category.options
    .map((option) => option.id)
    .filter(isActive);

  if (isActive(optionId)) return { deactivate: [optionId] };

  return { deactivate: activeIds, activate: optionId };
}

export function filterInboxGithubPrOption(
  categories: FilterCategory[],
  hasGithub: boolean
): FilterCategory[] {
  if (hasGithub) return categories;

  return categories.map((category) =>
    category.id === 'type'
      ? {
          ...category,
          options: category.options.filter(
            (option) => option.id !== 'github-pr'
          ),
        }
      : category
  );
}
