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

vi.mock('@queries/contacts/contacts', () => ({
  useContacts: () => () => [],
}));

import {
  type FilterCategory,
  filterInboxGithubPrOption,
} from './filter-categories';
import { VIEW_FILTER_CATEGORIES } from './unified-filter-dropdown';

const categories: FilterCategory[] = [
  {
    id: 'type',
    label: 'Type',
    options: [
      { id: 'document', label: 'Docs' },
      { id: 'github-pr', label: 'GitHub PRs' },
    ],
    multiple: true,
  },
  {
    id: 'status',
    label: 'Status',
    options: [{ id: 'unread', label: 'Unread' }],
  },
];

describe('filterInboxGithubPrOption', () => {
  it('keeps the GitHub PR option for linked users', () => {
    expect(filterInboxGithubPrOption(categories, true)[0]?.options).toEqual([
      { id: 'document', label: 'Docs' },
      { id: 'github-pr', label: 'GitHub PRs' },
    ]);
  });

  it('removes the GitHub PR option for users without a linked GitHub account', () => {
    expect(filterInboxGithubPrOption(categories, false)[0]?.options).toEqual([
      { id: 'document', label: 'Docs' },
    ]);
  });

  it('leaves other filter categories unchanged', () => {
    expect(filterInboxGithubPrOption(categories, false)[1]).toBe(categories[1]);
  });
});

describe('task filter categories', () => {
  it('maps Milestones to the frozen task-milestone filter', () => {
    expect(VIEW_FILTER_CATEGORIES.tasks).toContainEqual({
      id: 'milestone',
      label: 'Milestones',
      options: [{ id: 'task-milestone', label: 'Milestones' }],
    });
  });
});
