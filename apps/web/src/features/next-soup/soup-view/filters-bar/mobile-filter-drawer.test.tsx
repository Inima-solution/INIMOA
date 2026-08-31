import { fireEvent, render, screen } from '@solidjs/testing-library';
import type { JSX } from 'solid-js';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const state = vi.hoisted(() => {
  const activeIds = new Set<string>();
  const queryFilters = {
    state: { include: {} as { properties?: unknown[] } },
    add: vi.fn(),
    remove: vi.fn(),
    set: vi.fn((next: { include: { properties?: unknown[] } }) => {
      queryFilters.state.include = {
        ...queryFilters.state.include,
        ...next.include,
      };
    }),
  };
  const predicates = {
    activeIds,
    isActive: (id: string) => activeIds.has(id),
    getConfig: (id: string) => ({
      query: {
        include: {
          subType: ['task'],
          properties: [
            {
              propertyId: 'due-date',
              type: 'date',
              value: id.replace('task-due-', ''),
            },
          ],
        },
      },
    }),
    set: (
      next: (value: { andIds: string[]; orIds: string[] }) => {
        and: string[];
        or: string[];
      }
    ) => {
      const value = next({ andIds: [], orIds: [...activeIds] });
      activeIds.clear();
      for (const id of value.or) activeIds.add(id);
    },
    toggle: vi.fn(),
  };
  return {
    activeIds,
    predicates,
    queryFilters,
    propertyDefinitions: [] as unknown[],
  };
});

vi.mock('@app/constants/list-views', () => ({
  isListViewID: (id: string) => id === 'tasks',
}));
vi.mock('@app/features/next-soup/filters', () => ({
  NO_ASSIGNEE: 'NO_ASSIGNEE',
}));
vi.mock('@app/features/next-soup/sidebar/soup-filter-presets', () => ({
  getViewPreset: () => undefined,
}));
vi.mock('@app/features/next-soup/soup-view/sort-options', () => ({
  CHANNEL_SORT_OPTIONS: [],
  DEFAULT_SORT_OPTIONS: [],
  DOCUMENT_SORT_OPTIONS: [],
  EMAIL_SORT_OPTIONS: [],
  TASK_SORT_OPTIONS: [],
}));
vi.mock('@app/features/next-soup/soup-view/soup-view-context', () => ({
  useSoupView: () => ({
    soup: { predicates: state.predicates, sort: { active: () => [] } },
    queryFilters: state.queryFilters,
    assigneeFilter: () => [],
    activeTab: () => undefined,
    inboxFilter: () => undefined,
    setInboxFilter: vi.fn(),
  }),
}));
vi.mock('@app/lib/analytics/posthog', () => ({
  useFeatureFlag: () => () => ({ enabled: false }),
}));
vi.mock('@components/app/mobile/MobileDrawer', () => {
  const passthrough = (props: JSX.HTMLAttributes<HTMLElement>) =>
    props.children;
  return {
    MobileDrawer: Object.assign(passthrough, {
      Trigger: passthrough,
      Portal: passthrough,
      Overlay: passthrough,
      Content: passthrough,
      Handle: passthrough,
      Label: passthrough,
      Section: passthrough,
    }),
    scrollToFocusedInput: vi.fn(),
  };
});
vi.mock('@components/app/mobile/pressPulse', () => ({ pressPulse: vi.fn() }));
vi.mock('@components/app/split-layout/layoutUtils', () => ({
  useSplitPanelOrThrow: () => ({
    handle: { content: () => ({ type: 'component', id: 'tasks' }) },
  }),
}));
vi.mock('@core/context/user', () => ({ useUserId: () => () => 'user-1' }));
vi.mock('@core/email-link', () => ({ useAddInboxFlow: () => vi.fn() }));
vi.mock('@kobalte/core/accordion', () => {
  const passthrough = (props: JSX.HTMLAttributes<HTMLElement>) =>
    props.children;
  const content = (props: JSX.HTMLAttributes<HTMLDivElement>) => (
    <div {...props} />
  );
  return {
    Accordion: Object.assign(passthrough, {
      Item: passthrough,
      Header: passthrough,
      Trigger: passthrough,
      Content: content,
    }),
  };
});
vi.mock('@queries/contacts/contacts', () => ({ useContacts: () => () => [] }));
vi.mock('@queries/properties/definitions', () => ({
  useListPropertiesQuery: () => ({ data: state.propertyDefinitions }),
}));
vi.mock('@ui', () => ({
  Button: 'button',
  cn: (...values: string[]) => values.join(' '),
}));
vi.mock('./consolidated-filter-chip', () => ({
  ConsolidatedFilterChip: () => null,
}));
vi.mock('./inbox-picker', () => ({
  useInboxPicker: () => ({
    activeIds: () => [],
    onChange: vi.fn(),
    reset: vi.fn(),
    hasMultiple: () => false,
  }),
}));
vi.mock('./unified-filter-dropdown', () => ({
  buildContactLabel: () => '',
  VIEW_FILTER_CATEGORIES: {
    tasks: [
      {
        id: 'due-date',
        label: 'Due date',
        multiple: false,
        options: [
          { id: 'task-due-overdue', label: 'Overdue' },
          { id: 'task-due-today', label: 'Today' },
          { id: 'task-due-upcoming', label: 'Upcoming' },
          { id: 'task-due-none', label: 'No due date' },
        ],
      },
    ],
  },
}));
vi.mock('./use-filter-refinements', () => ({
  useFilterRefinements: () => ({
    consolidatedFiltersList: () => [],
    resetToTabDefaults: vi.fn(),
    handleAssigneeChange: vi.fn(),
  }),
}));
vi.mock('@core/component/VerticalScrollIndicators', () => ({
  ScrollIndicators: (props: { children?: unknown }) => props.children,
}));

