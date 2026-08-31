/** @vitest-environment jsdom */

import type { ProjectOperations } from '@service-storage/generated/schemas';
import { cleanup, fireEvent, render, waitFor } from '@solidjs/testing-library';
import { createSignal, type JSX } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  mutateAsync: vi.fn(),
  mutation: {} as Record<string, unknown>,
  pending: (() => false) as () => boolean,
  setPending: (_value: boolean) => {},
  refetch: vi.fn(),
  team: {} as Record<string, unknown>,
}));

vi.mock('@queries/storage/project-operations', () => ({
  useReplaceProjectOperationsMutation: () => mocks.mutation,
}));
vi.mock('@queries/team/teams', () => ({
  useCurrentTeamQuery: () => mocks.team,
}));
vi.mock('@core/user', () => ({
  tryMacroId: (id: string) => id,
  getDisplayName: (id: string) =>
    id === 'macro|lead@example.com' ? 'Lead Name' : 'Second Member',
}));
vi.mock('@core/util/result', () => ({
  thrownResultErrorHasCode: (error: { code?: string }, code: string) =>
    error?.code === code,
}));
vi.mock('@ui', () => {
  const Dialog = (props: { children: JSX.Element; open: boolean }) =>
    props.open ? <div role="dialog">{props.children}</div> : null;
  Dialog.Title = (props: { children: JSX.Element }) => (
    <h2>{props.children}</h2>
  );
  return {
    Dialog,
    Panel: Object.assign(
      (props: { children: JSX.Element }) => <div>{props.children}</div>,
      {
        Header: (props: { children: JSX.Element }) => (
          <header>{props.children}</header>
        ),
        Body: (props: { children: JSX.Element }) => <div>{props.children}</div>,
      }
    ),
    Button: (props: JSX.ButtonHTMLAttributes<HTMLButtonElement>) => (
      <button {...props} />
    ),
  };
});

import { ProjectOperationsEditor } from './ProjectOperationsEditor';

const operations: ProjectOperations = {
  projectId: 'project-1',
  updatedAt: '2026-01-01T00:00:00.000Z',
  createdAt: '2025-12-01T00:00:00.000Z',
  status: 'active',
  priority: 'high',
  leadUserId: 'macro|lead@example.com',
  startDate: '2026-01-02',
  targetDate: '2026-02-03',
  policy: { keep: ['every', 'byte'] },
};

function renderEditor(initial = operations) {
  const [current, setCurrent] = createSignal(initial);
  const onClose = vi.fn();
  return Object.assign(
    render(() => (
      <ProjectOperationsEditor
        open
        operations={current}
        onClose={onClose}
        refetchOverview={mocks.refetch}
      />
    )),
    { current, setCurrent, onClose }
  );
}

beforeEach(() => {
  mocks.mutateAsync.mockReset();
  mocks.refetch.mockReset();
  mocks.refetch.mockResolvedValue(undefined);
  const [pending, setPending] = createSignal(false);
  mocks.pending = pending;
  mocks.setPending = setPending;
  mocks.mutation = {
    get isPending() {
      return mocks.pending();
    },
    mutateAsync: mocks.mutateAsync,
  };
  mocks.team = {
    data: {
      members: [
        { user_id: 'macro|lead@example.com' },
        { user_id: 'macro|second@example.com' },
      ],
    },
  };
});
afterEach(cleanup);

