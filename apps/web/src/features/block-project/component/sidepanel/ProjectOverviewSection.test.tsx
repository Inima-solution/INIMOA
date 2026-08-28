/**
 * @vitest-environment jsdom
 */

import type { GetProjectOverview200DataOneOf } from '@service-storage/generated/schemas';
import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import type { JSX } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  query: {} as Record<string, unknown>,
  refetch: vi.fn(),
  sections: vi.fn(),
}));

vi.mock('@core/block', () => ({
  useBlockId: () => 'project-1',
}));

vi.mock('@queries/storage/project-overview', () => ({
  useProjectOverviewQuery: () => mocks.query,
}));

vi.mock('@core/util/result', () => ({
  thrownResultErrorHasCode: (
    error: { codes?: string[] } | undefined,
    code: string
  ) => error?.codes?.includes(code) ?? false,
}));

vi.mock('@core/util/date', () => ({
  formatDate: () => 'Feb 4, 2026, 3:04 PM',
}));

vi.mock('@ui', () => ({
  Button: (props: { children: string; onClick: () => void }) => (
    <button type="button" onClick={props.onClick}>
      {props.children}
    </button>
  ),
}));

vi.mock('@components/app/side-panel', () => ({
  OwnerValue: () => <span>Lead Display</span>,
  SidePanel: {
    Section: (props: {
      children: JSX.Element;
      defaultOpen?: boolean;
      id: string;
      order?: number;
      title: string;
    }) => {
      mocks.sections(props);
      return (
        <section
          data-default-open={String(props.defaultOpen)}
          data-order={props.order}
          data-section={props.id}
        >
          <h2>{props.title}</h2>
          {props.children}
        </section>
      );
    },
    Grid: (props: { children: JSX.Element }) => <div>{props.children}</div>,
    Row: (props: { children: JSX.Element; label: string }) => (
      <div data-row={props.label}>
        <span>{props.label}</span>
        {props.children}
      </div>
    ),
    Pill: (props: { children: JSX.Element }) => <span>{props.children}</span>,
    EmptyPill: (props: { label: string }) => <span>{props.label}</span>,
    Loading: () => <div>Panel loading</div>,
  },
}));

vi.mock('@app/features/activity/EntityActivitySection', () => ({
  EntityActivitySectionConditional: () => null,
}));

vi.mock('@app/features/property/side-panel/properties', () => ({
  EntityPropertiesSection: () => null,
  EntityTagsSection: () => null,
}));

vi.mock('@core/signal/permissions', () => ({
  useCanEdit: () => () => false,
}));

vi.mock('@core/util/currentBlockDocumentName', () => ({
  useBlockDocumentName: () => () => 'Project',
}));

import { ProjectOverviewSection } from './ProjectOverviewSection';
import { ProjectSidePanelSections } from './ProjectSidePanelSections';

const populatedOverview: GetProjectOverview200DataOneOf = {
  operations: {
    status: 'active',
    priority: 'urgent',
    leadUserId: 'macro|lead@example.com',
    startDate: '2026-01-02',
    targetDate: '2026-02-03',
    completedAt: '2026-02-04T15:04:00.000Z',
    policy: { hidden: true },
    createdAt: '2026-01-01T00:00:00.000Z',
    projectId: 'project-1',
    updatedAt: '2026-01-01T00:00:00.000Z',
  },
  immediateChildren: {
    childProjects: 2,
    tasks: 3,
    nonTaskDocuments: 4,
    chats: 5,
  },
  project: {
    id: 'project-1',
    name: 'Project',
    userId: 'macro|lead@example.com',
  },
  userAccessLevel: 'view',
};

function readyQuery(data: GetProjectOverview200DataOneOf = populatedOverview) {
  return {
    data,
    error: undefined,
    fetchStatus: 'idle',
    isError: false,
    isPending: false,
    refetch: mocks.refetch,
  };
}

function renderOverview() {
  return render(() => <ProjectOverviewSection order={5} />);
}

beforeEach(() => {
  mocks.refetch.mockReset();
  mocks.sections.mockReset();
  mocks.query = readyQuery();
});

afterEach(cleanup);

