import { ProgressMeter } from '@core/component/LexicalMarkdown/component/status/Progress';
import type { ProgressStats } from '@core/component/LexicalMarkdown/plugins';
import { thrownResultErrorHasCode } from '@core/util/result';
import { propertiesKeys } from '@queries/properties/keys';
import { fetchTaskSubtaskProgress } from '@queries/properties/task-subtask-progress';
import { useQueries } from '@tanstack/solid-query';
import {
  type Accessor,
  createContext,
  createMemo,
  type ParentProps,
  Show,
  useContext,
} from 'solid-js';
import type { Store } from 'solid-js/store';

const TASK_IDS_PER_REQUEST = 200;

type TaskSubtaskProgressState =
  | { kind: 'loading' }
  | { kind: 'ready'; completedSubtasks: number; totalSubtasks: number }
  | { kind: 'empty' }
  | { kind: 'unavailable' }
  | { kind: 'error' }
  | { kind: 'offline' };

type TaskSubtaskProgressContextValue = {
  progressForTask: (taskId: string) => TaskSubtaskProgressState | undefined;
};

const TaskSubtaskProgressContext =
  createContext<TaskSubtaskProgressContextValue>();

function uniqueTaskIds(taskIds: readonly string[]) {
  const seen = new Set<string>();
  return taskIds.filter((taskId) => {
    if (seen.has(taskId)) return false;
    seen.add(taskId);
    return true;
  });
}

function chunkTaskIds(taskIds: readonly string[]) {
  const chunks: string[][] = [];
  for (let index = 0; index < taskIds.length; index += TASK_IDS_PER_REQUEST) {
    chunks.push(taskIds.slice(index, index + TASK_IDS_PER_REQUEST));
  }
  return chunks;
}

function isUnavailableError(error: unknown) {
  return (
    thrownResultErrorHasCode(error, 'UNAUTHORIZED') ||
    thrownResultErrorHasCode(error, 'FORBIDDEN') ||
    thrownResultErrorHasCode(error, 'NOT_FOUND')
  );
}

function progressStateLabel(
  kind: TaskSubtaskProgressState['kind'],
  compact: boolean
) {
  if (kind === 'ready') return '';

  if (compact) {
    return {
      loading: 'Loading…',
      empty: 'No subtasks',
      unavailable: 'Unavailable',
      error: 'Load failed',
      offline: 'Offline',
    }[kind];
  }

  return {
    loading: 'Loading subtask progress',
    empty: 'No subtasks',
    unavailable: 'Subtask progress unavailable',
    error: "Couldn't load subtask progress",
    offline: 'Subtask progress offline',
  }[kind];
}

export function TaskSubtaskProgressProvider(
  props: ParentProps<{ taskIds: Accessor<readonly string[]> }>
) {
  const taskIds = createMemo(() => uniqueTaskIds(props.taskIds()));
  const taskIdChunks = createMemo(() => chunkTaskIds(taskIds()));
  const queries = useQueries(() => ({
    queries: taskIdChunks().map((ids) => ({
      queryKey: propertiesKeys.taskSubtaskProgress(ids).queryKey,
      queryFn: () => fetchTaskSubtaskProgress(ids),
      enabled: ids.length > 0,
    })),
  }));

  const progressForTask = (
    taskId: string
  ): TaskSubtaskProgressState | undefined => {
    const ids = taskIds();
    const taskIndex = ids.indexOf(taskId);
    if (taskIndex === -1) return undefined;

    const query = queries[Math.floor(taskIndex / TASK_IDS_PER_REQUEST)];
    if (!query || query.fetchStatus === 'paused') return { kind: 'offline' };
    if (query.isPending) return { kind: 'loading' };
    if (query.isError) {
      return isUnavailableError(query.error)
        ? { kind: 'unavailable' }
        : { kind: 'error' };
    }

    const progress = query.data?.find((item) => item.taskId === taskId);
    if (!progress || progress.hasUnavailableSubtasks) {
      return { kind: 'unavailable' };
    }
    if (progress.totalSubtasks === 0) return { kind: 'empty' };

    return {
      kind: 'ready',
      completedSubtasks: progress.completedSubtasks,
      totalSubtasks: progress.totalSubtasks,
    };
  };

  return (
    <TaskSubtaskProgressContext.Provider value={{ progressForTask }}>
      {props.children}
    </TaskSubtaskProgressContext.Provider>
  );
}

export function TaskSubtaskProgressIndicator(props: {
  taskId: string;
  mode?: 'detail' | 'row';
}) {
  const context = useContext(TaskSubtaskProgressContext);
  const progress = () => context?.progressForTask(props.taskId);
  const readyProgress = () => {
    const state = progress();
    return state?.kind === 'ready' ? state : undefined;
  };
  const isDetail = () => props.mode !== 'row';
  const indicatorClass = () =>
    isDetail() ? 'min-w-32 text-sm' : 'min-w-24 text-xs';
  const statusAttributes = () =>
    isDetail()
      ? { role: 'status' as const, 'aria-live': 'polite' as const }
      : {};

  return (
    <Show when={progress()}>
      {(state) => (
        <Show
          when={state().kind === 'ready'}
          fallback={
            <span
              aria-label={
                isDetail() ? undefined : progressStateLabel(state().kind, false)
              }
              class={`text-ink-muted ${indicatorClass()}`}
              {...statusAttributes()}
            >
              {progressStateLabel(state().kind, !isDetail())}
            </span>
          }
        >
          <Show when={readyProgress()}>
            {(ready) => (
              <div
                aria-label={`${ready().completedSubtasks} of ${ready().totalSubtasks} subtasks complete`}
                {...statusAttributes()}
              >
                <ProgressMeter
                  class={indicatorClass()}
                  stats={
                    {
                      get completed() {
                        return ready().completedSubtasks;
                      },
                      get total() {
                        return ready().totalSubtasks;
                      },
                    } as Store<ProgressStats>
                  }
                />
              </div>
            )}
          </Show>
        </Show>
      )}
    </Show>
  );
}
