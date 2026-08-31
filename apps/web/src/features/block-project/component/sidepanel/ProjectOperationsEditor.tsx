import { getDisplayName, tryMacroId } from '@core/user';
import { thrownResultErrorHasCode } from '@core/util/result';
import { useReplaceProjectOperationsMutation } from '@queries/storage/project-operations';
import { useCurrentTeamQuery } from '@queries/team/teams';
import type {
  ProjectOperationalStatus,
  ProjectOperations,
  ProjectPriority,
} from '@service-storage/generated/schemas';
import { Button, Dialog, Panel } from '@ui';
import { createMemo, createSignal, For, Show } from 'solid-js';

const statuses: { value: ProjectOperationalStatus; label: string }[] = [
  { value: 'planned', label: 'Planned' },
  { value: 'active', label: 'Active' },
  { value: 'paused', label: 'Paused' },
  { value: 'completed', label: 'Completed' },
  { value: 'archived', label: 'Archived' },
];

const priorities: { value: ProjectPriority; label: string }[] = [
  { value: 'low', label: 'Low' },
  { value: 'normal', label: 'Normal' },
  { value: 'high', label: 'High' },
  { value: 'urgent', label: 'Urgent' },
];

const dateOrderError = 'Start date must be on or before target date.';

type FormValues = {
  status: ProjectOperationalStatus;
  priority: ProjectPriority;
  leadUserId: string | null;
  startDate: string;
  targetDate: string;
};

function valuesFrom(operations: ProjectOperations): FormValues {
  return {
    status: operations.status,
    priority: operations.priority,
    leadUserId: operations.leadUserId ?? null,
    startDate: operations.startDate ?? '',
    targetDate: operations.targetDate ?? '',
  };
}