describe('ProjectOperationsEditor', () => {
  it('prepopulates editable values and shows current-team display names without raw ids', () => {
    const view = renderEditor();
    expect(
      view.getByRole('heading', { name: 'Edit project details' })
    ).toBeTruthy();
    expect((view.getByLabelText('Status') as HTMLSelectElement).value).toBe(
      'active'
    );
    expect((view.getByLabelText('Priority') as HTMLSelectElement).value).toBe(
      'high'
    );
    expect(view.getByText('Lead Name')).toBeTruthy();
    expect(view.container.textContent).not.toContain('macro|lead@example.com');
  });

  it('sends the exact full replacement, preserving the original policy', async () => {
    mocks.mutateAsync.mockResolvedValue(undefined);
    const view = renderEditor();
    fireEvent.change(view.getByLabelText('Status'), {
      target: { value: 'paused' },
    });
    fireEvent.change(view.getByLabelText('Priority'), {
      target: { value: 'urgent' },
    });
    fireEvent.change(view.getByLabelText('Lead'), { target: { value: '' } });
    fireEvent.input(view.getByLabelText('Start date'), {
      target: { value: '' },
    });
    fireEvent.input(view.getByLabelText('Target date'), {
      target: { value: '2026-03-04' },
    });
    fireEvent.submit(
      view.getByRole('button', { name: 'Save' }).closest('form')!
    );
    await waitFor(() => expect(mocks.mutateAsync).toHaveBeenCalledOnce());
    expect(mocks.mutateAsync).toHaveBeenCalledWith({
      projectId: 'project-1',
      request: {
        expectedUpdatedAt: operations.updatedAt,
        status: 'paused',
        priority: 'urgent',
        leadUserId: null,
        startDate: null,
        targetDate: '2026-03-04',
        policy: operations.policy,
      },
    });
    expect(view.onClose).toHaveBeenCalledOnce();
  });

  it('blocks inverted dates before making a request', () => {
    const view = renderEditor();
    fireEvent.input(view.getByLabelText('Start date'), {
      target: { value: '2026-03-05' },
    });
    fireEvent.input(view.getByLabelText('Target date'), {
      target: { value: '2026-03-04' },
    });
    fireEvent.submit(
      view.getByRole('button', { name: 'Save' }).closest('form')!
    );
    expect(view.getByRole('alert').textContent).toBe(
      'Start date must be on or before target date.'
    );
    const startDate = view.getByLabelText('Start date');
    const targetDate = view.getByLabelText('Target date');
    for (const input of [startDate, targetDate]) {
      expect(input.getAttribute('aria-invalid')).toBe('true');
      expect(input.getAttribute('aria-describedby')).toBe(
        'project-operation-date-order-error'
      );
    }
    expect(view.getByRole('alert').id).toBe(
      'project-operation-date-order-error'
    );
    expect(mocks.mutateAsync).not.toHaveBeenCalled();
  });

  it('keeps the current lead when the team roster is unavailable', async () => {
    mocks.team = { data: undefined };
    mocks.mutateAsync.mockResolvedValue(undefined);
    const view = renderEditor();
    expect((view.getByLabelText('Lead') as HTMLSelectElement).disabled).toBe(
      true
    );
    fireEvent.submit(
      view.getByRole('button', { name: 'Save' }).closest('form')!
    );
    await waitFor(() => expect(mocks.mutateAsync).toHaveBeenCalledOnce());
    expect(mocks.mutateAsync.mock.calls[0][0].request.leadUserId).toBe(
      'macro|lead@example.com'
    );
  });

  it('refetches and resets after a 409 so retry uses the refreshed version', async () => {
    mocks.mutateAsync
      .mockRejectedValueOnce({ code: 'CONFLICT' })
      .mockResolvedValueOnce(undefined);
    const view = renderEditor();
    mocks.refetch.mockImplementation(async () =>
      view.setCurrent({
        ...operations,
        updatedAt: '2026-02-01T00:00:00.000Z',
        status: 'paused',
      })
    );
    fireEvent.submit(
      view.getByRole('button', { name: 'Save' }).closest('form')!
    );
    await waitFor(() => expect(mocks.refetch).toHaveBeenCalledOnce());
    expect(view.getByRole('alert').textContent).toBe(
      'This project was updated elsewhere. The latest details are now loaded.'
    );
    expect(view.container.textContent).not.toContain('CONFLICT');
    expect(view.container.textContent).not.toContain('macro|lead@example.com');
    expect((view.getByLabelText('Status') as HTMLSelectElement).value).toBe(
      'paused'
    );
    fireEvent.submit(
      view.getByRole('button', { name: 'Save' }).closest('form')!
    );
    await waitFor(() => expect(mocks.mutateAsync).toHaveBeenCalledTimes(2));
    expect(mocks.mutateAsync.mock.calls[1][0].request.expectedUpdatedAt).toBe(
      '2026-02-01T00:00:00.000Z'
    );
  });

  it('shows fixed generic failure copy without closing or exposing backend text', async () => {
    mocks.mutateAsync.mockRejectedValue({
      code: 'FORBIDDEN',
      message: 'secret backend detail',
    });
    const view = renderEditor();
    fireEvent.submit(
      view.getByRole('button', { name: 'Save' }).closest('form')!
    );
    await waitFor(() =>
      expect(view.getByRole('alert').textContent).toBe(
        'Unable to save project details. Please try again.'
      )
    );
    expect(view.container.textContent).not.toContain('secret backend detail');
    expect(view.onClose).not.toHaveBeenCalled();
  });

  it('disables every form path after the first pending save and prevents a duplicate', async () => {
    mocks.mutateAsync.mockImplementation(() => {
      mocks.setPending(true);
      return new Promise(() => {});
    });
    const view = renderEditor();
    const form = view.getByRole('button', { name: 'Save' }).closest('form')!;
    fireEvent.submit(form);
    await waitFor(() => expect(mocks.mutateAsync).toHaveBeenCalledOnce());
    await waitFor(() => expect(form.getAttribute('aria-busy')).toBe('true'));
    for (const label of [
      'Status',
      'Priority',
      'Lead',
      'Start date',
      'Target date',
    ]) {
      expect((view.getByLabelText(label) as HTMLInputElement).disabled).toBe(
        true
      );
    }
    expect(
      (view.getByRole('button', { name: 'Cancel' }) as HTMLButtonElement)
        .disabled
    ).toBe(true);
    expect(
      (
        view.getByRole('button', {
          name: 'Saving project details',
        }) as HTMLButtonElement
      ).disabled
    ).toBe(true);
    fireEvent.submit(
      view
        .getByRole('button', { name: 'Saving project details' })
        .closest('form')!
    );
    expect(mocks.mutateAsync).toHaveBeenCalledOnce();
  });

  it('keeps explicit null and omitted policy semantics intact', async () => {
    mocks.mutateAsync.mockResolvedValue(undefined);
    const nullPolicy = renderEditor({ ...operations, policy: null });
    fireEvent.submit(
      nullPolicy.getByRole('button', { name: 'Save' }).closest('form')!
    );
    await waitFor(() => expect(mocks.mutateAsync).toHaveBeenCalledOnce());
    expect(mocks.mutateAsync.mock.calls[0][0].request.policy).toBeNull();
    nullPolicy.unmount();

    mocks.mutateAsync.mockReset();
    const missingPolicy = renderEditor({ ...operations, policy: undefined });
    fireEvent.submit(
      missingPolicy.getByRole('button', { name: 'Save' }).closest('form')!
    );
    await waitFor(() => expect(mocks.mutateAsync).toHaveBeenCalledOnce());
    expect('policy' in mocks.mutateAsync.mock.calls[0][0].request).toBe(true);
    expect(mocks.mutateAsync.mock.calls[0][0].request.policy).toBeUndefined();
  });
});
