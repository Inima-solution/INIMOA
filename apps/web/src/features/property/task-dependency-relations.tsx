import { ItemPreview } from '@core/component/ItemPreview';
import { thrownResultErrorHasCode } from '@core/util/result';
import type { TaskEntityWithProperties } from '@entity';
import { getTaskMilestoneState } from '@entity/utils/task-properties';
import { isAccessiblePreviewItem } from '@queries/preview';
import { previewDataLoader } from '@queries/preview/dataloader';
import { previewKeys } from '@queries/preview/keys';
import { propertiesKeys } from '@queries/properties/keys';
import {
  fetchTaskDependencyRelations,
  type TaskDependencyRelations as TaskDependencyRelationsResult,
} from '@queries/properties/task-dependency-relations';
import { useQueries } from '@tanstack/solid-query';
import {
  type Accessor,
  createContext,
  createEffect,
  createMemo,
  createSignal,
  For,
  type ParentProps,
  Show,
  useContext,
} from 'solid-js';

const TASK_IDS_PER_REQUEST = 200;

type RelationState =
  | { kind: 'loading' }
  | { kind: 'ready'; relation: TaskDependencyRelationsResult }
  | { kind: 'unavailable' }
  | { kind: 'offline' }
  | { kind: 'error' };

type ContextValue = {
  relationsForTask: (taskId: string) => RelationState | undefined;
};

const TaskDependencyRelationsContext = createContext<ContextValue>();

function uniqueTaskIds(taskIds: readonly string[]) {
  const seen = new Set<string>();
  return taskIds.filter((id) => {
    if (seen.has(id)) return false;
    seen.add(id);
    return true;
  });
}

function chunks(ids: readonly string[]) {
  const result: string[][] = [];
  for (let index = 0; index < ids.length; index += TASK_IDS_PER_REQUEST) {
    result.push(ids.slice(index, index + TASK_IDS_PER_REQUEST));
  }
  return result;
}

function unavailableError(error: unknown) {
  return (
    thrownResultErrorHasCode(error, 'UNAUTHORIZED') ||
    thrownResultErrorHasCode(error, 'FORBIDDEN') ||
    thrownResultErrorHasCode(error, 'NOT_FOUND')
  );
}

export function TaskDependencyRelationsProvider(
  props: ParentProps<{ taskIds: Accessor<readonly string[]> }>
) {
  const taskIds = createMemo(() => uniqueTaskIds(props.taskIds()));
  const taskIdChunks = createMemo(() => chunks(taskIds()));
  const queries = useQueries(() => ({
    queries: taskIdChunks().map((ids) => ({
      queryKey: propertiesKeys.taskDependencyRelations(ids).queryKey,
      queryFn: () => fetchTaskDependencyRelations(ids),
      enabled: ids.length > 0,
    })),
  }));

  const relationsForTask = (taskId: string): RelationState | undefined => {
    const ids = taskIds();
    const index = ids.indexOf(taskId);
    if (index === -1) return undefined;

    const query = queries[Math.floor(index / TASK_IDS_PER_REQUEST)];
    if (!query || query.fetchStatus === 'paused') return { kind: 'offline' };
    if (query.isPending) return { kind: 'loading' };
    if (query.isError) {
      return unavailableError(query.error)
        ? { kind: 'unavailable' }
        : { kind: 'error' };
    }

    const relation = query.data?.find((item) => item.taskId === taskId);
    return relation ? { kind: 'ready', relation } : { kind: 'unavailable' };
  };

  return (
    <TaskDependencyRelationsContext.Provider value={{ relationsForTask }}>
      {props.children}
    </TaskDependencyRelationsContext.Provider>
  );
}

type DirectionState = 'loading' | 'ready' | 'unavailable' | 'offline' | 'error';