import { MobileFilterDrawer } from './mobile-filter-drawer';

beforeEach(() => {
  state.activeIds.clear();
  state.queryFilters.state.include = {};
  state.queryFilters.add.mockClear();
  state.queryFilters.remove.mockClear();
  state.queryFilters.set.mockClear();
  state.propertyDefinitions = [];
});

describe('MobileFilterDrawer due date', () => {
  it('renders Due date as radio choices and swaps the active semantic query', () => {
    state.activeIds.add('task-high-priority');
    const view = render(() => <MobileFilterDrawer />);

    const overdue = screen.getByRole('radio', { name: 'Overdue' });
    const today = screen.getByRole('radio', { name: 'Today' });
    expect(screen.getByRole('radiogroup')).toBeTruthy();
    expect(overdue.getAttribute('aria-checked')).toBe('false');

    fireEvent.click(overdue);
    expect(state.activeIds).toEqual(
      new Set(['task-high-priority', 'task-due-overdue'])
    );
    expect(state.queryFilters.add).toHaveBeenLastCalledWith(
      expect.objectContaining({
        include: expect.objectContaining({
          properties: [expect.objectContaining({ value: 'overdue' })],
        }),
      })
    );

    fireEvent.click(today);
    expect(state.activeIds).toEqual(
      new Set(['task-high-priority', 'task-due-today'])
    );
    expect(state.queryFilters.remove).toHaveBeenLastCalledWith(
      expect.objectContaining({
        include: expect.objectContaining({
          properties: [expect.objectContaining({ value: 'overdue' })],
        }),
      })
    );
    fireEvent.click(today);
    expect(state.activeIds).toEqual(new Set(['task-high-priority']));
    expect(state.queryFilters.remove).toHaveBeenLastCalledWith(
      expect.objectContaining({
        include: expect.objectContaining({
          properties: [expect.objectContaining({ value: 'today' })],
        }),
      })
    );

    view.unmount();
  });

  it('renders supported task custom properties with boolean radio and select checkbox semantics', () => {
    state.propertyDefinitions = [
      {
        definition: {
          id: 'ready',
          display_name: 'Ready',
          data_type: 'BOOLEAN',
          is_system: false,
        },
        property_options: [],
      },
      {
        definition: {
          id: 'status',
          display_name: 'Status',
          data_type: 'SELECT_STRING',
          is_system: false,
        },
        property_options: [
          {
            id: 'open',
            property_definition_id: 'status',
            value: { type: 'string', value: 'Open' },
          },
          {
            id: 'closed',
            property_definition_id: 'status',
            value: { type: 'string', value: 'Closed' },
          },
        ],
      },
      {
        definition: {
          id: 'unsupported',
          display_name: 'Unsupported',
          data_type: 'STRING',
          is_system: false,
        },
        property_options: [],
      },
      {
        definition: {
          id: 'empty',
          display_name: 'Empty',
          data_type: 'SELECT_STRING',
          is_system: false,
        },
        property_options: [],
      },
    ];
    const view = render(() => <MobileFilterDrawer />);

    expect(screen.getByRole('radiogroup', { name: 'Ready' })).toBeTruthy();
    expect(screen.getByRole('radio', { name: 'True' })).toBeTruthy();
    expect(screen.getByRole('radio', { name: 'False' })).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: 'Open' })).toBeTruthy();
    expect(screen.queryByText('Unsupported')).toBeNull();
    expect(screen.queryByText('Empty')).toBeNull();

    fireEvent.click(screen.getByRole('radio', { name: 'True' }));
    expect(state.queryFilters.set).toHaveBeenLastCalledWith({
      include: {
        properties: [{ propertyId: 'ready', type: 'boolean', value: true }],
      },
    });
    fireEvent.click(screen.getByRole('radio', { name: 'False' }));
    expect(state.queryFilters.set).toHaveBeenLastCalledWith({
      include: {
        properties: [{ propertyId: 'ready', type: 'boolean', value: false }],
      },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Open' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Closed' }));
    expect(state.queryFilters.set).toHaveBeenLastCalledWith({
      include: {
        properties: [
          { propertyId: 'ready', type: 'boolean', value: false },
          { propertyId: 'status', type: 'select', value: 'open' },
          { propertyId: 'status', type: 'select', value: 'closed' },
        ],
      },
    });
    view.unmount();
  });
});
