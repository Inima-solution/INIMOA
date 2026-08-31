/** @vitest-environment jsdom */

import type { DateProperty } from '@property/types';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SYSTEM_PROPERTY_IDS } from '../../constants';
import { PropertyRootContext, type PropertySaveFn } from '../../core/context';
import { DateEditor } from './DateEditor';

const selectedDate = new Date('2026-09-01T12:00:00.000Z');

vi.hoisted(() => {
  vi.stubGlobal(
    'WebSocket',
    class {
      close() {}
      send() {}
      addEventListener() {}
      removeEventListener() {}
    }
  );
});

vi.mock('../selectors/PropertyDateSelector', () => ({
  PropertyDateSelector: (props: {
    onSelectDate: (date: Date | null) => void;
  }) => (
    <>
      <button type="button" onClick={() => props.onSelectDate(selectedDate)}>
        select date
      </button>
      <button type="button" onClick={() => props.onSelectDate(null)}>
        clear date
      </button>
    </>
  ),
}));

vi.mock('./EditorPopover', () => ({
  EditorPopover: (props: { children: unknown }) => <>{props.children}</>,
}));

const dateProperty: DateProperty = {
  propertyId: 'task-start-date-row',
  propertyDefinitionId: SYSTEM_PROPERTY_IDS.START_DATE,
  displayName: 'Start Date',
  valueType: 'DATE',
  value: new Date('2026-08-31T12:00:00.000Z'),
  isMultiSelect: false,
  isMetadata: false,
  owner: { scope: 'system' },
  createdAt: new Date('2026-08-01T00:00:00.000Z'),
  updatedAt: new Date('2026-08-31T00:00:00.000Z'),
};

function renderEditor(onSave: PropertySaveFn, onRefresh = vi.fn()) {
  const [editorOpen] = createSignal(true);
  return {
    onRefresh,
    ...render(() => (
      <PropertyRootContext.Provider
        value={{
          property: () => dateProperty,
          canEdit: () => true,
          onSave,
          onRefresh,
          editorOpen,
          openEditor: () => undefined,
          closeEditor: () => undefined,
        }}
      >
        <DateEditor />
      </PropertyRootContext.Provider>
    )),
  };
}

afterEach(cleanup);

describe('DateEditor', () => {
  it('saves the selected Date exactly once and refreshes after a successful save', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const onRefresh = vi.fn();
    renderEditor(onSave, onRefresh);

    await fireEvent.click(screen.getByRole('button', { name: 'select date' }));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledTimes(1);
    });
    expect(onSave).toHaveBeenCalledWith(dateProperty, {
      valueType: 'DATE',
      value: selectedDate,
    });
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it('saves a clear as a DATE null value and refreshes after success', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const onRefresh = vi.fn();
    renderEditor(onSave, onRefresh);

    await fireEvent.click(screen.getByRole('button', { name: 'clear date' }));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledTimes(1);
    });
    expect(onSave).toHaveBeenCalledWith(dateProperty, {
      valueType: 'DATE',
      value: null,
    });
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it('catches rejected saves without refreshing or reporting a false success', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('save failed'));
    const onRefresh = vi.fn();
    renderEditor(onSave, onRefresh);

    await fireEvent.click(screen.getByRole('button', { name: 'select date' }));

    await waitFor(() => {
      expect(onSave).toHaveBeenCalledTimes(1);
    });
    await Promise.resolve();
    expect(onRefresh).not.toHaveBeenCalled();
  });
});
