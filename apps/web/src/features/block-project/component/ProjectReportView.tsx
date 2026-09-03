import { thrownResultErrorHasCode } from '@core/util/result';
import type { useProjectOverviewQuery } from '@queries/storage/project-overview';
import { Button } from '@ui';
import { createMemo, Match, Show, Switch } from 'solid-js';

export function ProjectReportView(props: {
  query: ReturnType<typeof useProjectOverviewQuery>;
}) {
  const query = props.query;
  const hasAccessError = createMemo(
    () =>
      thrownResultErrorHasCode(query.error, 'UNAUTHORIZED') ||
      thrownResultErrorHasCode(query.error, 'FORBIDDEN')
  );
  const isPausedWithoutData = createMemo(
    () => query.fetchStatus === 'paused' && query.isPending
  );

  return (
    <div
      class="size-full overflow-y-auto bg-surface p-4 sm:p-6"
      data-testid="project-report-view"
    >
      <div class="mx-auto flex w-full max-w-4xl flex-col gap-5">
        <header>
          <h2 class="font-semibold text-lg text-ink">Project report</h2>
          <p class="mt-1 text-ink-muted text-sm">
            Current health for direct, live project tasks.
          </p>
        </header>
        <Switch>
          <Match when={hasAccessError()}>
            <ReportMessage>
              Project report is not available to you.
            </ReportMessage>
          </Match>
          <Match when={isPausedWithoutData()}>
            <ReportMessage>
              Project report is unavailable while offline.
            </ReportMessage>
          </Match>
          <Match when={query.isError}>
            <div class="flex items-center gap-2" aria-live="polite">
              <span class="text-ink-muted text-sm">
                Project report is unavailable.
              </span>
              <Button size="sm" onClick={() => void query.refetch()}>
                Retry
              </Button>
            </div>
          </Match>
          <Match when={query.data}>
            {(overview) => {
              const completionAvailable = () =>
                !overview().progress.hasUnavailableStatuses;
              const completionRate = () => {
                const { completedTasks, includedTasks } = overview().progress;
                return includedTasks === 0
                  ? undefined
                  : Math.round((completedTasks / includedTasks) * 100);
              };
              const riskAvailable = () =>
                !overview().risk.hasUnavailableRiskData;

              return (
                <>
                  <Show when={query.fetchStatus === 'paused'}>
                    <p class="text-ink-muted text-sm" aria-live="polite">
                      Showing the last loaded report while offline.
                    </p>
                  </Show>
                  <section aria-label="Current health">
                    <h3 class="mb-2 font-medium text-ink text-sm">
                      Current health
                    </h3>
                    <dl class="grid grid-cols-1 gap-3 sm:grid-cols-3">
                      <Metric
                        label="Completion rate"
                        value={
                          completionAvailable()
                            ? completionRate() === undefined
                              ? 'N/A'
                              : `${completionRate()}%`
                            : 'Unavailable'
                        }
                        detail={
                          completionAvailable()
                            ? overview().progress.includedTasks === 0
                              ? 'No eligible tasks'
                              : `${overview().progress.completedTasks} of ${overview().progress.includedTasks} complete`
                            : 'Task status data needs attention'
                        }
                      />
                      <Metric
                        label="Overdue"
                        value={
                          riskAvailable()
                            ? String(overview().risk.overdueTasks)
                            : 'Unavailable'
                        }
                        detail="Due before today and still open"
                      />
                      <Metric
                        label="Blocked"
                        value={
                          riskAvailable()
                            ? String(overview().risk.blockedTasks)
                            : 'Unavailable'
                        }
                        detail="Open tasks with unmet dependencies"
                      />
                    </dl>
                  </section>
                  <section
                    class="rounded-lg border border-border p-4"
                    aria-label="Historical flow"
                  >
                    <h3 class="font-medium text-ink text-sm">
                      Historical flow
                    </h3>
                    <p class="mt-1 text-ink-muted text-sm">
                      Throughput and lead time are unavailable until complete
                      task transition history is recorded.
                    </p>
                  </section>
                </>
              );
            }}
          </Match>
          <Match when>
            <div role="status" aria-live="polite">
              <ReportMessage>Loading project report…</ReportMessage>
            </div>
          </Match>
        </Switch>
      </div>
    </div>
  );
}

function Metric(props: { label: string; value: string; detail: string }) {
  return (
    <div class="rounded-lg border border-border p-4">
      <dt class="text-ink-muted text-sm">{props.label}</dt>
      <dd class="mt-1 font-semibold text-2xl text-ink tabular-nums">
        {props.value}
      </dd>
      <dd class="mt-1 text-ink-muted text-xs">{props.detail}</dd>
    </div>
  );
}

function ReportMessage(props: { children: string }) {
  return (
    <p class="text-ink-muted text-sm" aria-live="polite">
      {props.children}
    </p>
  );
}
