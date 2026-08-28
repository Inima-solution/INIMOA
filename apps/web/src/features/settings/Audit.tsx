import { SidePanel, useSidePanel } from '@components/app/side-panel/SidePanel';
import { downloadFile } from '@filesystem/download';
import {
  type BusinessAuditReauthenticationOutcome,
  type BusinessAuditRetentionFilter,
  useBusinessAuditAccessQuery,
  useBusinessAuditDetailQuery,
  useBusinessAuditExportMutation,
  useBusinessAuditListQuery,
  useBusinessAuditMfaReauthenticationMutation,
  useBusinessAuditPasswordReauthenticationMutation,
} from '@queries/business-audit/business-audit';
import { useCurrentTeamQuery } from '@queries/team/teams';
import { Button, Dialog, Panel } from '@ui';
import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Match,
  onCleanup,
  onMount,
  Show,
  Switch,
} from 'solid-js';
import {
  auditAffordances,
  auditSurfaceState,
  knownAuditMetadata,
  validateAuditExport,
} from './Audit.helpers';

const formatTime = (value: string) =>
  new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(value));

function useOnline() {
  const [online, setOnline] = createSignal(
    typeof navigator === 'undefined' || navigator.onLine
  );
  onMount(() => {
    const update = () => setOnline(navigator.onLine);
    window.addEventListener('online', update);
    window.addEventListener('offline', update);
    onCleanup(() => {
      window.removeEventListener('online', update);
      window.removeEventListener('offline', update);
    });
  });
  return online;
}

function AuditDetail(props: {
  eventId: () => string | undefined;
  canExport: () => boolean;
  teamId: () => string | undefined;
}) {
  const detail = useBusinessAuditDetailQuery({
    teamId: props.teamId,
    eventId: props.eventId,
    enabled: props.canExport,
  });
  return (
    <SidePanel.Section
      id="audit-detail"
      title="Event details"
      defaultOpen
      order={10}
    >
      <Show when={detail.isLoading}>
        <SidePanel.Loading />
      </Show>
      <Show when={detail.isError}>
        <div class="flex items-center gap-2 text-xs text-ink-muted">
          <span>Details are unavailable.</span>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void detail.refetch()}
          >
            Retry
          </Button>
        </div>
      </Show>
      <Show when={detail.data}>
        {(event) => (
          <SidePanel.Grid>
            <SidePanel.Row label="Action">{event().action}</SidePanel.Row>
            <SidePanel.Row label="Outcome">{event().outcome}</SidePanel.Row>
            <SidePanel.Row label="Actor">{event().actor}</SidePanel.Row>
            <Show when={event().delegated_actor}>
              <SidePanel.Row label="Initiated by">
                {event().delegated_actor}
              </SidePanel.Row>
            </Show>
            <SidePanel.Row label="Target">
              {event().target_type}: {event().target_id}
            </SidePanel.Row>
            <SidePanel.Row label="Time">
              <time class="tabular-nums" dateTime={event().occurred_at}>
                {formatTime(event().occurred_at)}
              </time>
            </SidePanel.Row>
            <SidePanel.Row label="Retention">
              {event().retention_class}
            </SidePanel.Row>
            <SidePanel.Row label="Request ID">
              {event().request_id}
            </SidePanel.Row>
            <Show when={event().reason}>
              <SidePanel.Row label="Reason">{event().reason}</SidePanel.Row>
            </Show>
            <For
              each={knownAuditMetadata(
                event().metadata as Record<string, unknown>
              )}
            >
              {(entry) => (
                <SidePanel.Row label={entry.label}>
                  {String(entry.value)}
                </SidePanel.Row>
              )}
            </For>
          </SidePanel.Grid>
        )}
      </Show>
    </SidePanel.Section>
  );
}

