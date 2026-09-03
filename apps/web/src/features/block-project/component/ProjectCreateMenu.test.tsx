/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, waitFor } from '@solidjs/testing-library';
import type { JSX, ParentProps } from 'solid-js';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  createDecision: vi.fn(),
  insertSplit: vi.fn(),
  replaceSplit: vi.fn(),
  splitGoBack: vi.fn(),
  splitReplace: vi.fn(),
  toastFailure: vi.fn(),
}));

vi.mock('@components/app/split-layout/layout', () => ({
  useSplitLayout: () => ({
    insertSplit: mocks.insertSplit,
    replaceSplit: mocks.replaceSplit,
  }),
}));

vi.mock('@core/component/EntityIcon', () => ({
  EntityIcon: (props: { targetType: string }) => (
    <span data-icon={props.targetType} />
  ),
}));

vi.mock('@core/component/Toast/Toast', () => ({
  toast: { failure: mocks.toastFailure },
}));

vi.mock('@core/hotkey/state', () => ({
  pressedKeys: () => new Set<string>(),
}));

vi.mock('@core/util/create', () => ({
  createCanvasFileFromJsonString: vi.fn(),
  createChat: vi.fn(),
  createDecision: mocks.createDecision,
  createMarkdownFile: vi.fn(),
  createTask: vi.fn(),
}));

vi.mock('@queries/storage/projects', () => ({
  createProject: vi.fn(),
}));

vi.mock('@ui', () => {
  const Passthrough = (props: ParentProps) => <>{props.children}</>;
  const Dropdown = Object.assign(Passthrough, {
    Trigger: (props: JSX.ButtonHTMLAttributes<HTMLButtonElement>) => (
      <button type="button" {...props} />
    ),
    Content: Passthrough,
    Group: Passthrough,
    Item: (
      props: ParentProps<{
        onSelect?: () => void;
      }>
    ) => (
      <button type="button" onClick={props.onSelect}>
        {props.children}
      </button>
    ),
  });
  const Dialog = Object.assign(Passthrough, {
    Title: Passthrough,
  });
  return { Dialog, Dropdown, Surface: Passthrough };
});

import { ProjectCreateMenu } from './ProjectCreateMenu';

beforeEach(() => {
  mocks.createDecision.mockReset();
  mocks.insertSplit.mockReset();
  mocks.replaceSplit.mockReset();
  mocks.splitGoBack.mockReset();
  mocks.splitReplace.mockReset();
  mocks.toastFailure.mockReset();
  mocks.replaceSplit.mockReturnValue({
    goBack: mocks.splitGoBack,
    replace: mocks.splitReplace,
  });
});

afterEach(cleanup);

it('creates a Decision in the current project and opens its Markdown block', async () => {
  mocks.createDecision.mockResolvedValue('decision-1');
  const view = render(() => <ProjectCreateMenu id="project-1" />);

  fireEvent.click(view.getByRole('button', { name: 'Decision' }));

  await waitFor(() => {
    expect(mocks.createDecision).toHaveBeenCalledWith({
      title: '',
      content: '',
      projectId: 'project-1',
      source: 'project-create-menu',
    });
    expect(mocks.splitReplace).toHaveBeenCalledWith({
      next: { type: 'decision', id: 'decision-1', params: undefined },
      mergeHistory: true,
      referredFrom: 'entity-actions-menu',
    });
  });
  expect(mocks.replaceSplit).toHaveBeenCalledWith({
    content: { type: 'component', id: 'loading' },
    referredFrom: 'entity-actions-menu',
  });
  expect(mocks.insertSplit).not.toHaveBeenCalled();
  expect(mocks.splitGoBack).not.toHaveBeenCalled();
  expect(mocks.toastFailure).not.toHaveBeenCalled();
});

it.each([
  ['an empty result', undefined, 'Failed to create decision'],
  ['a rejected request', new Error('network down'), 'network down'],
] as const)(
  'closes the loading split after %s without opening a Decision',
  async (_label, outcome, message) => {
    if (outcome instanceof Error) {
      mocks.createDecision.mockRejectedValue(outcome);
    } else {
      mocks.createDecision.mockResolvedValue(outcome);
    }
    const view = render(() => <ProjectCreateMenu id="project-1" />);

    fireEvent.click(view.getByRole('button', { name: 'Decision' }));

    await waitFor(() => {
      expect(mocks.splitGoBack).toHaveBeenCalledTimes(1);
    });
    expect(mocks.splitReplace).not.toHaveBeenCalled();
    expect(mocks.toastFailure).toHaveBeenCalledWith(message);
  }
);
