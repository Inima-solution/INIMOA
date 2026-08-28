import { OwnerValue, SidePanel } from '@components/app/side-panel';
import { useBlockId } from '@core/block';
import { formatDate } from '@core/util/date';
import { thrownResultErrorHasCode } from '@core/util/result';
import { useProjectOverviewQuery } from '@queries/storage/project-overview';
import { Button } from '@ui';
import { createMemo, Match, Show, Switch } from 'solid-js';

const statusLabels = {
  planned: 'Planned',
  active: 'Active',
  paused: 'Paused',
  completed: 'Completed',
  archived: 'Archived',
} as const;

const priorityLabels = {
  low: 'Low',
  normal: 'Normal',
  high: 'High',
  urgent: 'Urgent',
} as const;

function formatDateOnly(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeZone: 'UTC',
  }).format(new Date(`${value}T00:00:00Z`));
}

export function ProjectOverviewSection(props: { order?: number }) {
  const projectId = useBlockId();
  const query = useProjectOverviewQuery(() => projectId);
  const hasAccessError = createMemo(
    () =>
      thrownResultErrorHasCode(query.error, 'UNAUTHORIZED') ||
      thrownResultErrorHasCode(query.error, 'FORBIDDEN')
  );
  const isPausedWithoutData = createMemo(
    () => query.fetchStatus === 'paused' && query.isPending
  );

  return (
    <SidePanel.Section
      id="project-overview"
      title="Overview"
      defaultOpen
      order={props.order}
    >
      <Switch>
        <Match when={hasAccessError()}>
          <OverviewMessage>
            Project overview is not available to you.
          </OverviewMessage>
        </Match>
        <Match when={isPausedWithoutData()}>
          <OverviewMessage>
            Project overview is unavailable while offline.
          </OverviewMessage>
        </Match>
        <Match when={query.isError}>
          <div class="flex items-center gap-2 p-2" aria-live="polite">
            <span class="text-ink-muted text-sm">
              Project overview is unavailable.
            </span>
            <Button size="sm" onClick={() => void query.refetch()}>
              Retry
            </Button>
          </div>
        </Match>
        <Match when={query.data}>
          {(overview) => (
            <>
              <Show when={query.fetchStatus === 'paused'}>
                <p class="px-2 pb-1 text-ink-muted text-sm" aria-live="polite">
                  Showing the last loaded overview while offline.
                </p>
              </Show>
              <SidePanel.Grid>
                <SidePanel.Row label="Status">
                  <SidePanel.Pill>
                    <span class="truncate">
                      {statusLabels[overview().operations.status]}
                    </span>
                  </SidePanel.Pill>
                </SidePanel.Row>
                <SidePanel.Row label="Priority">
                  <SidePanel.Pill>
                    <span class="truncate">
                      {priorityLabels[overview().operations.priority]}
                    </span>
                  </SidePanel.Pill>
                </SidePanel.Row>
                <SidePanel.Row label="Lead">
                  <Show
                    when={overview().operations.leadUserId}
                    fallback={<SidePanel.EmptyPill label="Unassigned" />}
                  >
                    {(leadUserId) => <OwnerValue ownerId={leadUserId()} />}
                  </Show>
                </SidePanel.Row>
                <DateOnlyRow
                  label="Start"
                  value={overview().operations.startDate}
                />
                <DateOnlyRow
                  label="Target"
                  value={overview().operations.targetDate}
                />
                <Show when={overview().operations.completedAt}>
                  {(completedAt) => (
                    <SidePanel.Row label="Completed">
                      <SidePanel.Pill>
                        <span class="truncate">
                          {formatDate(completedAt(), { showTime: true })}
                        </span>
                      </SidePanel.Pill>
                    </SidePanel.Row>
                  )}
                </Show>
                <SidePanel.Row label="Child projects">
                  <CountValue
                    value={overview().immediateChildren.childProjects}
                  />
                </SidePanel.Row>
                <SidePanel.Row label="Tasks">
                  <CountValue value={overview().immediateChildren.tasks} />
                </SidePanel.Row>
                <SidePanel.Row label="Files">
                  <CountValue
                    value={overview().immediateChildren.nonTaskDocuments}
                  />
                </SidePanel.Row>
                <SidePanel.Row label="Chats">
                  <CountValue value={overview().immediateChildren.chats} />
                </SidePanel.Row>
              </SidePanel.Grid>
            </>
          )}
        </Match>
        <Match when>
          <div role="status" aria-live="polite" aria-label="Loading overview">
            <SidePanel.Loading />
          </div>
        </Match>
      </Switch>
    </SidePanel.Section>
  );
}

function OverviewMessage(props: { children: string }) {
  return (
    <p class="p-2 text-ink-muted text-sm" aria-live="polite">
      {props.children}
    </p>
  );
}

function DateOnlyRow(props: {
  label: string;
  value: string | null | undefined;
}) {
  return (
    <SidePanel.Row label={props.label}>
      <Show
        when={props.value}
        fallback={<SidePanel.EmptyPill label="Not set" />}
      >
        {(value) => (
          <SidePanel.Pill>
            <span class="truncate">{formatDateOnly(value())}</span>
          </SidePanel.Pill>
        )}
      </Show>
    </SidePanel.Row>
  );
}

function CountValue(props: { value: number }) {
  return (
    <SidePanel.Pill>
      <span class="truncate tabular-nums">{props.value}</span>
    </SidePanel.Pill>
  );
}
