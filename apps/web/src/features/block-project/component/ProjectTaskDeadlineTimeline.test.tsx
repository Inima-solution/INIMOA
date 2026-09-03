/** @vitest-environment jsdom */

import type { TaskEntityWithProperties } from '@entity';
import { PROPERTY_OPTION_IDS, SYSTEM_PROPERTY_IDS } from '@property/constants';
import type { SoupProperty } from '@service-storage/generated/schemas';
import { cleanup, fireEvent, render, waitFor } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProjectTaskDeadlineTimeline } from './ProjectTaskDeadlineTimeline';

const mocks = vi.hoisted(() => ({
  relationCalls: [] as Array<{
    taskId: string;
    task?: TaskEntityWithProperties;
    mode?: string;
  }>,
  resizeCallbacks: [] as Array<(entry: { width: number }) => void>,
  relationsForTask: undefined as
    | ((taskId: string) =>
        | {
            kind: string;
            relation?: {
              dependsOnTaskIds: string[];
              hasUnavailableDependencies: boolean;
            };
          }
        | undefined)
    | undefined,
  relationLookupCalls: [] as string[],
  progressTaskIds: [] as string[],
}));

vi.mock('@core/component/LoadingBlock', () => ({
  LoadingBlock: () => <div data-testid="loading-block">Loading tasks</div>,
}));
vi.mock('@solid-primitives/resize-observer', () => ({
  createResizeObserver: (
    _element: unknown,
    callback: (entry: { width: number }) => void
  ) => mocks.resizeCallbacks.push(callback),
}));
vi.mock('@entity', () => ({
  Entity: {
    Icon: () => <span aria-hidden="true" />,
    Title: (props: { entity: TaskEntityWithProperties }) => props.entity.name,
  },
}));
vi.mock('@property/task-dependency-relations', () => ({
  useTaskDependencyRelations: () => (taskId: string) => {
    mocks.relationLookupCalls.push(taskId);
    return mocks.relationsForTask?.(taskId);
  },
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
vi.mock('@property/task-subtask-progress', () => ({
  TaskSubtaskProgressIndicator: (props: { taskId: string; mode?: string }) => {
    mocks.progressTaskIds.push(props.taskId);
    return (
      <span data-testid="task-subtask-progress" data-mode={props.mode}>
        Derived progress
      </span>
    );
  },
}));

afterEach(() => {
  cleanup();
  mocks.relationCalls = [];
  mocks.resizeCallbacks = [];
  mocks.relationsForTask = undefined;
  mocks.relationLookupCalls = [];
  mocks.progressTaskIds = [];
  vi.useRealTimers();
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

function selectOption(value: string) {
  return { type: 'SelectOption', value: [value] };
}

function withProperty(
  entity: TaskEntityWithProperties,
  definitionId: string,
  value: unknown
) {
  entity.properties = [
    ...(entity.properties ?? []),
    {
      definition: { id: definitionId },
      value,
    } as unknown as SoupProperty,
  ];
  return entity;
}

function dependencyState(
  dependsOnTaskIds: string[],
  hasUnavailableDependencies = false
) {
  return {
    kind: 'ready',
    relation: { dependsOnTaskIds, hasUnavailableDependencies },
  };
}

function stubRect(
  element: Element,
  left: number,
  top: number,
  right: number,
  bottom: number
) {
  vi.spyOn(element, 'getBoundingClientRect').mockReturnValue({
    left,
    top,
    right,
    bottom,
  } as DOMRect);
}

describe('ProjectTaskDeadlineTimeline', () => {
  it('renders one private, neutral dependency overlay from canonical predecessors and updates it on readiness and resize', async () => {
    const predecessor = task('Visible predecessor', due('2026-04-10T08:00:00'));
    const dependent = task('Visible dependent', due('2026-04-11T08:00:00'));
    const [relationState, setRelationState] = createSignal(
      dependencyState([predecessor.id])
    );
    mocks.relationsForTask = (taskId) =>
      taskId === dependent.id ? relationState() : dependencyState([]);
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[predecessor, dependent]}
        projectStartDate="2026-04-10"
        projectTargetDate="2026-04-11"
        onOpenTask={() => {}}
      />
    ));
    const container = view.container.querySelector(
      '.project-task-deadline-timeline > div.relative'
    );
    const markers = view.container.querySelectorAll(
      '[data-project-task-timeline-deadline]'
    );
    expect(container).toBeTruthy();
    expect(markers).toHaveLength(2);
    stubRect(container!, 0, 0, 400, 200);
    stubRect(markers[0]!, 40, 40, 44, 44);
    stubRect(markers[1]!, 240, 100, 244, 104);
    mocks.resizeCallbacks.at(-1)?.({ width: 400 });

    await waitFor(() =>
      expect(view.container.querySelectorAll('svg path')).toHaveLength(1)
    );
    const svg = view.container.querySelector('svg');
    const widePath = svg?.querySelector('path')?.getAttribute('d');
    expect(svg?.getAttribute('aria-hidden')).toBe('true');
    expect(svg?.getAttribute('focusable')).toBe('false');
    expect(svg?.classList.contains('pointer-events-none')).toBe(true);
    expect(svg?.classList.contains('text-ink-muted')).toBe(true);
    expect(svg?.textContent).toBe('');
    expect(svg?.outerHTML).not.toContain(predecessor.id);
    expect(svg?.outerHTML).not.toContain(dependent.id);
    expect(svg?.outerHTML).not.toContain(predecessor.name);
    expect(svg?.outerHTML).not.toContain(dependent.name);
    expect(mocks.relationCalls).toHaveLength(2);

    stubRect(markers[1]!, 120, 100, 124, 104);
    mocks.resizeCallbacks.at(-1)?.({ width: 160 });
    await waitFor(() =>
      expect(svg?.querySelector('path')?.getAttribute('d')).not.toBe(widePath)
    );

    setRelationState(dependencyState([predecessor.id], true));
    await waitFor(() =>
      expect(view.container.querySelectorAll('svg path')).toHaveLength(0)
    );
    expect(
      view.getByRole('button', { name: 'Visible predecessor' })
    ).toBeTruthy();
    expect(
      view.getByRole('button', { name: 'Visible dependent' })
    ).toBeTruthy();
  });

  it('removes a departed dependency marker before recomputing the remaining task window', async () => {
    const predecessor = task('Retained task', due('2026-04-10T08:00:00'));
    const dependent = task('Removed task', due('2026-04-11T08:00:00'));
    const [tasks, setTasks] = createSignal([predecessor, dependent]);
    mocks.relationsForTask = (taskId) =>
      taskId === dependent.id
        ? dependencyState([predecessor.id])
        : dependencyState([]);
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks()}
        projectStartDate="2026-04-10"
        projectTargetDate="2026-04-11"
        onOpenTask={() => {}}
      />
    ));
    const container = view.container.querySelector(
      '.project-task-deadline-timeline > div.relative'
    );
    const markers = view.container.querySelectorAll(
      '[data-project-task-timeline-deadline]'
    );
    stubRect(container!, 0, 0, 400, 200);
    stubRect(markers[0]!, 40, 40, 44, 44);
    stubRect(markers[1]!, 240, 100, 244, 104);
    mocks.resizeCallbacks.at(-1)?.({ width: 400 });
    await waitFor(() =>
      expect(view.container.querySelectorAll('svg path')).toHaveLength(1)
    );

    mocks.relationLookupCalls = [];
    setTasks([predecessor]);
    await waitFor(() =>
      expect(view.container.querySelectorAll('svg path')).toHaveLength(0)
    );
    expect(view.queryByRole('button', { name: 'Removed task' })).toBeNull();
    expect(mocks.relationLookupCalls).not.toContain(dependent.id);
    expect(mocks.relationLookupCalls).toContain(predecessor.id);
  });

  it('cancels the exact pending dependency animation frame on unmount', () => {
    const pendingFrames = new Map<number, FrameRequestCallback>();
    const requestFrame = vi
      .spyOn(window, 'requestAnimationFrame')
      .mockImplementation((callback) => {
        pendingFrames.set(701, callback);
        return 701;
      });
    const cancelFrame = vi
      .spyOn(window, 'cancelAnimationFrame')
      .mockImplementation((handle) => pendingFrames.delete(handle));
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Frame task', due('2026-04-10T08:00:00'))]}
        projectStartDate="2026-04-10"
        projectTargetDate="2026-04-10"
        onOpenTask={() => {}}
      />
    ));

    expect(requestFrame).toHaveBeenCalledTimes(1);
    view.unmount();
    expect(cancelFrame).toHaveBeenCalledWith(701);
    expect(pendingFrames.has(701)).toBe(false);
    let postUnmountCallbacks = 0;
    for (const callback of pendingFrames.values()) {
      postUnmountCallbacks += 1;
      callback(0);
    }
    expect(postUnmountCallbacks).toBe(0);
    requestFrame.mockRestore();
    cancelFrame.mockRestore();
  });

  it('never reads dependency state beyond the first 500 rendered tasks', async () => {
    const tasks = Array.from({ length: 501 }, (_, index) =>
      task(`Task ${index}`, due('2026-04-10T08:00:00'))
    );
    mocks.relationsForTask = () => dependencyState([]);
    render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        projectStartDate="2026-04-10"
        projectTargetDate="2026-04-10"
        onOpenTask={() => {}}
      />
    ));
    await waitFor(() =>
      expect(mocks.relationLookupCalls.length).toBeGreaterThan(0)
    );
    expect(mocks.relationLookupCalls).not.toContain(tasks[500]!.id);
  });

  it('suppresses a dense complete dependency overlay without changing visible task rows', async () => {
    const tasks = Array.from({ length: 500 }, (_, index) =>
      task(`Dense task ${index}`, due('2026-04-10T08:00:00'))
    );
    const predecessorIds = tasks.slice(0, 498).map((item) => item.id);
    mocks.relationsForTask = (taskId) =>
      taskId === tasks[498]?.id || taskId === tasks[499]?.id
        ? dependencyState(predecessorIds)
        : dependencyState([]);
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        projectStartDate="2026-04-10"
        projectTargetDate="2026-04-10"
        onOpenTask={() => {}}
      />
    ));
    const container = view.container.querySelector(
      '.project-task-deadline-timeline > div.relative'
    );
    stubRect(container!, 0, 0, 400, 200);
    mocks.resizeCallbacks.at(-1)?.({ width: 400 });

    await waitFor(() =>
      expect(mocks.relationLookupCalls.length).toBeGreaterThan(500)
    );
    expect(view.container.querySelectorAll('svg path')).toHaveLength(0);
    expect(view.getByRole('button', { name: 'Dense task 0' })).toBeTruthy();
    expect(view.getByRole('button', { name: 'Dense task 499' })).toBeTruthy();
    expect(view.getAllByTestId('task-dependency-relation')).toHaveLength(500);
  });

  it('renders canonical task status and priority metadata in stable order, omitting unavailable values', () => {
    const both = withProperty(
      withProperty(
        task('Both', due('2026-04-10T08:00:00')),
        SYSTEM_PROPERTY_IDS.STATUS,
        selectOption(PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS)
      ),
      SYSTEM_PROPERTY_IDS.PRIORITY,
      selectOption(PROPERTY_OPTION_IDS.PRIORITY.HIGH)
    );
    const statusOnly = withProperty(
      task('Status only', due('2026-04-10T09:00:00')),
      SYSTEM_PROPERTY_IDS.STATUS,
      selectOption(PROPERTY_OPTION_IDS.STATUS.COMPLETED)
    );
    const priorityOnly = withProperty(
      task('Priority only', due('2026-04-10T10:00:00')),
      SYSTEM_PROPERTY_IDS.PRIORITY,
      selectOption(PROPERTY_OPTION_IDS.PRIORITY.LOW)
    );
    const unavailable = withProperty(
      withProperty(
        task('No canonical metadata', due('2026-04-10T11:00:00')),
        SYSTEM_PROPERTY_IDS.STATUS,
        { type: 'Text', value: PROPERTY_OPTION_IDS.STATUS.CANCELED }
      ),
      SYSTEM_PROPERTY_IDS.PRIORITY,
      selectOption('unknown-priority-id')
    );
    const malformed = withProperty(
      task('Malformed', due('2026-04-10T12:00:00')),
      SYSTEM_PROPERTY_IDS.STATUS,
      { type: 'SelectOption', value: [123] }
    );
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[both, statusOnly, priorityOnly, unavailable, malformed]}
        onOpenTask={() => {}}
      />
    ));

    const bothMetadata = view.getByText('Status: In Progress · Priority: High');
    expect(bothMetadata.getAttribute('aria-label')).toBe(
      'Status: In Progress · Priority: High'
    );
    expect(bothMetadata.getAttribute('title')).toBe(
      'Status: In Progress · Priority: High'
    );
    expect(view.getByText('Status: Completed')).toBeTruthy();
    expect(view.getByText('Priority: Low')).toBeTruthy();
    expect(
      view.getByRole('button', { name: 'No canonical metadata' })
    ).toBeTruthy();
    expect(view.getByRole('button', { name: 'Malformed' })).toBeTruthy();
    expect(view.queryByText('Status: Canceled')).toBeNull();
    expect(view.queryByText('Priority: unknown-priority-id')).toBeNull();
    expect(view.queryByText(PROPERTY_OPTION_IDS.STATUS.CANCELED)).toBeNull();
    expect(view.queryByText('123')).toBeNull();
    expect(
      view.getByRole('button', {
        name: 'Both Status: In Progress · Priority: High',
      })
    ).toBeTruthy();
  });

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

  it('adds only truthful, hidden task geometry to valid project ranges', () => {
    const clippedSpan = task(
      'Clipped span',
      due('2026-04-12T08:00:00'),
      false,
      due('2026-04-09T08:00:00')
    );
    const deadline = task('Deadline point', due('2026-04-11T08:00:00'));
    const outside = task('Outside', due('2026-04-14T08:00:00'));
    const invalid = task(
      'Invalid geometry',
      due('2026-04-11T08:00:00'),
      false,
      due('2026-04-12T08:00:00')
    );
    const milestone = task('Milestone', due('2026-04-13T08:00:00'), true);
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[clippedSpan, deadline, outside, invalid, milestone]}
        projectStartDate="2026-04-10"
        projectTargetDate="2026-04-13"
        onOpenTask={() => {}}
      />
    ));

    const span = view.container.querySelector(
      '[data-project-task-timeline-span]'
    );
    const points = view.container.querySelectorAll(
      '[data-project-task-timeline-deadline]'
    );
    expect(span?.parentElement?.getAttribute('aria-hidden')).toBe('true');
    expect(span?.parentElement?.classList.contains('inset-x-3')).toBe(true);
    expect(span?.getAttribute('style')).toContain('left: 0%');
    expect(span?.getAttribute('style')).toContain('width: 75%');
    expect(span?.classList.contains('bg-ink-muted')).toBe(true);
    expect(span?.classList.contains('bg-accent')).toBe(false);
    expect(points).toHaveLength(2);
    expect(points[0].parentElement?.getAttribute('aria-hidden')).toBe('true');
    expect(points[0].classList.contains('bg-ink-muted')).toBe(true);
    expect(points[0].classList.contains('bg-accent')).toBe(false);
    expect(points[0].getAttribute('style')).toContain('left: 37.5%');
    expect(points[1].getAttribute('style')).toContain('left: 87.5%');
    expect(
      view.getByRole('button', {
        name: 'Clipped span Start Apr 9, 2026; due Apr 12, 2026',
      })
    ).toBeTruthy();
    expect(view.getAllByText('Milestone')).toHaveLength(1);
    expect(
      view
        .getByRole('button', { name: 'Outside' })
        .querySelector(
          '[data-project-task-timeline-span], [data-project-task-timeline-deadline]'
        )
    ).toBeNull();
    expect(
      view
        .getByRole('button', { name: /Invalid geometry/ })
        .querySelector(
          '[data-project-task-timeline-span], [data-project-task-timeline-deadline]'
        )
    ).toBeNull();
  });

  it('does not project geometry when the project range is unavailable', () => {
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Scheduled', due('2026-04-10T08:00:00'))]}
        projectStartDate="invalid"
        projectTargetDate="2026-04-11"
        onOpenTask={() => {}}
      />
    ));

    expect(
      view.container.querySelector(
        '[data-project-task-timeline-span], [data-project-task-timeline-deadline]'
      )
    ).toBeNull();
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

  it('renders derived progress only for milestones and keeps every row relation', () => {
    const first = task('First', due('2026-04-10T08:00:00'), true);
    const second = task('Second', due('2026-04-10T10:00:00'));
    const third = task('Third', due('2026-04-10T12:00:00'));
    const malformed = withProperty(
      task('Malformed milestone', due('2026-04-10T14:00:00')),
      SYSTEM_PROPERTY_IDS.MILESTONE,
      { type: 'Boolean', value: 'true' }
    );
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[first, second, third, malformed]}
        onOpenTask={() => {}}
      />
    ));

    expect(mocks.relationCalls).toEqual([
      { taskId: first.id, task: first, mode: 'row' },
      { taskId: second.id, task: second, mode: 'row' },
      { taskId: third.id, task: third, mode: 'row' },
      { taskId: malformed.id, task: malformed, mode: 'row' },
    ]);
    expect(view.getAllByTestId('task-dependency-relation')).toHaveLength(4);
    expect(view.getAllByTestId('task-subtask-progress')).toHaveLength(1);
    expect(
      view.getByTestId('task-subtask-progress').getAttribute('data-mode')
    ).toBe('row');
    expect(
      view.getByTestId('task-subtask-progress').closest('button')
    ).toBeNull();
    expect(view.getByRole('button', { name: /First/ })).toBeTruthy();
    expect(mocks.progressTaskIds).toEqual([first.id]);
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

  it('uses bounded responsive ruler scales with one accessible description', async () => {
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Visible', due('2026-04-10T08:00:00'))]}
        projectStartDate="2026-01-01"
        projectTargetDate="2026-04-02"
        onOpenTask={() => {}}
      />
    ));
    const ruler = () => view.getByRole('img', { name: /Project range/ });
    expect(ruler().getAttribute('data-project-timeline-scale')).toBe(
      'boundary'
    );
    expect(ruler().textContent).toMatch(/Jan|2026/);
    const resize = (width: number) =>
      mocks.resizeCallbacks.forEach((callback) => callback({ width }));
    resize(10_000);
    await waitFor(() =>
      expect(ruler().getAttribute('data-project-timeline-scale')).toBe('week')
    );
    expect(ruler().textContent).toContain('Jan 1, 2026');
    expect(ruler().textContent).toContain('Jan 7, 2026');
    expect(mocks.resizeCallbacks.length).toBeGreaterThan(0);
    resize(200);
    await waitFor(() =>
      expect(ruler().getAttribute('data-project-timeline-scale')).toBe(
        'quarter'
      )
    );
    expect(ruler().textContent).toContain('Q1 2026');
    const quarterTicks = ruler().querySelectorAll('[aria-hidden="true"]');
    expect(quarterTicks[0].getAttribute('style')).toContain('flex-grow: 90');
    expect(quarterTicks[1].getAttribute('style')).toContain('flex-grow: 2');
    expect(quarterTicks[0].getAttribute('style')).toContain('flex-basis: 0%');
    expect(quarterTicks[1].getAttribute('style')).toContain('flex-basis: 0%');
    resize(0);
    await waitFor(() =>
      expect(ruler().getAttribute('data-project-timeline-scale')).toBe(
        'boundary'
      )
    );
    expect(ruler().textContent).toContain('Jan 1, 2026');
    expect(ruler().textContent).toContain('Apr 2, 2026');
    expect(
      ruler().querySelectorAll('[aria-hidden="true"]').length
    ).toBeLessThanOrEqual(64);
    expect(view.queryByRole('region', { name: /range/i })).toBeNull();
  });

  it('renders the day scale after a measured width change', async () => {
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Day', due('2026-01-02T08:00:00'))]}
        projectStartDate="2026-01-01"
        projectTargetDate="2026-01-03"
        onOpenTask={() => {}}
      />
    ));
    mocks.resizeCallbacks.forEach((callback) => callback({ width: 10_000 }));
    await waitFor(() =>
      expect(
        view.getByRole('img').getAttribute('data-project-timeline-scale')
      ).toBe('day')
    );
    expect(view.getByRole('img').textContent).toMatch(/Jan 1, 2026/);
  });

  it('renders the month scale after a measured width change', async () => {
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Month', due('2026-04-10T08:00:00'))]}
        projectStartDate="2026-01-01"
        projectTargetDate="2026-04-30"
        onOpenTask={() => {}}
      />
    ));
    mocks.resizeCallbacks.forEach((callback) => callback({ width: 360 }));
    await waitFor(() =>
      expect(
        view.getByRole('img').getAttribute('data-project-timeline-scale')
      ).toBe('month')
    );
    expect(view.getByRole('img').textContent).toMatch(/Jan 2026/);
  });

  it('uses fixed non-sensitive states for unavailable and incomplete ranges', () => {
    const cases = [
      {
        projectStartDate: undefined,
        projectTargetDate: undefined,
        text: 'Project dates are not set.',
      },
      {
        projectStartDate: 'invalid',
        projectTargetDate: '2026-01-02',
        text: 'Project date range is unavailable.',
      },
      {
        projectStartDate: '2026-01-03',
        projectTargetDate: '2026-01-02',
        text: 'Project date range is unavailable.',
      },
      {
        projectStartDate: '2026-01-02',
        projectTargetDate: undefined,
        text: 'Project date range is unavailable.',
      },
      {
        projectStartDate: 'raw-secret',
        projectTargetDate: '2026-01-02',
        rangeUnavailable: true,
        text: 'Project date range is unavailable.',
      },
    ];
    for (const entry of cases) {
      const view = render(() => (
        <ProjectTaskDeadlineTimeline
          tasks={[task('State')]}
          onOpenTask={() => {}}
          {...entry}
        />
      ));
      expect(view.getByText(entry.text)).toBeTruthy();
      expect(view.queryByText('raw-secret')).toBeNull();
      view.unmount();
    }
  });

  it('visibly states Today relations outside the range and keeps inside marker-only', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 0, 1, 12));
    const before = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Before')]}
        projectStartDate="2026-01-02"
        projectTargetDate="2026-01-03"
        onOpenTask={() => {}}
      />
    ));
    expect(before.getByText('Today is before this range.')).toBeTruthy();
    before.unmount();
    vi.setSystemTime(new Date(2026, 0, 2, 12));
    const inside = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Inside')]}
        projectStartDate="2026-01-02"
        projectTargetDate="2026-01-03"
        onOpenTask={() => {}}
      />
    ));
    expect(inside.queryByText(/Today is (before|after) this range/)).toBeNull();
    expect(inside.getByRole('img').querySelector('.bg-accent')).toBeTruthy();
    inside.unmount();
    vi.setSystemTime(new Date(2026, 0, 4, 12));
    const after = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('After')]}
        projectStartDate="2026-01-02"
        projectTargetDate="2026-01-03"
        onOpenTask={() => {}}
      />
    ));
    expect(after.getByText('Today is after this range.')).toBeTruthy();
  });

  it('refreshes today at local midnight and clears its timeout when unmounted', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 0, 1, 23, 59, 59));
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={[task('Midnight', due('2026-01-02T08:00:00'))]}
        projectStartDate="2026-01-01"
        projectTargetDate="2026-01-02"
        onOpenTask={() => {}}
      />
    ));
    const ruler = view.getByRole('img', { name: /Today is/ });
    expect(ruler.getAttribute('aria-label')).toMatch(/Jan 1, 2026/);
    vi.advanceTimersByTime(1_200);
    expect(ruler.getAttribute('aria-label')).toMatch(/Jan 2, 2026/);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('keeps 500 visible tasks and bounded ruler ticks without loading more', () => {
    const tasks = Array.from({ length: 500 }, (_, index) =>
      task(`Task ${index}`, due('2026-04-10T08:00:00'))
    );
    const loadMore = vi.fn();
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        hasNextPage={false}
        onLoadMore={loadMore}
        projectStartDate="2020-01-01"
        projectTargetDate="2040-12-31"
        onOpenTask={() => {}}
      />
    ));
    expect(view.getAllByRole('button')).toHaveLength(500);
    expect(mocks.relationCalls).toHaveLength(500);
    expect(view.getAllByRole('button')[0].textContent).toContain('Task 0');
    expect(loadMore).not.toHaveBeenCalled();
    expect(
      view.getByRole('img').querySelectorAll('[aria-hidden="true"]')
    ).toHaveLength(2);
  });

  it('limits an over-cap source to the first 500 tasks and relations', () => {
    const tasks = Array.from({ length: 501 }, (_, index) =>
      task(`Task ${index}`, due('2026-04-10T08:00:00'))
    );
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        projectStartDate="2026-04-10"
        projectTargetDate="2026-04-10"
        onOpenTask={() => {}}
      />
    ));

    const buttons = view.getAllByRole('button');
    expect(buttons).toHaveLength(500);
    expect(buttons[0].textContent).toContain('Task 0');
    expect(buttons.at(-1)?.textContent).toContain('Task 499');
    expect(view.queryByRole('button', { name: 'Task 500' })).toBeNull();
    expect(mocks.relationCalls).toHaveLength(500);
    expect(
      view.container.querySelectorAll('[data-project-task-timeline-deadline]')
    ).toHaveLength(500);
    expect(
      view.getByRole('status', { name: /Showing the first 500 tasks/ })
    ).toBeTruthy();
  });

  it('treats exactly 500 retained tasks as complete without a limit notice', () => {
    const tasks = Array.from({ length: 500 }, (_, index) =>
      task(`Task ${index}`, due('2026-04-10T08:00:00'))
    );
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        hasNextPage={false}
        onOpenTask={() => {}}
      />
    ));

    expect(view.getAllByRole('button')).toHaveLength(500);
    expect(view.queryByText(/Showing the first 500 tasks/)).toBeNull();
  });

  it('shows the accessible limit notice and stops continuation at 500 tasks', () => {
    const tasks = Array.from({ length: 500 }, (_, index) =>
      task(`Task ${index}`, due('2026-04-10T08:00:00'))
    );
    const loadMore = vi.fn();
    const onOpenTask = vi.fn();
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        hasNextPage
        onLoadMore={loadMore}
        onOpenTask={onOpenTask}
      />
    ));

    expect(
      view.getByRole('status', {
        name: 'Showing the first 500 tasks. Refine filters to view a smaller set.',
      })
    ).toBeTruthy();
    expect(
      view.queryByRole('button', { name: /Load more|Retry loading more/ })
    ).toBeNull();
    const finalTask = view.getByRole('button', { name: 'Task 499' });
    finalTask.focus();
    fireEvent.click(finalTask);
    expect(onOpenTask).toHaveBeenCalledWith(tasks[499], expect.any(MouseEvent));
    expect(loadMore).not.toHaveBeenCalled();
  });

  it('keeps source error recovery available at the task window limit', () => {
    const tasks = Array.from({ length: 500 }, (_, index) =>
      task(`Task ${index}`, due('2026-04-10T08:00:00'))
    );
    const onRetry = vi.fn();
    const onLoadMore = vi.fn();
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        error
        hasNextPage
        onRetry={onRetry}
        onLoadMore={onLoadMore}
        onOpenTask={() => {}}
      />
    ));

    expect(
      view.getByRole('status', {
        name: 'Showing the first 500 tasks. Refine filters to view a smaller set.',
      })
    ).toBeTruthy();
    expect(view.getAllByRole('button', { name: /Task \d+/ })).toHaveLength(500);
    fireEvent.click(view.getByRole('button', { name: 'Retry' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
    expect(onLoadMore).not.toHaveBeenCalled();
    expect(
      view.queryByRole('button', { name: 'Retry loading more' })
    ).toBeNull();
  });

  it('keeps manual continuation below the 500-task window', () => {
    const tasks = Array.from({ length: 499 }, (_, index) =>
      task(`Task ${index}`, due('2026-04-10T08:00:00'))
    );
    const loadMore = vi.fn();
    const view = render(() => (
      <ProjectTaskDeadlineTimeline
        tasks={tasks}
        hasNextPage
        onLoadMore={loadMore}
        onOpenTask={() => {}}
      />
    ));

    fireEvent.click(view.getByRole('button', { name: 'Load more tasks' }));
    expect(loadMore).toHaveBeenCalledTimes(1);
    expect(view.queryByText(/Showing the first 500 tasks/)).toBeNull();
  });
});