function RelationDirection(props: {
  label: string;
  taskIds: readonly string[];
  blockingTaskIds?: readonly string[];
  unavailable: boolean;
  onState?: (state: DirectionState) => void;
}) {
  // These use the same preview query keys and batched loader as useItemPreview.
  // ItemPreview below then reads the warmed cache and supplies the canonical link.
  const previews = useQueries(() => ({
    queries: (props.unavailable ? [] : props.taskIds).map((id) => ({
      queryKey: previewKeys.item(id).queryKey,
      queryFn: () => previewDataLoader.load({ id, type: 'document' }),
    })),
  }));
  const state = createMemo<DirectionState>(() => {
    if (props.unavailable) return 'unavailable';
    if (previews.some((query) => query.fetchStatus === 'paused'))
      return 'offline';
    if (previews.some((query) => query.isPending)) return 'loading';
    if (previews.some((query) => query.isError)) return 'unavailable';
    if (
      previews.some(
        (query) => !query.data || !isAccessiblePreviewItem(query.data)
      )
    ) {
      return 'unavailable';
    }
    return 'ready';
  });
  createEffect(() => props.onState?.(state()));

  return (
    <Show when={props.taskIds.length > 0 || props.unavailable}>
      <div
        role="group"
        aria-label={props.label}
        class="border-t border-edge-muted pt-2 text-sm text-ink"
      >
        <div class="text-xs text-ink-muted">{props.label}</div>
        <Show
          when={state() === 'ready'}
          fallback={
            <span role="status" aria-live="polite" class="text-ink-muted">
              {state() === 'loading'
                ? `Loading ${props.label.toLowerCase()}`
                : state() === 'offline'
                  ? `${props.label} offline`
                  : `${props.label} unavailable`}
            </span>
          }
        >
          <div class="flex flex-col gap-1">
            <For each={props.taskIds}>
              {(taskId) => {
                const isBlocking = () =>
                  props.blockingTaskIds?.includes(taskId) ?? false;
                return (
                  <div class="flex min-w-0 items-center gap-1">
                    <ItemPreview
                      id={taskId}
                      type="document"
                      class="w-fit max-w-full text-ink"
                      textClass="text-sm text-ink"
                    />
                    <Show when={isBlocking()}>
                      <span
                        aria-label="Blocking predecessor"
                        class="shrink-0 text-xs text-ink-muted"
                      >
                        Blocking
                      </span>
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </div>
    </Show>
  );
}

function relationStatusLabel(state: Exclude<RelationState, { kind: 'ready' }>) {
  return {
    loading: 'Loading task relations',
    unavailable: 'Task relations unavailable',
    offline: 'Task relations offline',
    error: "Couldn't load task relations",
  }[state.kind];
}

function rowStatusLabel(state: RelationState) {
  if (state.kind === 'ready') {
    return state.relation.readiness === 'blocked' ? 'Blocked' : 'Ready';
  }
  return {
    loading: 'Loading…',
    unavailable: 'Unavailable',
    offline: 'Offline',
    error: 'Load failed',
  }[state.kind];
}

function milestoneStateLabel(
  state: NonNullable<ReturnType<typeof getTaskMilestoneState>>
) {
  return {
    milestone: 'Milestone',
    complete: 'Complete',
    overdue: 'Overdue',
    'at-risk': 'At risk',
  }[state];
}

export function TaskDependencyRelations(props: {
  taskId: string;
  task?: TaskEntityWithProperties;
  mode?: 'detail' | 'row';
}) {
  const context = useContext(TaskDependencyRelationsContext);
  const state = () => context?.relationsForTask(props.taskId);
  const isRow = () => props.mode === 'row';
  const milestoneState = () => {
    const currentState = state();
    if (!props.task || !currentState) return undefined;

    return getTaskMilestoneState(props.task, new Date(), {
      isAuthoritative: currentState.kind === 'ready',
      readiness:
        currentState.kind === 'ready'
          ? currentState.relation.readiness
          : undefined,
    });
  };

  return (
    <Show when={state()}>
      {(current) => (
        <Show
          when={isRow()}
          fallback={
            <Show
              when={(() => {
                const currentState = current();
                return currentState.kind === 'ready' ? currentState : undefined;
              })()}
              fallback={
                <span
                  role="status"
                  aria-live="polite"
                  class="text-sm text-ink-muted"
                >
                  {relationStatusLabel(
                    current() as Exclude<RelationState, { kind: 'ready' }>
                  )}
                </span>
              }
            >
              {(ready) => {
                const relation = () => ready().relation;
                const blocked = () => relation().readiness === 'blocked';
                const blockersUnavailable = () =>
                  relation().hasUnavailableDependencies;
                const [predecessorState, setPredecessorState] =
                  createSignal<DirectionState>(
                    relation().hasUnavailableDependencies
                      ? 'unavailable'
                      : 'loading'
                  );
                return (
                  <div class="basis-full min-w-0 flex flex-col gap-2 text-sm text-ink">
                    <Show when={blocked()}>
                      <p
                        class="text-ink-muted"
                        role="status"
                        aria-live="polite"
                      >
                        {predecessorState() === 'ready' &&
                        !blockersUnavailable()
                          ? 'This task is blocked. Complete its blocking tasks first.'
                          : predecessorState() === 'loading'
                            ? 'This task is blocked. Checking blocking tasks.'
                            : predecessorState() === 'offline'
                              ? 'This task is blocked. Blocking tasks are offline.'
                              : 'This task is blocked. Some dependencies are unavailable.'}
                      </p>
                    </Show>
                    <RelationDirection
                      label="Predecessors"
                      taskIds={relation().dependsOnTaskIds}
                      blockingTaskIds={relation().blockingTaskIds}
                      unavailable={relation().hasUnavailableDependencies}
                      onState={setPredecessorState}
                    />
                    <RelationDirection
                      label="Successors"
                      taskIds={relation().successorTaskIds}
                      unavailable={relation().hasUnavailableSuccessors}
                    />
                    <Show when={!blocked()}>
                      <span class="text-ink-muted">Ready</span>
                    </Show>
                  </div>
                );
              }}
            </Show>
          }
        >
          <span
            role="status"
            aria-live="polite"
            aria-label={rowStatusLabel(current())}
            class="shrink-0 whitespace-nowrap text-xs text-ink-muted"
          >
            {rowStatusLabel(current())}
          </span>
          <Show when={milestoneState()}>
            {(milestone) => (
              <span
                aria-label={`Milestone status: ${milestoneStateLabel(milestone())}`}
                class="shrink-0 whitespace-nowrap text-xs text-ink-muted"
              >
                {milestoneStateLabel(milestone())}
              </span>
            )}
          </Show>
        </Show>
      )}
    </Show>
  );
}
