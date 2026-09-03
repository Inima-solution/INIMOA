import { LoadingBlock } from '@core/component/LoadingBlock';
import { Entity, type TaskEntityWithProperties } from '@entity';
import {
  getLocalDateKey,
  getPropertyOptionLabel,
  getTaskDueDate,
  getTaskPriorityOptionId,
  getTaskScheduleProjection,
  getTaskStatusOptionId,
  isTaskMilestone,
} from '@entity/utils/task-properties';
import {
  TaskDependencyRelations,
  useTaskDependencyRelations,
} from '@property/task-dependency-relations';
import { TaskSubtaskProgressIndicator } from '@property/task-subtask-progress';
import { createResizeObserver } from '@solid-primitives/resize-observer';
import { Button, EmptyStatePanel } from '@ui';
import {
  createEffect,
  createMemo,
  createSignal,
  createUniqueId,
  For,
  onCleanup,
  Show,
} from 'solid-js';
import {
  getProjectTimelineDependencyEdges,
  getProjectTimelineDependencyPaths,
} from './project-task-timeline-dependencies';
import {
  formatProjectTimelineDate,
  formatProjectTimelineTick,
  getInclusiveLocalCalendarDays,
  getProjectTimelineClippedSpanPercent,
  getProjectTimelineDayCenterPercent,
  getProjectTimelineRange,
  getProjectTimelineRuler,
  getProjectTimelineToday,
  getProjectTimelineTodayPercent,
} from './project-task-timeline-range';

type DeadlineGroup = {
  id: string;
  label: string;
  tasks: TaskEntityWithProperties[];
};

type TaskTimelineGeometry =
  | { kind: 'span'; leftPercent: number; widthPercent: number }
  | { kind: 'deadline'; leftPercent: number };

const PROJECT_TASK_TIMELINE_TASK_WINDOW_LIMIT = 500;

const dateFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
});

function groupTasks(tasks: TaskEntityWithProperties[]): DeadlineGroup[] {
  const scheduled = new Map<string, DeadlineGroup>();
  const unscheduled: TaskEntityWithProperties[] = [];

  for (const task of tasks) {
    const dueDate = getTaskDueDate(task);
    if (!dueDate) {
      unscheduled.push(task);
      continue;
    }
    const id = getLocalDateKey(dueDate);
    const group = scheduled.get(id) ?? {
      id,
      label: dateFormatter.format(dueDate),
      tasks: [],
    };
    group.tasks.push(task);
    scheduled.set(id, group);
  }

  return [
    ...[...scheduled.values()].sort((left, right) =>
      left.id.localeCompare(right.id)
    ),
    ...(unscheduled.length > 0
      ? [{ id: 'unscheduled', label: 'Unscheduled', tasks: unscheduled }]
      : []),
  ];
}

