import { LoadingBlock } from '@core/component/LoadingBlock';
import { Entity, type TaskEntityWithProperties } from '@entity';
import {
  getLocalDateKey,
  getTaskDueDate,
  getTaskScheduleProjection,
} from '@entity/utils/task-properties';
import { TaskDependencyRelations } from '@property/task-dependency-relations';
import { Button, EmptyStatePanel } from '@ui';
import { createUniqueId, For, Show } from 'solid-js';

type DeadlineGroup = {
  id: string;
  label: string;
  tasks: TaskEntityWithProperties[];
};

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
}) {
  const idPrefix = createUniqueId();
  const groups = () => groupTasks(props.tasks);
  const loadMoreLabel = () => {
    if (props.fetchingNextPage) return 'Loading more…';
    if (props.error) return 'Retry loading more';
    return 'Load more tasks';
  };

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
        <div class="min-w-0 flex-1 divide-y divide-edge-muted">
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

                          return (
                            <li>
                              <button
                                type="button"
                                class="flex min-h-10 w-full min-w-0 items-center gap-2 px-3 py-1 text-left text-sm text-ink hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 touch:min-h-11"
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
                                  <TaskDependencyRelations
                                    taskId={task.id}
                                    task={task}
                                    mode="row"
                                  />
                                </span>
                              </button>
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
        </div>
        <Show when={props.hasNextPage || props.error}>
          <div class="flex shrink-0 justify-center p-2">
            <Show
              when={props.hasNextPage}
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
