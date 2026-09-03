/** @vitest-environment jsdom */

import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProjectViewModeControl } from './ProjectViewModeControl';

afterEach(cleanup);

describe('ProjectViewModeControl', () => {
  it('is controlled and exposes every project view', () => {
    const onChange = vi.fn();
    const view = render(() => (
      <ProjectViewModeControl mode="list" onChange={onChange} />
    ));

    expect(view.getByRole('radiogroup', { name: 'Project view' })).toBeTruthy();
    expect(
      view.getByRole('radio', { name: 'List' }).getAttribute('data-checked')
    ).not.toBeNull();

    fireEvent.click(view.getByRole('radio', { name: 'Board' }));
    expect(onChange).toHaveBeenCalledWith('board');
    fireEvent.click(view.getByRole('radio', { name: 'Timeline' }));
    expect(onChange).toHaveBeenCalledWith('timeline');
    fireEvent.click(view.getByRole('radio', { name: 'Decisions' }));
    expect(onChange).toHaveBeenCalledWith('decisions');
  });

  it('uses dense desktop and touch-sized control targets', () => {
    const desktop = render(() => (
      <ProjectViewModeControl mode="list" onChange={() => {}} />
    ));
    expect(
      desktop.container.firstElementChild?.classList.contains('**:min-h-10')
    ).toBe(true);
    expect(
      desktop.container.firstElementChild?.classList.contains(
        'touch:**:min-h-11'
      )
    ).toBe(true);
    desktop.unmount();

    const touch = render(() => (
      <ProjectViewModeControl
        mode="decisions"
        density="touch"
        onChange={() => {}}
      />
    ));
    expect(
      touch.container.firstElementChild?.classList.contains('**:min-h-11')
    ).toBe(true);
    expect(
      touch
        .getByRole('radio', { name: 'Decisions' })
        .getAttribute('data-checked')
    ).not.toBeNull();
  });
});