function AuditRows(props: {
  items: Array<{
    id: string;
    action: string;
    outcome: string;
    actor: string;
    delegated_actor?: string | null;
    target_type: string;
    target_id: string;
    occurred_at: string;
  }>;
  canDetail: boolean;
  onSelect: (id: string, target: HTMLButtonElement) => void;
  selectedId: () => string | undefined;
  detailOpen: () => boolean;
}) {
  return (
    <div
      class="divide-y divide-edge-muted"
      role="list"
      aria-label="Audit events"
    >
      <For each={props.items}>
        {(event) => {
          const content = (
            <>
              <span
                class="mt-1.5 size-1.5 shrink-0 rounded-full bg-ink-muted"
                aria-hidden="true"
              />
              <span class="min-w-0 flex-1">
                <span class="block font-medium">
                  {event.action}{' '}
                  <span class="text-ink-muted">· {event.outcome}</span>
                </span>
                <span class="block truncate text-ink-muted">
                  {event.delegated_actor ?? event.actor} → {event.target_type}:{' '}
                  {event.target_id}
                </span>
              </span>
              <time
                class="shrink-0 text-ink-extra-muted tabular-nums"
                dateTime={event.occurred_at}
              >
                {formatTime(event.occurred_at)}
              </time>
            </>
          );
          const rowClass =
            'relative flex w-full min-h-14 items-start gap-3 px-4 py-3 text-left text-xs text-ink';
          return (
            <div role="listitem">
              <Show
                when={props.canDetail}
                fallback={<div class={rowClass}>{content}</div>}
              >
                <button
                  type="button"
                  aria-expanded={
                    props.detailOpen() && props.selectedId() === event.id
                  }
                  onClick={(e) => props.onSelect(event.id, e.currentTarget)}
                  class={`${rowClass} hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40`}
                >
                  {content}
                </button>
              </Show>
            </div>
          );
        }}
      </For>
    </div>
  );
}

function ExportDialog(props: {
  open: boolean;
  retentionClass: () => BusinessAuditRetentionFilter | undefined;
  onClose: () => void;
}) {
  const password = useBusinessAuditPasswordReauthenticationMutation();
  const mfa = useBusinessAuditMfaReauthenticationMutation();
  const exportCsv = useBusinessAuditExportMutation();
  const [reason, setReason] = createSignal('');
  const [from, setFrom] = createSignal('');
  const [until, setUntil] = createSignal('');
  const [passwordValue, setPasswordValue] = createSignal('');
  const [mfaChallenge, setMfaChallenge] =
    createSignal<
      Extract<BusinessAuditReauthenticationOutcome, { kind: 'mfa_required' }>
    >();
  const [mfaCode, setMfaCode] = createSignal('');
  const [error, setError] = createSignal<string>();
  const clearSecrets = () => {
    setPasswordValue('');
    setMfaCode('');
    setMfaChallenge(undefined);
  };
  const close = () => {
    clearSecrets();
    setError(undefined);
    props.onClose();
  };
  const runExport = async (receipt: string) => {
    const validation = validateAuditExport(reason(), from(), until());
    if (!validation.valid) {
      setError(validation.reasonError ?? validation.dateError);
      return;
    }
    try {
      const result = await exportCsv.mutateAsync({
        reauthenticationReceipt: receipt,
        from: validation.window.fromUtc,
        until: validation.window.toUtcExclusive,
        retentionClass: props.retentionClass(),
        reason: reason().trim(),
      });
      // The client payload may be backed by a SharedArrayBuffer. Copy it into
      // a fresh ArrayBuffer-backed view before passing it to Blob, which only
      // accepts transferable BlobPart buffers in this TypeScript lib target.
      const downloadBytes = new Uint8Array(result.bytes.byteLength);
      downloadBytes.set(result.bytes);
      downloadFile(
        new Blob([downloadBytes.buffer], { type: result.contentType }),
        'business-audit.csv'
      );
      clearSecrets();
      close();
    } catch {
      clearSecrets();
      setError(
        'Export could not be completed. Check the period, then try again.'
      );
    }
  };
  const handlePassword = async () => {
    const validation = validateAuditExport(reason(), from(), until());
    if (!validation.valid) {
      setError(validation.reasonError ?? validation.dateError);
      return;
    }
    try {
      const outcome = await password.mutateAsync(passwordValue());
      setPasswordValue('');
      if (outcome.kind === 'receipt')
        await runExport(outcome.reauthenticationReceipt);
      else setMfaChallenge(outcome);
    } catch {
      setPasswordValue('');
      setError('Password verification failed. Try again.');
    }
  };
  const handleMfa = async () => {
    const challenge = mfaChallenge();
    if (!challenge) return;
    try {
      const outcome = await mfa.mutateAsync({
        twoFactorId: challenge.twoFactorId,
        code: mfaCode(),
      });
      setMfaCode('');
      if (outcome.kind === 'receipt')
        await runExport(outcome.reauthenticationReceipt);
      else setError('Verification could not be completed. Try again.');
    } catch {
      setMfaCode('');
      setError('Verification code was not accepted. Try again.');
    }
  };
  return (
    <Dialog open={props.open} onOpenChange={(open) => !open && close()}>
      <Panel depth={2} class="ph-no-capture max-h-[75vh] text-ink rounded-xl">
        <Panel.Header class="px-3">
          <Dialog.Title class="text-sm font-medium">
            Export audit records
          </Dialog.Title>
        </Panel.Header>
        <Panel.Body class="flex flex-col gap-3 p-3">
          <Dialog.Description class="text-pretty text-xs text-ink-muted">
            UTC dates use a half-open period. Exports are limited to 31 days and
            1,000 rows. You will verify your identity once before the file is
            created.
          </Dialog.Description>
          <label class="flex flex-col gap-1 text-xs text-ink-muted">
            Reason
            <textarea
              class="settings-input min-h-16"
              maxLength={1000}
              value={reason()}
              onInput={(e) => setReason(e.currentTarget.value)}
            />
          </label>
          <div class="grid grid-cols-2 gap-2 mobile:grid-cols-1">
            <label class="flex flex-col gap-1 text-xs text-ink-muted">
              From
              <input
                class="settings-input"
                type="date"
                value={from()}
                onInput={(e) => setFrom(e.currentTarget.value)}
              />
            </label>
            <label class="flex flex-col gap-1 text-xs text-ink-muted">
              To
              <input
                class="settings-input"
                type="date"
                value={until()}
                onInput={(e) => setUntil(e.currentTarget.value)}
              />
            </label>
          </div>
          <Show
            when={mfaChallenge()}
            fallback={
              <label class="flex flex-col gap-1 text-xs text-ink-muted">
                Password
                <input
                  class="settings-input"
                  type="password"
                  autocomplete="current-password"
                  maxLength={1024}
                  value={passwordValue()}
                  onInput={(e) => setPasswordValue(e.currentTarget.value)}
                />
              </label>
            }
          >
            <label class="flex flex-col gap-1 text-xs text-ink-muted">
              Verification code{' '}
              <span class="text-ink-extra-muted">
                (
                {mfaChallenge()
                  ?.methods.map((method) => method.method)
                  .join(', ')}
                )
              </span>
              <input
                class="settings-input"
                inputmode="numeric"
                autocomplete="one-time-code"
                maxLength={1024}
                value={mfaCode()}
                onInput={(e) => setMfaCode(e.currentTarget.value)}
              />
            </label>
          </Show>
          <Show when={error()}>
            <p class="text-xs text-failure-ink" aria-live="polite">
              {error()}
            </p>
          </Show>
          <div class="flex justify-end gap-2">
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button
              variant="active"
              onClick={() =>
                void (mfaChallenge() ? handleMfa() : handlePassword())
              }
              disabled={
                password.isPending || mfa.isPending || exportCsv.isPending
              }
            >
              Verify and export
            </Button>
          </div>
        </Panel.Body>
      </Panel>
    </Dialog>
  );
}

