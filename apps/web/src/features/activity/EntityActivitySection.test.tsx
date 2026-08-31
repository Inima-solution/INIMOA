/**
 * @vitest-environment jsdom
 */

import { cleanup, fireEvent, render, waitFor } from '@solidjs/testing-library';
import { type Accessor, createSignal, type JSX, type Setter } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

type QueryState = {
  data?: Array<Record<string, unknown>>;
  isError: boolean;
  isFetching: boolean;
  isLoading: boolean;
  isRefetching: boolean;
  error?: Error;
};

type QueryMock = {
  isEnabled: () => boolean;
  result: {
    readonly data: QueryState['data'];
    readonly isError: boolean;
    readonly isFetching: boolean;
    readonly isLoading: boolean;
    readonly isRefetching: boolean;
    refetch: () => Promise<void>;
  };
};

const mocks = vi.hoisted(() => ({
  createQuery: vi.fn(),
  enabled: true,
  query: {} as QueryMock,
  refetch: vi.fn<() => Promise<void>>(),
}));

vi.mock('@queries/activity/graphql/entity', () => ({
  createEntityActivityQuery: (options: unknown) => {
    mocks.createQuery(options);
    return mocks.query;
  },
}));

vi.mock('./use-entity-activity-flag', () => ({
  useEntityActivityFlag: () => () => mocks.enabled,
}));

vi.mock('@components/app/side-panel/SidePanel', () => ({
  SidePanel: {
    Card: (props: { children: JSX.Element }) => <div>{props.children}</div>,
    EmptyPill: (props: { label: string }) => <span>{props.label}</span>,
    Loading: () => <div>Panel loading</div>,
    Section: (props: { children: JSX.Element; id: string; title: string }) => (
      <section data-section={props.id}>
        <h2>{props.title}</h2>
        {props.children}
      </section>
    ),
  },
}));

vi.mock('@ui', () => ({
  Button: (props: {
    'aria-label'?: string;
    children: JSX.Element;
    disabled?: boolean;
    onClick: () => void;
  }) => (
    <button
      type="button"
      aria-label={props['aria-label']}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  ),
  cn: (...classes: Array<string | false>) => classes.filter(Boolean).join(' '),
}));

vi.mock('./actor-name', () => ({
  ActorName: () => <span>Alex</span>,
}));

vi.mock('./action-phrase', () => ({
  ActionPhrase: () => <span>updated the project</span>,
}));

vi.mock('@entity/utils/timestamp', () => ({
  formatRelativeTimestamp: () => 'now',
}));

import { EntityActivitySectionConditional } from './EntityActivitySection';

let queryState: Accessor<QueryState>;
let setQueryState: Setter<QueryState>;

function activityEvent() {
  return {
    actorId: 'actor-1',
    occurredAt: '2026-09-01T00:00:00.000Z',
  };
}

function renderSection(entityType = 'PROJECT') {
  return render(() => (
    <EntityActivitySectionConditional
      entityId="project-1"
      entityType={entityType as never}
    />
  ));
}

beforeEach(() => {
  [queryState, setQueryState] = createSignal<QueryState>({
    data: undefined,
    isError: false,
    isFetching: false,
    isLoading: false,
    isRefetching: false,
  });
  mocks.enabled = true;
  mocks.createQuery.mockReset();
  mocks.refetch.mockReset();
  mocks.refetch.mockResolvedValue(undefined);
  mocks.query = {
    isEnabled: () => true,
    result: {
      get data() {
        return queryState().data;
      },
      get isError() {
        return queryState().isError;
      },
      get isFetching() {
        return queryState().isFetching;
      },
      get isLoading() {
        return queryState().isLoading;
      },
      get isRefetching() {
        return queryState().isRefetching;
      },
      refetch: () => mocks.refetch(),
    },
  };
});

afterEach(cleanup);

describe('EntityActivitySectionConditional', () => {
  it('does not create a query while the feature is off', () => {
    mocks.enabled = false;

    const view = renderSection();

    expect(mocks.createQuery).not.toHaveBeenCalled();
    expect(view.container.textContent).toBe('');
  });

  it('uses one canonical enabled query and leaves unsupported input unrendered', () => {
    const enabled = renderSection();
    expect(mocks.createQuery).toHaveBeenCalledOnce();
    expect(enabled.getByRole('heading', { name: 'Activity' })).toBeTruthy();
    enabled.unmount();

    mocks.query.isEnabled = () => false;
    const unsupported = renderSection('USER');
    expect(mocks.createQuery).toHaveBeenCalledTimes(2);
    expect(unsupported.queryByRole('heading', { name: 'Activity' })).toBeNull();
  });

  it('uses the side-panel loading fallback only for the initial load', () => {
    setQueryState({
      data: undefined,
      isError: false,
      isFetching: true,
      isLoading: true,
      isRefetching: false,
    });

    const view = renderSection();
    expect(view.getByText('Panel loading')).toBeTruthy();
    expect(view.queryByText('No activity yet')).toBeNull();
  });

  it('renders a genuine successful empty result', () => {
    const view = renderSection();
    expect(view.getByText('No activity yet')).toBeTruthy();
  });

  it('shows generic unavailable copy and an accessible Retry without raw errors', () => {
    setQueryState({
      data: undefined,
      error: new Error('backend failure request-id=secret'),
      isError: true,
      isFetching: false,
      isLoading: false,
      isRefetching: false,
    });

    const view = renderSection();
    expect(view.getByText('Activity is unavailable')).toBeTruthy();
    expect(view.getByRole('button', { name: 'Retry' })).toBeTruthy();
    expect(view.container.textContent).not.toContain('request-id=secret');
    expect(view.queryByText('No activity yet')).toBeNull();
  });

  it('calls refetch once and prevents duplicate retry while it is pending', () => {
    let resolveRetry: (() => void) | undefined;
    mocks.refetch.mockImplementation(() => {
      setQueryState({ ...queryState(), isRefetching: true });
      return new Promise<void>((resolve) => {
        resolveRetry = resolve;
      });
    });
    setQueryState({
      data: undefined,
      isError: true,
      isFetching: false,
      isLoading: false,
      isRefetching: false,
    });

    const view = renderSection();
    fireEvent.click(view.getByRole('button', { name: 'Retry' }));
    const retrying = view.getByRole('button', { name: 'Retrying activity' });
    fireEvent.click(retrying);

    expect(mocks.refetch).toHaveBeenCalledOnce();
    expect((retrying as HTMLButtonElement).disabled).toBe(true);
    setQueryState({ ...queryState(), isRefetching: false });
    resolveRetry?.();
  });

  it('keeps retained events visible with a compact stale notice and recovers', async () => {
    let resolveRetry: (() => void) | undefined;
    mocks.refetch.mockImplementation(() => {
      setQueryState({ ...queryState(), isRefetching: true });
      return new Promise<void>((resolve) => {
        resolveRetry = resolve;
      });
    });
    setQueryState({
      data: [activityEvent()],
      isError: true,
      isFetching: false,
      isLoading: false,
      isRefetching: false,
    });

    const view = renderSection();
    expect(view.getByText('Alex')).toBeTruthy();
    expect(view.getByText('Activity may be out of date.')).toBeTruthy();
    fireEvent.click(view.getByRole('button', { name: 'Retry' }));
    setQueryState({
      data: [activityEvent()],
      isError: false,
      isFetching: false,
      isLoading: false,
      isRefetching: false,
    });
    resolveRetry?.();

    await waitFor(() => {
      expect(view.queryByText('Activity may be out of date.')).toBeNull();
      expect(view.getByText('Alex')).toBeTruthy();
    });
  });
});
