/** @vitest-environment jsdom */

import type { Property as PropertyValue } from '@property/types';
import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { Show } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useProperty } from '../core/context';
import { Property as PropertyComponent } from '../property';
import { PropertyEditTrigger } from './PropertyEditTrigger';

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

const textProperty: PropertyValue = {
  propertyId: 'priority',
  propertyDefinitionId: 'priority-definition',
  displayName: 'Priority',
  valueType: 'STRING',
  value: 'High',
  isMultiSelect: false,
  isMetadata: false,
  owner: { scope: 'system' },
  createdAt: new Date('2026-08-01T00:00:00.000Z'),
  updatedAt: new Date('2026-08-31T00:00:00.000Z'),
};

function EditorState() {
  const ctx = useProperty();
  return (
    <Show when={ctx.editorOpen()}>
      <span>editor open</span>
    </Show>
  );
}

function renderTrigger(options: {
  canEdit?: boolean;
  isMetadata?: boolean;
  onEdit?: (property: PropertyValue, anchor?: HTMLElement) => void;
  onClick?: () => void;
}) {
  const property = { ...textProperty, isMetadata: options.isMetadata ?? false };
  return render(() => (
    <PropertyComponent.Root
      property={property}
      canEdit={options.canEdit ?? true}
      onEdit={options.onEdit}
    >
      <PropertyEditTrigger onClick={options.onClick}>
        edit priority
      </PropertyEditTrigger>
      <EditorState />
    </PropertyComponent.Root>
  ));
}

afterEach(cleanup);

describe('PropertyEditTrigger', () => {
  it.each([
    ['cannot edit', { canEdit: false }],
    ['is metadata', { isMetadata: true }],
  ])(
    '%s does not route the click to an editor or external callbacks',
    async (_, options) => {
      const onEdit = vi.fn();
      const onClick = vi.fn();
      renderTrigger({ ...options, onEdit, onClick });

      await fireEvent.click(
        screen.getByRole('button', { name: 'edit priority' })
      );

      expect(onEdit).not.toHaveBeenCalled();
      expect(onClick).not.toHaveBeenCalled();
      expect(screen.queryByText('editor open')).toBeNull();
    }
  );

  it('opens the local editor when editable and no external editor is supplied', async () => {
    renderTrigger({});

    await fireEvent.click(
      screen.getByRole('button', { name: 'edit priority' })
    );

    expect(screen.getByText('editor open')).toBeTruthy();
  });

  it('forwards an editable click exactly once to the external editor with its button anchor', async () => {
    const onEdit = vi.fn();
    const onClick = vi.fn();
    renderTrigger({ onEdit, onClick });
    const button = screen.getByRole('button', { name: 'edit priority' });

    await fireEvent.click(button);

    expect(onEdit).toHaveBeenCalledTimes(1);
    expect(onEdit).toHaveBeenCalledWith(textProperty, button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
