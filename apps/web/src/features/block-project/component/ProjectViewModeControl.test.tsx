/** @vitest-environment jsdom */

import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProjectViewModeControl } from './ProjectViewModeControl';

afterEach(cleanup);

describe('ProjectViewModeControl', () => {
  it('is controlled and exposes the project task view label', () => {
    const onChange = vi.fn();
    const view = render(() => (
      <ProjectViewModeControl mode="list" onChange={onChange} />
    ));

    expect(
      view.getByRole('radiogroup', { name: 'Project task view' })
    ).toBeTruthy();
    expect(
      view.getByRole('radio', { name: 'List' }).getAttribute('data-checked')
    ).not.toBeNull();

    fireEvent.click(view.getByRole('radio', { name: 'Board' }));
    expect(onChange).toHaveBeenCalledWith('board');
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
        mode="board"
        density="touch"
        onChange={() => {}}
      />
    ));
    expect(
      touch.container.firstElementChild?.classList.contains('**:min-h-11')
    ).toBe(true);
  });
});