export function ProjectOperationsEditor(props: {
  open: boolean;
  operations: () => ProjectOperations;
  onClose: () => void;
  refetchOverview: () => Promise<unknown>;
}) {
  const teamQuery = useCurrentTeamQuery(() => props.open);
  const replaceOperations = useReplaceProjectOperationsMutation();
  const [values, setValues] = createSignal(valuesFrom(props.operations()));
  const [error, setError] = createSignal<string>();
  const isPending = () => replaceOperations.isPending;
  const hasDateOrderError = () => error() === dateOrderError;
  const members = createMemo(() => teamQuery.data?.members ?? []);
  const leadChoices = createMemo(() => {
    const currentLead = props.operations().leadUserId ?? null;
    const ids = new Set(members().map((member) => member.user_id));
    if (currentLead) ids.add(currentLead);
    return [...ids];
  });
  const leadIsAvailable = () => Boolean(teamQuery.data);

  const update = <K extends keyof FormValues>(key: K, value: FormValues[K]) => {
    if (isPending()) return;
    setValues((current) => ({ ...current, [key]: value }));
    setError(undefined);
  };

  const close = () => {
    if (!isPending()) props.onClose();
  };

  const submit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (isPending()) return;

    const current = values();
    if (
      current.startDate &&
      current.targetDate &&
      current.startDate > current.targetDate
    ) {
      setError(dateOrderError);
      return;
    }

    const operations = props.operations();
    try {
      await replaceOperations.mutateAsync({
        projectId: operations.projectId,
        request: {
          expectedUpdatedAt: operations.updatedAt,
          status: current.status,
          priority: current.priority,
          leadUserId: leadIsAvailable()
            ? current.leadUserId
            : (operations.leadUserId ?? null),
          startDate: current.startDate || null,
          targetDate: current.targetDate || null,
          policy: operations.policy,
        },
      });
      props.onClose();
    } catch (cause) {
      if (thrownResultErrorHasCode(cause, 'CONFLICT')) {
        setError(
          'This project was updated elsewhere. The latest details are now loaded.'
        );
        await props.refetchOverview();
        setValues(valuesFrom(props.operations()));
        return;
      }
      setError('Unable to save project details. Please try again.');
    }
  };

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => !open && close()}
      position="center"
      class="w-120"
    >
      <Panel depth={2} class="rounded-xl *:max-h-[75vh]">
        <Panel.Header class="px-4 pt-4">
          <Dialog.Title class="text-ink text-sm font-semibold">
            Edit project details
          </Dialog.Title>
        </Panel.Header>
        <Panel.Body class="p-4">
          <form
            class="flex flex-col gap-4"
            aria-busy={isPending()}
            onSubmit={submit}
          >
            <Field label="Status" for="project-operation-status">
              <select
                id="project-operation-status"
                class="settings-input w-full"
                disabled={isPending()}
                value={values().status}
                onChange={(event) =>
                  update(
                    'status',
                    event.currentTarget.value as ProjectOperationalStatus
                  )
                }
              >
                <For each={statuses}>
                  {(status) => (
                    <option value={status.value}>{status.label}</option>
                  )}
                </For>
              </select>
            </Field>
            <Field label="Priority" for="project-operation-priority">
              <select
                id="project-operation-priority"
                class="settings-input w-full"
                disabled={isPending()}
                value={values().priority}
                onChange={(event) =>
                  update(
                    'priority',
                    event.currentTarget.value as ProjectPriority
                  )
                }
              >
                <For each={priorities}>
                  {(priority) => (
                    <option value={priority.value}>{priority.label}</option>
                  )}
                </For>
              </select>
            </Field>
            <Field label="Lead" for="project-operation-lead">
              <select
                id="project-operation-lead"
                class="settings-input w-full"
                disabled={isPending() || !leadIsAvailable()}
                value={values().leadUserId ?? ''}
                onChange={(event) =>
                  update('leadUserId', event.currentTarget.value || null)
                }
              >
                <option value="">Unassigned</option>
                <For each={leadChoices()}>
                  {(userId) => (
                    <option value={userId}>
                      {getDisplayName(tryMacroId(userId))}
                    </option>
                  )}
                </For>
              </select>
              <Show when={!leadIsAvailable()}>
                <p class="text-ink-extra-muted text-xs">
                  Team members are unavailable. The current lead will be kept.
                </p>
              </Show>
            </Field>
            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field label="Start date" for="project-operation-start-date">
                <input
                  id="project-operation-start-date"
                  type="date"
                  class="settings-input w-full"
                  disabled={isPending()}
                  value={values().startDate}
                  aria-invalid={hasDateOrderError()}
                  aria-describedby={
                    hasDateOrderError()
                      ? 'project-operation-date-order-error'
                      : undefined
                  }
                  onInput={(event) =>
                    update('startDate', event.currentTarget.value)
                  }
                />
              </Field>
              <Field label="Target date" for="project-operation-target-date">
                <input
                  id="project-operation-target-date"
                  type="date"
                  class="settings-input w-full"
                  disabled={isPending()}
                  value={values().targetDate}
                  aria-invalid={hasDateOrderError()}
                  aria-describedby={
                    hasDateOrderError()
                      ? 'project-operation-date-order-error'
                      : undefined
                  }
                  onInput={(event) =>
                    update('targetDate', event.currentTarget.value)
                  }
                />
              </Field>
            </div>
            <Show when={error()}>
              {(message) => (
                <p
                  id={
                    hasDateOrderError()
                      ? 'project-operation-date-order-error'
                      : undefined
                  }
                  role="alert"
                  class="text-sm text-failure-ink"
                >
                  {message()}
                </p>
              )}
            </Show>
            <div class="flex items-center justify-end gap-2">
              <Button
                type="button"
                variant="base"
                size="sm"
                disabled={isPending()}
                onClick={close}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                variant="cta"
                size="sm"
                disabled={isPending()}
                aria-label={isPending() ? 'Saving project details' : undefined}
              >
                {isPending() ? 'Saving…' : 'Save'}
              </Button>
            </div>
          </form>
        </Panel.Body>
      </Panel>
    </Dialog>
  );
}

function Field(props: {
  label: string;
  for: string;
  children: import('solid-js').JSX.Element;
}) {
  return (
    <div class="flex flex-col gap-1.5">
      <label for={props.for} class="text-xs font-medium text-ink-muted">
        {props.label}
      </label>
      {props.children}
    </div>
  );
}
