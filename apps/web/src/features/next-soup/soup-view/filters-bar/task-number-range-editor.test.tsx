/** @vitest-environment jsdom */

import { fireEvent, render } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';
import { TaskNumberRangeEditor } from './task-number-range-editor';

describe('TaskNumberRangeEditor', () => {
  it('blocks empty, non-finite, and inverted input before applying', () => {
    const onApply = vi.fn();
    const view = render(() => (
      <TaskNumberRangeEditor
        label="Estimate"
        onApply={onApply}
        onClear={() => {}}
      />
    ));
    fireEvent.click(view.getByRole('button', { name: 'Apply' }));
    expect(onApply).not.toHaveBeenCalled();
    expect(view.getByRole('alert').textContent).toMatch(/lower or upper/);

    fireEvent.input(view.getByLabelText('Estimate lower bound'), {
      target: { value: 'Infinity' },
    });
    fireEvent.click(view.getByRole('button', { name: 'Apply' }));
    expect(onApply).not.toHaveBeenCalled();
    expect(view.getByRole('alert').textContent).toMatch(/finite/);

    fireEvent.input(view.getByLabelText('Estimate lower bound'), {
      target: { value: '4' },
    });
    fireEvent.input(view.getByLabelText('Estimate upper bound'), {
      target: { value: '2' },
    });
    fireEvent.click(view.getByRole('button', { name: 'Apply' }));
    expect(onApply).not.toHaveBeenCalled();
    expect(view.getByRole('alert').textContent).toMatch(/must not exceed/);
  });

  it('applies explicit operators and exclusion, and delegates clear', () => {
    const onApply = vi.fn();
    const onClear = vi.fn();
    const view = render(() => (
      <TaskNumberRangeEditor
        label="Estimate"
        onApply={onApply}
        onClear={onClear}
      />
    ));
    fireEvent.change(view.getByLabelText('Estimate lower operator'), {
      target: { value: 'gt' },
    });
    fireEvent.input(view.getByLabelText('Estimate lower bound'), {
      target: { value: '-0' },
    });
    fireEvent.input(view.getByLabelText('Estimate upper bound'), {
      target: { value: '10' },
    });
    fireEvent.click(view.getByLabelText('Exclude matching tasks'));
    fireEvent.click(view.getByRole('button', { name: 'Apply' }));
    expect(onApply).toHaveBeenCalledWith({ gt: 0, lte: 10 }, true);
    fireEvent.click(view.getByRole('button', { name: 'Clear' }));
    expect(onClear).toHaveBeenCalledOnce();
  });
});
