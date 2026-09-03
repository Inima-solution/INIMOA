/** @vitest-environment jsdom */

import { ThrownResultError } from '@core/util/result';
import type { GetProjectOverview200DataOneOf } from '@service-storage/generated/schemas';
import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProjectReportView } from './ProjectReportView';

const overview: GetProjectOverview200DataOneOf = {
  operations: {
    status: 'active',
    priority: 'normal',
    leadUserId: null,
    startDate: null,
    targetDate: null,
    objective: null,
    nextAction: null,
    completedAt: null,
    policy: null,
    createdAt: '2026-09-01T00:00:00Z',
    projectId: 'project-1',
    updatedAt: '2026-09-01T00:00:00Z',
  },
  immediateChildren: {
    childProjects: 0,
    tasks: 3,
    nonTaskDocuments: 0,
    chats: 0,
  },
  progress: {
    completedTasks: 2,
    includedTasks: 3,
    hasUnavailableStatuses: false,
  },
  project: {
    id: 'project-1',
    name: 'Project',
    userId: 'macro|owner@example.com',
  },
  risk: {
    overdueTasks: 1,
    blockedTasks: 2,
    unassignedTasks: 0,
    approachingTarget: false,
    hasUnavailableRiskData: false,
  },
  userAccessLevel: 'view',
};

function query(data: GetProjectOverview200DataOneOf | undefined = overview) {
  return {
    data,
    error: undefined as unknown,
    fetchStatus: 'idle',
    isError: false,
    isPending: data === undefined,
    refetch: vi.fn(),
  };
}

afterEach(cleanup);

describe('ProjectReportView', () => {
  it('renders exact overview aggregates without reading paged task rows', () => {
    const view = render(() => <ProjectReportView query={query() as never} />);

    expect(view.getByText('67%')).toBeTruthy();
    expect(view.getByText('2 of 3 complete')).toBeTruthy();
    expect(view.getByText('Overdue').parentElement?.textContent).toContain('1');
    expect(view.getByText('Blocked').parentElement?.textContent).toContain('2');
    expect(
      view.getByText(/Throughput and lead time are unavailable/)
    ).toBeTruthy();
  });

  it('supports simultaneous split instances without duplicate main landmarks or ids', () => {
    const view = render(() => (
      <>
        <ProjectReportView query={query() as never} />
        <ProjectReportView query={query() as never} />
      </>
    ));

    expect(view.queryAllByRole('main')).toHaveLength(0);
    expect(
      view.getAllByRole('region', { name: 'Current health' })
    ).toHaveLength(2);
    const ids = [...view.container.querySelectorAll('[id]')].map(
      (element) => element.id
    );
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('renders zero denominator and fail-closed unavailable aggregates', () => {
    const zero = render(() => (
      <ProjectReportView
        query={
          query({
            ...overview,
            progress: {
              completedTasks: 0,
              includedTasks: 0,
              hasUnavailableStatuses: false,
            },
          }) as never
        }
      />
    ));
    expect(zero.getByText('N/A')).toBeTruthy();
    expect(zero.getByText('No eligible tasks')).toBeTruthy();
    zero.unmount();

    const unavailable = render(() => (
      <ProjectReportView
        query={
          query({
            ...overview,
            progress: { ...overview.progress, hasUnavailableStatuses: true },
            risk: { ...overview.risk, hasUnavailableRiskData: true },
          }) as never
        }
      />
    ));
    expect(unavailable.getAllByText('Unavailable')).toHaveLength(3);
    expect(
      unavailable.getByText('Task status data needs attention')
    ).toBeTruthy();
  });

  it('preserves access, retry, loading, and offline states', () => {
    const denied = query();
    denied.error = new ThrownResultError([
      { code: 'FORBIDDEN', message: 'forbidden' },
    ]) as never;
    denied.isError = true;
    const deniedView = render(() => (
      <ProjectReportView query={denied as never} />
    ));
    expect(
      deniedView.getByText('Project report is not available to you.')
    ).toBeTruthy();
    expect(deniedView.queryByText('67%')).toBeNull();
    deniedView.unmount();

    const failed = query();
    failed.error = new Error('failed');
    failed.isError = true;
    const failedView = render(() => (
      <ProjectReportView query={failed as never} />
    ));
    fireEvent.click(failedView.getByRole('button', { name: 'Retry' }));
    expect(failed.refetch).toHaveBeenCalledOnce();
    failedView.unmount();

    const loading = { ...query(), data: undefined, isPending: true };
    const loadingView = render(() => (
      <ProjectReportView query={loading as never} />
    ));
    expect(loadingView.getByText('Loading project report…')).toBeTruthy();
    loadingView.unmount();

    const offline = { ...query(), data: undefined, isPending: true };
    offline.fetchStatus = 'paused';
    const offlineView = render(() => (
      <ProjectReportView query={offline as never} />
    ));
    expect(
      offlineView.getByText('Project report is unavailable while offline.')
    ).toBeTruthy();
  });
});