describe('ProjectOverviewSection', () => {
  it('renders only canonical populated operations and exact depth-one counts', () => {
    const view = renderOverview();
    const formatDateOnly = (value: string) =>
      new Intl.DateTimeFormat(undefined, {
        dateStyle: 'medium',
        timeZone: 'UTC',
      }).format(new Date(`${value}T00:00:00Z`));

    expect(view.getByText('Active')).toBeTruthy();
    expect(view.getByText('Urgent')).toBeTruthy();
    expect(view.getByText('Lead Display')).toBeTruthy();
    expect(view.getByText(formatDateOnly('2026-01-02'))).toBeTruthy();
    expect(view.getByText(formatDateOnly('2026-02-03'))).toBeTruthy();
    expect(view.getByText('Feb 4, 2026, 3:04 PM')).toBeTruthy();
    expect(
      view.container.querySelector('[data-row="Child projects"]')?.textContent
    ).toContain('2');
    expect(
      view.container.querySelector('[data-row="Tasks"]')?.textContent
    ).toContain('3');
    expect(
      view.container.querySelector('[data-row="Files"]')?.textContent
    ).toContain('4');
    expect(
      view.container.querySelector('[data-row="Chats"]')?.textContent
    ).toContain('5');
    expect(view.container.textContent).not.toContain('macro|lead@example.com');
    expect(view.container.textContent).not.toContain('hidden');
    expect(view.container.textContent).not.toMatch(
      /objective|progress|risk|next action/i
    );
    expect(view.container.querySelector('.tabular-nums')?.textContent).toBe(
      '2'
    );
  });

  it('renders zero counts and empty values without cards', () => {
    mocks.query = readyQuery({
      ...populatedOverview,
      operations: {
        ...populatedOverview.operations,
        status: 'planned',
        priority: 'normal',
        leadUserId: null,
        startDate: null,
        targetDate: null,
        completedAt: null,
      },
      immediateChildren: {
        childProjects: 0,
        tasks: 0,
        nonTaskDocuments: 0,
        chats: 0,
      },
    });
    const view = renderOverview();

    expect(view.getByText('Unassigned')).toBeTruthy();
    expect(view.getAllByText('Not set')).toHaveLength(2);
    for (const label of ['Child projects', 'Tasks', 'Files', 'Chats']) {
      expect(
        view.container.querySelector(`[data-row="${label}"]`)?.textContent
      ).toContain('0');
    }
    expect(view.container.querySelector('.card')).toBeNull();
  });

  it.each(['FORBIDDEN', 'UNAUTHORIZED'])(
    'renders only permission copy for %s',
    (code) => {
      mocks.query = {
        ...readyQuery(),
        error: { codes: [code] },
        isError: true,
      };
      const view = renderOverview();

      expect(
        view.getByText('Project overview is not available to you.')
      ).toBeTruthy();
      expect(view.container.querySelector('[data-row]')).toBeNull();
      expect(view.container.textContent).not.toContain('Lead Display');
      expect(view.container.textContent).not.toContain('Child projects');
    }
  );

  it('keeps ready data during a paused fetch and otherwise shows only offline copy', () => {
    mocks.query = {
      ...readyQuery(),
      data: undefined,
      fetchStatus: 'paused',
      isPending: true,
    };
    const offline = renderOverview();
    expect(
      offline.getByText('Project overview is unavailable while offline.')
    ).toBeTruthy();
    expect(offline.container.querySelector('[data-row]')).toBeNull();
    offline.unmount();

    mocks.query = { ...readyQuery(), fetchStatus: 'paused' };
    const stale = renderOverview();
    expect(stale.getByText('Active')).toBeTruthy();
    expect(
      stale
        .getByText('Showing the last loaded overview while offline.')
        .getAttribute('aria-live')
    ).toBe('polite');
  });

  it('offers one retry for a generic error without leaking counts', () => {
    mocks.query = {
      ...readyQuery(),
      data: undefined,
      error: { codes: ['UNKNOWN'] },
      isError: true,
    };
    const view = renderOverview();

    fireEvent.click(view.getByRole('button', { name: 'Retry' }));
    expect(mocks.refetch).toHaveBeenCalledOnce();
    expect(view.container.querySelector('[data-row]')).toBeNull();
    expect(view.container.textContent).not.toContain('Child projects');
  });

  it('uses the side-panel loading fallback with a polite status', () => {
    mocks.query = {
      data: undefined,
      error: undefined,
      fetchStatus: 'fetching',
      isError: false,
      isPending: true,
      refetch: mocks.refetch,
    };
    const view = renderOverview();

    expect(
      view
        .getByRole('status', { name: 'Loading overview' })
        .getAttribute('aria-live')
    ).toBe('polite');
    expect(view.getByText('Panel loading')).toBeTruthy();
  });

  it('registers the default-open overview before Details', () => {
    render(() => <ProjectSidePanelSections />);
    const registrations = mocks.sections.mock.calls.map(([props]) => props);
    const overviewIndex = registrations.findIndex(
      (props) => props.id === 'project-overview'
    );
    const detailsIndex = registrations.findIndex(
      (props) => props.id === 'details'
    );
    const overview = registrations[overviewIndex];

    expect(overview).toMatchObject({
      id: 'project-overview',
      order: 5,
      defaultOpen: true,
    });
    expect(overviewIndex).toBeLessThan(detailsIndex);
  });
});