export function ProjectTaskDeadlineTimeline(props: {
  tasks: TaskEntityWithProperties[];
  loading?: boolean;
  error?: boolean;
  searching?: boolean;
  onOpenTask: (task: TaskEntityWithProperties, event: MouseEvent) => void;
  onRetry?: () => void;
  hasNextPage?: boolean;
  fetching?: boolean;
  fetchingNextPage?: boolean;
  onLoadMore?: () => void;
  projectStartDate?: string | null;
  projectTargetDate?: string | null;
  rangeUnavailable?: boolean;
}) {
  const idPrefix = createUniqueId();
  const [ruler, setRuler] = createSignal<HTMLDivElement>();
  const [rulerWidth, setRulerWidth] = createSignal<number>();
  const [timeline, setTimeline] = createSignal<HTMLDivElement>();
  const [today, setToday] = createSignal(new Date());
  const [markerVersion, setMarkerVersion] = createSignal(0);
  const [dependencyPaths, setDependencyPaths] = createSignal<string[]>([]);
  const relationsForTask = useTaskDependencyRelations();
  const dependencyMarkers = new Map<string, HTMLElement>();
  let refreshTimer: number | undefined;
  let dependencyFrame: number | undefined;
  createResizeObserver(ruler, ({ width }) => setRulerWidth(width));
  const range = createMemo(() =>
    props.rangeUnavailable
      ? { kind: 'unavailable' as const }
      : getProjectTimelineRange(props.projectStartDate, props.projectTargetDate)
  );
  const scheduleTodayRefresh = () => {
    const now = new Date();
    const nextMidnight = new Date(now);
    nextMidnight.setDate(nextMidnight.getDate() + 1);
    nextMidnight.setHours(0, 0, 0, 0);
    refreshTimer = window.setTimeout(
      () => {
        setToday(new Date());
        scheduleTodayRefresh();
      },
      nextMidnight.getTime() - now.getTime() + 100
    );
  };
  scheduleTodayRefresh();
  onCleanup(() => {
    if (refreshTimer !== undefined) clearTimeout(refreshTimer);
    if (dependencyFrame !== undefined)
      window.cancelAnimationFrame(dependencyFrame);
    dependencyMarkers.clear();
  });
  const visibleTasks = () =>
    props.tasks.slice(0, PROJECT_TASK_TIMELINE_TASK_WINDOW_LIMIT);
  const taskWindowLimited = () =>
    props.tasks.length > PROJECT_TASK_TIMELINE_TASK_WINDOW_LIMIT ||
    (props.tasks.length === PROJECT_TASK_TIMELINE_TASK_WINDOW_LIMIT &&
      props.hasNextPage);
  const groups = () => groupTasks(visibleTasks());
  const scheduleDependencyPaths = () => {
    if (dependencyFrame !== undefined) return;
    dependencyFrame = window.requestAnimationFrame(() => {
      dependencyFrame = undefined;
      const container = timeline();
      const visibleTaskIds = new Set(
        [...dependencyMarkers]
          .filter(
            ([, marker]) => marker.isConnected && container?.contains(marker)
          )
          .map(([taskId]) => taskId)
      );
      const edges = getProjectTimelineDependencyEdges(
        groups().flatMap((group) => group.tasks.map((task) => task.id)),
        relationsForTask,
        visibleTaskIds,
        PROJECT_TASK_TIMELINE_TASK_WINDOW_LIMIT
      );
      setDependencyPaths(
        getProjectTimelineDependencyPaths(edges, dependencyMarkers, container)
      );
    });
  };
  createResizeObserver(timeline, scheduleDependencyPaths);
  createEffect(() => {
    markerVersion();
    const container = timeline();
    const visibleTaskIds = new Set(
      [...dependencyMarkers]
        .filter(
          ([, marker]) => marker.isConnected && container?.contains(marker)
        )
        .map(([taskId]) => taskId)
    );
    getProjectTimelineDependencyEdges(
      groups().flatMap((group) => group.tasks.map((task) => task.id)),
      relationsForTask,
      visibleTaskIds,
      PROJECT_TASK_TIMELINE_TASK_WINDOW_LIMIT
    );
    scheduleDependencyPaths();
  });
  const loadMoreLabel = () => {
    if (props.fetchingNextPage) return 'Loading more…';
    if (props.error) return 'Retry loading more';
    return 'Load more tasks';
  };

  const rangeRuler = createMemo(() => {
    const value = range();
    if (value.kind === 'not-set') {
      return (
        <p class="px-3 py-2 text-xs text-ink-muted">
          Project dates are not set.
        </p>
      );
    }
    if (value.kind === 'invalid' || value.kind === 'unavailable') {
      return (
        <p class="px-3 py-2 text-xs text-ink-muted">
          Project date range is unavailable.
        </p>
      );
    }
    const rulerModel = getProjectTimelineRuler(value, rulerWidth());
    const todayRelation = getProjectTimelineToday(value, today());
    const bounds = `${formatProjectTimelineDate(value.start)} to ${formatProjectTimelineDate(value.end)}`;
    const todayText =
      todayRelation.relation === 'inside'
        ? `Today is ${formatProjectTimelineDate(todayRelation.date)} within this range.`
        : `Today is ${formatProjectTimelineDate(todayRelation.date)}, ${todayRelation.relation} this range.`;
    return (
      <div class="border-b border-edge-muted px-3 py-2">
        <div
          ref={setRuler}
          role="img"
          aria-label={`Project range ${bounds}. ${rulerModel.scale === 'boundary' ? 'Range boundaries only.' : `${rulerModel.scale} scale.`} ${todayText}`}
          class="relative flex min-h-6 items-end justify-between"
          data-project-timeline-scale={rulerModel.scale}
        >
          <Show
            when={rulerModel.scale !== 'boundary'}
            fallback={
              <span
                aria-hidden="true"
                class="flex w-full justify-between text-xs text-ink-muted"
              >
                <span>{formatProjectTimelineDate(value.start)}</span>
                <span>{formatProjectTimelineDate(value.end)}</span>
              </span>
            }
          >
            <For each={rulerModel.ticks}>
              {(tick) => (
                <span
                  aria-hidden="true"
                  class="min-w-0 border-l border-edge-muted px-1 text-xs text-ink-muted"
                  style={{
                    'flex-basis': '0%',
                    'flex-grow': getInclusiveLocalCalendarDays(
                      tick.start,
                      tick.end
                    ),
                  }}
                >
                  <span class="block truncate">
                    {formatProjectTimelineTick(tick, rulerModel.scale)}
                  </span>
                </span>
              )}
            </For>
          </Show>
          <Show when={todayRelation.relation === 'inside'}>
            <span
              aria-hidden="true"
              class="absolute bottom-0 top-0 w-px bg-accent"
              style={{
                left: `${getProjectTimelineTodayPercent(
                  value,
                  todayRelation.date
                )}%`,
              }}
            />
          </Show>
        </div>
        <Show when={todayRelation.relation !== 'inside'}>
          <p class="pt-1 text-xs text-ink-muted">
            Today is {todayRelation.relation} this range.
          </p>
        </Show>
      </div>
    );
  });

  return (
    <Show
      when={props.tasks.length > 0 || props.hasNextPage}
      fallback={
        <Show when={!props.loading} fallback={<LoadingBlock />}>
          <Show
            when={!props.error}
            fallback={
              <EmptyStatePanel
                centered
                title="Couldn’t load tasks"
                description="Try again to load the latest tasks."
                primaryAction={{
                  label: 'Retry',
                  onClick: () => props.onRetry?.(),
                }}
              />
            }
          >
            <EmptyStatePanel
              centered
              title={
                props.searching
                  ? 'No tasks match this search'
                  : 'No tasks in this project'
              }
            />
          </Show>
        </Show>
      }
    >
      <section
        aria-label="Project task deadline timeline"
        class="project-task-deadline-timeline flex size-full min-w-0 min-h-0 flex-col overflow-y-auto"
      >
        {rangeRuler()}
        <Show when={taskWindowLimited()}>
          <p
            role="status"
            aria-label="Showing the first 500 tasks. Refine filters to view a smaller set."
            class="border-b border-edge-muted px-3 py-2 text-xs text-ink-muted"
          >
            Showing the first 500 tasks. Refine filters to view a smaller set.
          </p>
        </Show>
        <div
          ref={setTimeline}
          class="relative min-w-0 flex-1 divide-y divide-edge-muted"
        >
          <For each={groups()}>
            {(group) => {
              const headingId = `${idPrefix}-project-task-deadline-group-${group.id}`;

              return (
                <div class="min-w-0" aria-labelledby={headingId}>
                  <header class="flex min-h-10 items-center border-b border-edge-muted px-3">
                    <h2
                      id={headingId}
                      class="truncate text-sm font-medium text-ink"
                    >
                      {group.label}
                    </h2>
                  </header>
                  <ul class="py-1" aria-labelledby={headingId}>
                    <For each={group.tasks}>
                      {(task) =>
                        (() => {
                          let dependencyMarker: HTMLElement | undefined;
                          const setDependencyMarker = (marker: HTMLElement) => {
                            dependencyMarker = marker;
                            dependencyMarkers.set(task.id, marker);
                            setMarkerVersion((version) => version + 1);
                          };
                          onCleanup(() => {
                            if (
                              dependencyMarker &&
                              dependencyMarkers.get(task.id) ===
                                dependencyMarker
                            ) {
                              dependencyMarkers.delete(task.id);
                              setMarkerVersion((version) => version + 1);
                              scheduleDependencyPaths();
                            }
                          });
                          const schedule = getTaskScheduleProjection(task);
                          const scheduleText = () => {
                            if (schedule.kind === 'span') {
                              return `${dateFormatter.format(schedule.startDate)} – ${dateFormatter.format(schedule.dueDate)}`;
                            }
                            if (schedule.kind === 'invalid-range') {
                              return 'Start date is after due date';
                            }
                            return undefined;
                          };
                          const scheduleAccessibleLabel = () => {
                            if (schedule.kind === 'span') {
                              return `Start ${dateFormatter.format(schedule.startDate)}; due ${dateFormatter.format(schedule.dueDate)}`;
                            }
                            return scheduleText();
                          };
                          const taskGeometry = ():
                            | TaskTimelineGeometry
                            | undefined => {
                            const timelineRange = range();
                            if (timelineRange.kind !== 'valid')
                              return undefined;
                            if (schedule.kind === 'span') {
                              const span = getProjectTimelineClippedSpanPercent(
                                timelineRange,
                                schedule.startDate,
                                schedule.dueDate
                              );
                              return span
                                ? { kind: 'span' as const, ...span }
                                : undefined;
                            }
                            if (schedule.kind === 'deadline') {
                              const leftPercent =
                                getProjectTimelineDayCenterPercent(
                                  timelineRange,
                                  schedule.dueDate
                                );
                              return leftPercent !== undefined
                                ? {
                                    kind: 'deadline' as const,
                                    leftPercent,
                                  }
                                : undefined;
                            }
                            return undefined;
                          };
                          const taskSpanGeometry = ():
                            | Extract<TaskTimelineGeometry, { kind: 'span' }>
                            | undefined => {
                            const geometry = taskGeometry();
                            return geometry?.kind === 'span'
                              ? geometry
                              : undefined;
                          };
                          const taskDeadlineGeometry = ():
                            | Extract<
                                TaskTimelineGeometry,
                                { kind: 'deadline' }
                              >
                            | undefined => {
                            const geometry = taskGeometry();
                            return geometry?.kind === 'deadline'
                              ? geometry
                              : undefined;
                          };
                          const taskMetadataText = () => {
                            const statusLabel = getPropertyOptionLabel(
                              getTaskStatusOptionId(task) ?? ''
                            );
                            const priorityLabel = getPropertyOptionLabel(
                              getTaskPriorityOptionId(task) ?? ''
                            );
                            return [
                              statusLabel && `Status: ${statusLabel}`,
                              priorityLabel && `Priority: ${priorityLabel}`,
                            ]
                              .filter(Boolean)
                              .join(' · ');
                          };

                          return (
                            <li class="flex min-w-0 items-center gap-2">
                              <button
                                type="button"
                                class="relative flex min-h-10 min-w-0 flex-1 items-center gap-2 px-3 py-1 text-left text-sm text-ink hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 touch:min-h-11"
                                onClick={(event) =>
                                  props.onOpenTask(task, event)
                                }
                              >
                                <span class="size-4 shrink-0">
                                  <Entity.Icon entity={task} />
                                </span>
                                <span class="min-w-0 flex-1 truncate">
                                  <Entity.Title entity={task} />
                                </span>
                                <span class="flex max-w-1/2 min-w-0 shrink-0 items-center gap-2 overflow-hidden">
                                  <Show when={scheduleText()}>
                                    {(text) => (
                                      <span
                                        aria-label={scheduleAccessibleLabel()}
                                        title={scheduleAccessibleLabel()}
                                        class="max-w-40 min-w-0 truncate text-xs text-ink-muted"
                                      >
                                        {text()}
                                      </span>
                                    )}
                                  </Show>
                                  <Show when={taskMetadataText()}>
                                    {(text) => (
                                      <span
                                        aria-label={text()}
                                        title={text()}
                                        class="max-w-40 min-w-0 truncate text-xs text-ink-muted"
                                      >
                                        {text()}
                                      </span>
                                    )}
                                  </Show>
                                  <TaskDependencyRelations
                                    taskId={task.id}
                                    task={task}
                                    mode="row"
                                  />
                                </span>
                                <Show when={taskSpanGeometry()}>
                                  {(geometry) => (
                                    <span
                                      aria-hidden="true"
                                      class="pointer-events-none absolute inset-x-3 bottom-0 h-px"
                                    >
                                      <span
                                        aria-hidden="true"
                                        ref={setDependencyMarker}
                                        data-project-task-timeline-span
                                        class="absolute bottom-0 h-px bg-ink-muted"
                                        style={{
                                          left: `${geometry().leftPercent}%`,
                                          width: `${geometry().widthPercent}%`,
                                        }}
                                      />
                                    </span>
                                  )}
                                </Show>
                                <Show when={taskDeadlineGeometry()}>
                                  {(geometry) => (
                                    <span
                                      aria-hidden="true"
                                      class="pointer-events-none absolute inset-x-3 bottom-0 h-px"
                                    >
                                      <span
                                        aria-hidden="true"
                                        ref={setDependencyMarker}
                                        data-project-task-timeline-deadline
                                        class="absolute bottom-0 size-1 -translate-x-1/2 rounded-full bg-ink-muted"
                                        style={{
                                          left: `${geometry().leftPercent}%`,
                                        }}
                                      />
                                    </span>
                                  )}
                                </Show>
                              </button>
                              <Show when={isTaskMilestone(task)}>
                                <TaskSubtaskProgressIndicator
                                  taskId={task.id}
                                  mode="row"
                                />
                              </Show>
                            </li>
                          );
                        })()
                      }
                    </For>
                  </ul>
                </div>
              );
            }}
          </For>
          <svg
            aria-hidden="true"
            ref={(element) => element.setAttribute('focusable', 'false')}
            class="pointer-events-none absolute inset-0 size-full text-ink-muted"
            fill="none"
            stroke="currentColor"
          >
            <For each={dependencyPaths()}>
              {(path) => <path d={path} stroke-width="1" />}
            </For>
          </svg>
        </div>
        <Show when={props.error || (!taskWindowLimited() && props.hasNextPage)}>
          <div class="flex shrink-0 justify-center p-2">
            <Show
              when={!taskWindowLimited() && props.hasNextPage}
              fallback={
                <Button size="sm" onClick={() => props.onRetry?.()}>
                  Retry
                </Button>
              }
            >
              <Button
                size="sm"
                disabled={props.fetching || props.fetchingNextPage}
                aria-busy={props.fetchingNextPage || undefined}
                onClick={() => props.onLoadMore?.()}
              >
                {loadMoreLabel()}
              </Button>
            </Show>
          </div>
        </Show>
      </section>
    </Show>
  );
}
