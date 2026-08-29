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
  getSingleSelectFilterPlan,
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

  it('offers the four Due date states as a single-select category', () => {
    expect(VIEW_FILTER_CATEGORIES.tasks).toContainEqual({
      id: 'due-date',
      label: 'Due date',
      options: [
        { id: 'task-due-overdue', label: 'Overdue' },
        { id: 'task-due-today', label: 'Today' },
        { id: 'task-due-upcoming', label: 'Upcoming' },
        { id: 'task-due-none', label: 'No due date' },
      ],
      multiple: false,
    });
  });
});

describe('getSingleSelectFilterPlan', () => {
  const dueDate: FilterCategory = {
    id: 'due-date',
    label: 'Due date',
    options: [
      { id: 'task-due-overdue', label: 'Overdue' },
      { id: 'task-due-today', label: 'Today' },
      { id: 'task-due-upcoming', label: 'Upcoming' },
      { id: 'task-due-none', label: 'No due date' },
    ],
    multiple: false,
  };

  it('clears the active Due date option', () => {
    expect(
      getSingleSelectFilterPlan(
        dueDate,
        'task-due-today',
        (id) => id === 'task-due-today'
      )
    ).toEqual({ deactivate: ['task-due-today'] });
  });

  it('swaps every active Due date sibling without touching unrelated ids', () => {
    expect(
      getSingleSelectFilterPlan(
        dueDate,
        'task-due-upcoming',
        (id) => id === 'task-due-overdue' || id === 'task-due-today'
      )
    ).toEqual({
      deactivate: ['task-due-overdue', 'task-due-today'],
      activate: 'task-due-upcoming',
    });
  });
});
