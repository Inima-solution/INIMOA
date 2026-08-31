/** @vitest-environment jsdom */

import type { TaskEntityWithProperties } from '@entity';
import { SYSTEM_PROPERTY_IDS } from '@property/constants';
import type { SoupProperty } from '@service-storage/generated/schemas';
import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProjectTaskDeadlineTimeline } from './ProjectTaskDeadlineTimeline';

const mocks = vi.hoisted(() => ({
  relationCalls: [] as Array<{
    taskId: string;
    task?: TaskEntityWithProperties;
    mode?: string;
  }>,
}));

vi.mock('@core/component/LoadingBlock', () => ({
  LoadingBlock: () => <div data-testid="loading-block">Loading tasks</div>,
}));
vi.mock('@entity', () => ({
  Entity: {
    Icon: () => <span aria-hidden="true" />,
    Title: (props: { entity: TaskEntityWithProperties }) => props.entity.name,
  },
}));
vi.mock('@property/task-dependency-relations', () => ({
  TaskDependencyRelations: (props: {
    taskId: string;
    task?: TaskEntityWithProperties;
    mode?: string;
  }) => {
    mocks.relationCalls.push(props);
    return (
      <span
        aria-hidden="true"
        data-testid="task-dependency-relation"
        data-mode={props.mode}
      />
    );
  },
}));

afterEach(() => {
  cleanup();
  mocks.relationCalls = [];
});

function task(
  name: string,
  dueDate?: unknown,
  milestone = false,
  startDate?: unknown
): TaskEntityWithProperties {
  const properties: SoupProperty[] = [];
  if (dueDate !== undefined) {
    properties.push({
      definition: { id: SYSTEM_PROPERTY_IDS.DUE_DATE },
      value: dueDate,
    } as unknown as SoupProperty);
  }
  if (startDate !== undefined) {
    properties.push({
      definition: { id: SYSTEM_PROPERTY_IDS.START_DATE },
      value: startDate,
    } as unknown as SoupProperty);
  }
  if (milestone) {
    properties.push({
      definition: { id: SYSTEM_PROPERTY_IDS.MILESTONE },
      value: { type: 'Boolean', value: true },
    } as unknown as SoupProperty);
  }
  return {
    id: `task-${name}`,
    name,
    ownerId: 'owner-id',
    type: 'document',
    fileType: 'md',
    subType: { type: 'task' },
    properties,
  };
}

function due(value: string) {
  return { type: 'Date', value };
}

