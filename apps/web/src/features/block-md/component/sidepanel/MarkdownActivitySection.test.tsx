/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import { afterEach, expect, it, vi } from 'vitest';

const activity = vi.hoisted(() => vi.fn(() => null));

vi.mock('@app/features/activity/EntityActivitySection', () => ({
  EntityActivitySectionConditional: activity,
}));

import { MarkdownActivitySection } from './MarkdownActivitySection';

afterEach(() => {
  activity.mockClear();
  cleanup();
});

it('projects a Decision opened as Markdown through exact DOCUMENT activity', () => {
  render(() => (
    <MarkdownActivitySection
      blockId="decision-1"
      blockName="decision"
      order={40}
    />
  ));

  expect(activity).toHaveBeenCalledWith({
    entityId: 'decision-1',
    entityType: 'DOCUMENT',
    order: 40,
  });
});

it('keeps Task activity on the TASK property target', () => {
  render(() => <MarkdownActivitySection blockId="task-1" blockName="task" />);

  expect(activity).toHaveBeenCalledWith(
    expect.objectContaining({ entityId: 'task-1', entityType: 'TASK' })
  );
});
