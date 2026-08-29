import { SoupEntityContextMenu } from '@app/features/next-soup/soup-view/soup-entity-context-menu';
import { LoadingBlock } from '@core/component/LoadingBlock';
import { Entity, type TaskEntityWithProperties } from '@entity';
import {
  getTaskStatusOptionId,
  TASK_STATUS_OPTIONS,
} from '@entity/utils/task-properties';
import { TaskDependencyRelations } from '@property/task-dependency-relations';
import type { Property } from '@property/types';
import { EmptyStatePanel } from '@ui';
import { For, Show } from 'solid-js';

type TaskStatusBucket = {
  id: string;
  label: string;
  tasks: TaskEntityWithProperties[];
};

const NO_STATUS_BUCKET_ID = 'no-status';

function bucketTasks(tasks: TaskEntityWithProperties[]): TaskStatusBucket[] {
  const buckets = [
    ...TASK_STATUS_OPTIONS.map((option) => ({
      id: option.value,
      label: option.label,
      tasks: [] as TaskEntityWithProperties[],
    })),
    {
      id: NO_STATUS_BUCKET_ID,
      label: 'No status',
      tasks: [] as TaskEntityWithProperties[],
    },
  ];
  const bucketById = new Map(buckets.map((bucket) => [bucket.id, bucket]));

  for (const task of tasks) {
    const statusId = getTaskStatusOptionId(task);
    const bucket = statusId ? bucketById.get(statusId) : undefined;
    (bucket ?? bucketById.get(NO_STATUS_BUCKET_ID))?.tasks.push(task);
  }

  return buckets;
}

export function ProjectTaskStatusBoard(props: {
  tasks: TaskEntityWithProperties[];
  loading?: boolean;
  error?: boolean;
  searching?: boolean;
  onOpenTask: (task: TaskEntityWithProperties, event: MouseEvent) => void;
  onRetry?: () => void;
  canEdit?: boolean;
  statusProperty?: Property;
  statusPending?: boolean;
  activeStatusTaskId?: string;
  onMoveTaskStatus?: (task: TaskEntityWithProperties, statusId: string) => void;
}) {
  const buckets = () => bucketTasks(props.tasks);
  const canMoveStatus = () =>
    props.canEdit === true &&
    props.statusProperty?.valueType === 'SELECT_STRING' &&
    props.onMoveTaskStatus !== undefined;

  return (
    <Show
      when={props.tasks.length > 0}
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
        aria-label="Project task status board"
        class="project-task-status-board @container/project-task-status-board flex size-full min-w-0 min-h-0 flex-col"
      >
        <div class="flex size-full min-h-0 flex-1 gap-2 overflow-x-auto pb-2 @max-[640px]/project-task-status-board:flex-col @max-[640px]/project-task-status-board:overflow-x-visible @max-[640px]/project-task-status-board:overflow-y-auto">
          <For each={buckets()}>
            {(bucket) => (
              <section
                aria-label={`${bucket.label} tasks`}
                class="flex h-full min-w-60 flex-1 flex-col border border-edge-muted bg-surface @max-[640px]/project-task-status-board:h-auto @max-[640px]/project-task-status-board:min-w-0 @max-[640px]/project-task-status-board:flex-none"
              >
                <header class="flex min-h-10 items-center border-b border-edge-muted px-2">
                  <h2 class="truncate text-sm font-medium text-ink">
                    {bucket.label}
                  </h2>
                </header>
                <div class="flex min-h-0 flex-1 flex-col overflow-y-auto py-1 scrollbar-hidden @max-[640px]/project-task-status-board:flex-none @max-[640px]/project-task-status-board:overflow-visible">
                  <For each={bucket.tasks}>
                    {(task) => {
                      const statusId = () => getTaskStatusOptionId(task);
                      const knownStatusId = () =>
                        TASK_STATUS_OPTIONS.some(
                          (option) => option.value === statusId()
                        )
                          ? statusId()
                          : '';
                      const controlId = encodeURIComponent(task.id);
                      const isActive = () =>
                        props.activeStatusTaskId === task.id;

                      return (
                        <SoupEntityContextMenu entity={task}>
                          <div class="flex min-w-0 items-center gap-1">
                            <button
                              type="button"
                              class="flex min-h-10 min-w-0 flex-1 items-center gap-2 px-2 py-1 text-left text-sm text-ink hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 touch:min-h-11"
                              onClick={(event) => props.onOpenTask(task, event)}
                            >
                              <span class="size-4 shrink-0">
                                <Entity.Icon entity={task} />
                              </span>
                              <span class="min-w-0 truncate">
                                <Entity.Title entity={task} />
                              </span>
                            </button>
                            <TaskDependencyRelations
                              taskId={task.id}
                              task={task}
                              mode="row"
                            />
                            <Show when={canMoveStatus()}>
                              <select
                                aria-label={`${task.name} status`}
                                aria-busy={isActive() || undefined}
                                data-project-task-status-control={controlId}
                                disabled={props.statusPending}
                                value={knownStatusId()}
                                class="min-h-10 shrink-0 border border-edge-muted bg-surface px-1 text-xs text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40 disabled:opacity-50 touch:min-h-11"
                                onChange={(event) => {
                                  const nextStatusId =
                                    event.currentTarget.value;
                                  if (
                                    !nextStatusId ||
                                    nextStatusId === statusId()
                                  ) {
                                    return;
                                  }
                                  props.onMoveTaskStatus?.(task, nextStatusId);
                                  requestAnimationFrame(() => {
                                    document
                                      .querySelector<HTMLSelectElement>(
                                        `[data-project-task-status-control="${controlId}"]`
                                      )
                                      ?.focus();
                                  });
                                }}
                              >
                                <option value="" disabled>
                                  No status
                                </option>
                                <For each={TASK_STATUS_OPTIONS}>
                                  {(option) => (
                                    <option value={option.value}>
                                      {option.label}
                                    </option>
                                  )}
                                </For>
                              </select>
                            </Show>
                          </div>
                        </SoupEntityContextMenu>
                      );
                    }}
                  </For>
                </div>
              </section>
            )}
          </For>
        </div>
      </section>
    </Show>
  );
}