function AuditContent() {
  const team = useCurrentTeamQuery();
  const teamId = () => team.data?.team.id;
  const online = useOnline();
  const access = useBusinessAuditAccessQuery(teamId);
  const [retentionClass, setRetentionClass] =
    createSignal<BusinessAuditRetentionFilter>();
  const list = useBusinessAuditListQuery({ teamId, retentionClass });
  const [selectedId, setSelectedId] = createSignal<string>();
  const [exportOpen, setExportOpen] = createSignal(false);
  const affordances = createMemo(() =>
    auditAffordances({
      canRead: access.data?.can_read ?? false,
      canExport: access.data?.can_export ?? false,
    })
  );
  const items = () => list.data?.pages.flatMap((page) => page.items) ?? [];
  const sidePanel = useSidePanel();
  const surfaceState = createMemo(() =>
    auditSurfaceState({
      loading: team.isLoading || access.isLoading,
      online: online(),
      teamError: team.isError,
      accessError: access.isError,
      canRead: affordances().showList,
    })
  );
  let previousTeamId: string | undefined;
  let detailTrigger: HTMLButtonElement | undefined;
  let detailWasOpen = false;
  const closeDetailProgrammatically = () => {
    detailTrigger = undefined;
    setSelectedId(undefined);
    sidePanel?.setIsOpen(false);
  };
  createEffect(() => {
    const detailOpen = sidePanel?.isOpen() ?? false;
    if (detailWasOpen && !detailOpen && detailTrigger) {
      const trigger = detailTrigger;
      detailTrigger = undefined;
      trigger.focus({ preventScroll: true });
    }
    detailWasOpen = detailOpen;
  });
  createEffect(() => {
    const nextTeamId = teamId();
    if (nextTeamId !== previousTeamId) {
      previousTeamId = nextTeamId;
      closeDetailProgrammatically();
      setExportOpen(false);
    }
  });
  createEffect(() => {
    if (affordances().showExport) return;
    closeDetailProgrammatically();
    setExportOpen(false);
  });
  const select = (id: string, target: HTMLButtonElement) => {
    if (!affordances().allowDetail) return;
    detailTrigger = target;
    setSelectedId(id);
    sidePanel?.setIsOpen(true);
  };
  return (
    <>
      <div class="ph-no-capture flex size-full min-h-0 flex-col">
        <header class="flex shrink-0 items-center justify-between gap-3 border-b border-edge-muted px-6 py-4">
          <div>
            <h1 class="text-lg font-semibold text-ink">Audit</h1>
            <p class="text-xs text-ink-muted">Sensitive organization actions</p>
          </div>
          <Show when={affordances().showExport}>
            <Button
              size="sm"
              variant="base"
              onClick={() => setExportOpen(true)}
            >
              Export
            </Button>
          </Show>
        </header>
        <Show when={surfaceState() === 'loading'}>
          <p class="p-6 text-sm text-ink-muted" aria-live="polite">
            Checking audit access…
          </p>
        </Show>
        <Show when={surfaceState() === 'offline'}>
          <div class="p-6 text-sm text-ink-muted" aria-live="polite">
            You appear to be offline. Reconnect, then retry.
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void access.refetch()}
            >
              Retry
            </Button>
          </div>
        </Show>
        <Show when={surfaceState() === 'team-error'}>
          <div class="p-6 text-sm text-ink-muted">
            Your workspace could not be loaded.
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void team.refetch()}
            >
              Retry
            </Button>
          </div>
        </Show>
        <Show when={surfaceState() === 'access-error'}>
          <div class="p-6 text-sm text-ink-muted">
            Audit access could not be checked.
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void access.refetch()}
            >
              Retry
            </Button>
          </div>
        </Show>
        <Show when={surfaceState() === 'permission-denied'}>
          <p class="p-6 text-sm text-ink-muted">
            You do not have permission to view business audit records.
          </p>
        </Show>
        <Show when={surfaceState() === 'ready'}>
          <Switch>
            <Match when={affordances().showList}>
              <div class="min-h-0 flex-1 overflow-y-auto">
                <div class="flex items-center gap-2 border-b border-edge-muted px-4 py-2">
                  <label class="text-xs text-ink-muted" for="audit-retention">
                    Retention
                  </label>
                  <select
                    id="audit-retention"
                    class="settings-input text-xs"
                    value={retentionClass() ?? ''}
                    onChange={(e) =>
                      setRetentionClass(
                        (e.currentTarget
                          .value as BusinessAuditRetentionFilter) || undefined
                      )
                    }
                  >
                    <option value="">All approved records</option>
                    <option value="standard">Standard</option>
                    <option value="confidential">Confidential</option>
                    <option value="restricted">Restricted</option>
                  </select>
                </div>
                <Show when={list.isError}>
                  <div class="p-6 text-sm text-ink-muted">
                    Audit records are unavailable.{' '}
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => void list.refetch()}
                    >
                      Retry
                    </Button>
                  </div>
                </Show>
                <Show when={!list.isError && list.isLoading}>
                  <p class="p-6 text-sm text-ink-muted" aria-live="polite">
                    Loading audit records…
                  </p>
                </Show>
                <Show
                  when={
                    !list.isError && !list.isLoading && items().length === 0
                  }
                >
                  <p class="p-6 text-sm text-ink-muted">
                    {retentionClass()
                      ? 'No records match this retention filter.'
                      : 'No audit records yet.'}
                  </p>
                </Show>
                <Show when={items().length > 0}>
                  <AuditRows
                    items={items()}
                    canDetail={affordances().allowDetail}
                    onSelect={select}
                    selectedId={selectedId}
                    detailOpen={() => sidePanel?.isOpen() ?? false}
                  />
                  <Show when={list.hasNextPage}>
                    <div class="flex justify-center p-3">
                      <Button
                        variant="ghost"
                        onClick={() => void list.fetchNextPage()}
                        disabled={list.isFetchingNextPage}
                      >
                        {list.isFetchingNextPage ? 'Loading…' : 'Show more'}
                      </Button>
                    </div>
                  </Show>
                </Show>
              </div>
            </Match>
          </Switch>
        </Show>
      </div>
      <Show when={affordances().allowDetail}>
        <AuditDetail
          teamId={teamId}
          eventId={selectedId}
          canExport={() => affordances().allowDetail}
        />
      </Show>
      <Show when={affordances().showExport}>
        <ExportDialog
          open={exportOpen()}
          onClose={() => setExportOpen(false)}
          retentionClass={retentionClass}
        />
      </Show>
    </>
  );
}

export function Audit() {
  return (
    <SidePanel.Layout defaultOpen={false}>
      <AuditContent />
    </SidePanel.Layout>
  );
}