describe('ProjectTaskDeadlineTimeline', () => {
  it('groups valid deadlines chronologically by local date and preserves source order within a day', () => {
    const first = task('First same day', due('2026-04-10T08:00:00'));
    const earlier = task('Earlier', due('2026-04-09T18:00:00'));
    const second = task('Second same day', due('2026-04-10T17:00:00'));
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[first, earlier, second]}
        onOpenTask={() => {}}
      />
    ));

    const labels = [
      new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(
        new Date('2026-04-09T18:00:00')
      ),
      new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(
        new Date('2026-04-10T08:00:00')
      ),
    ];
    const headings = view.getAllByRole('heading', { level: 2 });
    expect(view.getAllByRole('region')).toHaveLength(1);
    expect(headings.map((heading) => heading.textContent)).toEqual(labels);
    expect(
      view
        .getAllByRole('list')
        .map((list) => list.getAttribute('aria-labelledby'))
    ).toEqual(headings.map((heading) => heading.id));
    expect(
      Array.from(
        view.getByRole('list', { name: labels[1] }).querySelectorAll('button')
      ).map((button) => button.textContent)
    ).toEqual(['First same day', 'Second same day']);
  });

  it('keeps missing, malformed, and non-Date values in the final Unscheduled group', () => {
    const scheduled = task('Scheduled', due('2026-04-10T08:00:00'));
    const missing = task('Missing');
    const malformed = task('Malformed', due('not-a-date'));
    const nonDate = task('Not Date', { type: 'Text', value: '2026-04-11' });
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[missing, scheduled, malformed, nonDate]}
        onOpenTask={() => {}}
      />
    ));

    expect(view.getAllByRole('region')).toHaveLength(1);
    expect(view.getAllByRole('heading', { level: 2 }).at(-1)?.textContent).toBe(
      'Unscheduled'
    );
    expect(
      Array.from(
        view
          .getByRole('list', { name: 'Unscheduled' })
          .querySelectorAll('button')
      ).map((button) => button.textContent)
    ).toEqual(['Missing', 'Malformed', 'Not Date']);
    expect(view.queryByText(/span/i)).toBeNull();
  });

  it('renders schedule spans and invalid ranges while keeping deadline points and unscheduled tasks neutral', () => {
    const span = task(
      'Span',
      due('2026-04-12T08:00:00'),
      false,
      due('2026-04-10T08:00:00')
    );
    const point = task(
      'Point',
      due('2026-04-12T09:00:00'),
      false,
      due('2026-04-12T18:00:00')
    );
    const invalid = task(
      'Invalid',
      due('2026-04-12T10:00:00'),
      false,
      due('2026-04-13T10:00:00')
    );
    const unscheduled = task(
      'Unscheduled with start',
      undefined,
      false,
      due('2026-04-10T08:00:00')
    );
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[span, point, invalid, unscheduled]}
        onOpenTask={() => {}}
      />
    ));

    const startLabel = new Intl.DateTimeFormat(undefined, {
      dateStyle: 'medium',
    }).format(new Date('2026-04-10T08:00:00'));
    const dueLabel = new Intl.DateTimeFormat(undefined, {
      dateStyle: 'medium',
    }).format(new Date('2026-04-12T08:00:00'));
    const formattedSpan = `${startLabel} – ${dueLabel}`;
    const spanText = view.getByText(formattedSpan);
    const metadata = spanText.parentElement;
    expect(spanText.classList.contains('text-ink-muted')).toBe(true);
    expect(spanText.classList.contains('max-w-40')).toBe(true);
    expect(spanText.classList.contains('min-w-0')).toBe(true);
    expect(spanText.classList.contains('truncate')).toBe(true);
    expect(spanText.getAttribute('aria-label')).toBe(
      `Start ${startLabel}; due ${dueLabel}`
    );
    expect(spanText.getAttribute('title')).toBe(
      `Start ${startLabel}; due ${dueLabel}`
    );
    expect(metadata?.classList.contains('flex')).toBe(true);
    expect(metadata?.classList.contains('max-w-1/2')).toBe(true);
    expect(metadata?.classList.contains('min-w-0')).toBe(true);
    expect(metadata?.classList.contains('shrink-0')).toBe(true);
    expect(metadata?.classList.contains('items-center')).toBe(true);
    expect(metadata?.classList.contains('gap-2')).toBe(true);
    expect(metadata?.classList.contains('overflow-hidden')).toBe(true);
    expect(
      metadata?.querySelector('[data-testid="task-dependency-relation"]')
    ).toBeTruthy();
    expect(
      view.getByRole('button', {
        name: `Span Start ${startLabel}; due ${dueLabel}`,
      })
    ).toBeTruthy();
    expect(view.getByText('Start date is after due date')).toBeTruthy();
    expect(view.getByRole('button', { name: 'Point' }).textContent).toBe(
      'Point'
    );
    expect(
      view.getByRole('button', { name: 'Unscheduled with start' }).textContent
    ).toBe('Unscheduled with start');
  });

  it('uses instance-unique heading IDs while preserving each list association', () => {
    const first = task('First project', due('2026-04-10T08:00:00'));
    const second = task('Second project', due('2026-04-11T08:00:00'));
    const view = render(() => (
      <>
        <ProjectTaskDeadlineTimeline tasks={[first]} onOpenTask={() => {}} />
        <ProjectTaskDeadlineTimeline tasks={[second]} onOpenTask={() => {}} />
      </>
    ));

    const headings = view.getAllByRole('heading', { level: 2 });
    const lists = view.getAllByRole('list');
    expect(view.getAllByRole('region')).toHaveLength(2);
    expect(new Set(headings.map((heading) => heading.id)).size).toBe(
      headings.length
    );
    expect(
      lists.map((list) => {
        const heading = document.getElementById(
          list.getAttribute('aria-labelledby') ?? ''
        );
        return [heading?.textContent, list.getAttribute('aria-labelledby')];
      })
    ).toEqual(headings.map((heading) => [heading.textContent, heading.id]));
  });

  it('uses native task buttons and exposes a labeled region', async () => {
    const milestone = task(
      'Deadline milestone',
      due('2026-04-10T08:00:00'),
      true
    );
    const ordinary = task('Ordinary task', due('2026-04-10T10:00:00'));
    const onOpenTask = vi.fn();
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[milestone, ordinary]}
        onOpenTask={onOpenTask}
      />
    ));
    const button = view.getByRole('button', { name: /Deadline milestone/ });

    expect(
      view.getByRole('region', { name: 'Project task deadline timeline' })
    ).toBeTruthy();
    expect(view.getAllByRole('region')).toHaveLength(1);
    expect(button.tagName).toBe('BUTTON');
    expect(button.classList.contains('touch:min-h-11')).toBe(true);
    fireEvent.click(button, { altKey: true });
    expect(onOpenTask).toHaveBeenCalledWith(milestone, expect.any(MouseEvent));
    expect(onOpenTask.mock.calls[0][1].altKey).toBe(true);
    const user = userEvent.setup();
    button.focus();
    await user.keyboard('{Enter}');
    expect(onOpenTask).toHaveBeenCalledTimes(2);
  });

  it('renders one row-mode dependency relation for every visible task in source order without a standalone milestone label', () => {
    const first = task('First', due('2026-04-10T08:00:00'), true);
    const second = task('Second', due('2026-04-10T10:00:00'));
    const third = task('Third', due('2026-04-10T12:00:00'));
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[first, second, third]}
        onOpenTask={() => {}}
      />
    ));

    expect(mocks.relationCalls).toEqual([
      { taskId: first.id, task: first, mode: 'row' },
      { taskId: second.id, task: second, mode: 'row' },
      { taskId: third.id, task: third, mode: 'row' },
    ]);
    expect(view.getAllByTestId('task-dependency-relation')).toHaveLength(3);
    expect(view.queryByText('Milestone')).toBeNull();
  });

  it('does not invoke the relation renderer when the timeline has no visible tasks', () => {
    render(() => (
      <ProjectTaskDeadlineTimeline tasks={[]} onOpenTask={() => {}} />
    ));

    expect(mocks.relationCalls).toEqual([]);
  });

  it('distinguishes loading, initial error, empty/search-empty, retained error, and bounded paging', () => {
    const item = task('Retained', due('2026-04-10T08:00:00'));
    const loading = render(() => (
      <ProjectTaskDeadlineTimeline tasks={[]} loading onOpenTask={() => {}} />
    ));
    expect(loading.getByTestId('loading-block')).toBeTruthy();
    loading.unmount();

    const initialErrorRetry = vi.fn();
    const initialError = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[]}
        error
        onRetry={initialErrorRetry}
        onOpenTask={() => {}}
      />
    ));
    fireEvent.click(initialError.getByRole('button', { name: 'Retry' }));
    expect(initialErrorRetry).toHaveBeenCalledTimes(1);
    initialError.unmount();

    const empty = render(() => (
      <ProjectTaskDeadlineTimeline tasks={[]} onOpenTask={() => {}} />
    ));
    expect(empty.getByText('No tasks in this project')).toBeTruthy();
    empty.unmount();

    const searchEmpty = render(() => (
      <ProjectTaskDeadlineTimeline tasks={[]} searching onOpenTask={() => {}} />
    ));
    expect(searchEmpty.getByText('No tasks match this search')).toBeTruthy();
    searchEmpty.unmount();

    const loadMore = vi.fn();
    const retained = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[item]}
        error
        hasNextPage
        onLoadMore={loadMore}
        onOpenTask={() => {}}
      />
    ));
    expect(retained.getByRole('button', { name: 'Retained' })).toBeTruthy();
    expect(retained.queryByText('Couldn’t load tasks')).toBeNull();
    fireEvent.click(
      retained.getByRole('button', { name: 'Retry loading more' })
    );
    expect(loadMore).toHaveBeenCalledTimes(1);
    expect(
      retained.getAllByRole('button', { name: 'Retry loading more' })
    ).toHaveLength(1);
  });

  it('keeps retained final-page tasks visible with one safe source retry', () => {
    const item = task('Retained final page', due('2026-04-10T08:00:00'));
    const onRetry = vi.fn();
    const onLoadMore = vi.fn();
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[item]}
        error
        onRetry={onRetry}
        onLoadMore={onLoadMore}
        onOpenTask={() => {}}
      />
    ));

    expect(
      view.getByRole('button', { name: 'Retained final page' })
    ).toBeTruthy();
    expect(view.getAllByRole('button', { name: 'Retry' })).toHaveLength(1);
    fireEvent.click(view.getByRole('button', { name: 'Retry' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onLoadMore).not.toHaveBeenCalled();
  });
});
