/** @vitest-environment jsdom */

import { SYSTEM_PROPERTY_IDS } from '@property/constants';
import {
  PropertyRootContext,
  type PropertySaveFn,
} from '@property/core/context';
import type { EntityProperty } from '@property/types';
import type { EntityReference } from '@service-properties/generated/schemas/entityReference';
import type { EntityType } from '@service-properties/generated/schemas/entityType';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { createSignal, type JSX, Show } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EntityEditor } from './EntityEditor';

type SelectorProps = {
  config: {
    isMultiSelect: boolean;
    specificEntityType?: EntityType | null;
    selfFilter?: { entityType: EntityType; blockId?: string };
  };
  selectedOptions: () => Set<string>;
  setSelectedOptions: (
    options: Set<string>,
    entityInfo?: Array<{ id: string; entity_type: string }>
  ) => void;
};

let selectorProps: SelectorProps | undefined;

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

vi.mock('@queries/team/teams', () => ({
  useCurrentTeamQuery: () => ({ data: undefined }),
}));

vi.mock('../selectors/PropertyEntitySelector', () => ({
  PropertyEntitySelector: (props: SelectorProps) => {
    selectorProps = props;
    return (
      <>
        <button
          type="button"
          onClick={() =>
            props.setSelectedOptions(new Set(['task-retained', 'task-added']), [
              { id: 'task-added', entity_type: 'TASK' },
            ])
          }
        >
          replace dependency
        </button>
        <button
          type="button"
          onClick={() => props.setSelectedOptions(new Set())}
        >
          clear dependencies
        </button>
      </>
    );
  },
}));

vi.mock('./EditorPopover', () => ({
  EditorPopover: (props: { children: JSX.Element; onClose?: () => void }) => (
    <>
      {props.children}
      <button type="button" onClick={() => props.onClose?.()}>
        close dependencies
      </button>
    </>
  ),
}));

const dependsOnProperty: EntityProperty = {
  propertyId: 'depends-on-row',
  propertyDefinitionId: SYSTEM_PROPERTY_IDS.DEPENDS_ON,
  displayName: 'Depends On',
  valueType: 'ENTITY',
  value: [
    { entity_id: 'task-retained', entity_type: 'TASK' },
    { entity_id: 'task-removed', entity_type: 'TASK' },
  ],
  isMultiSelect: true,
  isMetadata: false,
  owner: { scope: 'system' },
  specificEntityType: 'TASK',
  createdAt: new Date('2026-08-01T00:00:00.000Z'),
  updatedAt: new Date('2026-08-31T00:00:00.000Z'),
};

function renderEditor(onSave: PropertySaveFn, onRefresh = vi.fn()) {
  const [editorOpen, setEditorOpen] = createSignal(true);
  const closeEditor = vi.fn(() => setEditorOpen(false));

  render(() => (
    <PropertyRootContext.Provider
      value={{
        property: () => dependsOnProperty,
        canEdit: () => true,
        onSave,
        onRefresh,
        editorOpen,
        openEditor: () => undefined,
        closeEditor,
      }}
    >
      <EntityEditor
        selfFilter={{ entityType: 'TASK', blockId: 'task-owner' }}
      />
      <Show when={editorOpen()} fallback={<span>editor closed</span>}>
        <span>editor open</span>
      </Show>
    </PropertyRootContext.Provider>
  ));

  return { closeEditor, onRefresh };
}

afterEach(cleanup);

beforeEach(() => {
  selectorProps = undefined;
});

describe('EntityEditor Depends On', () => {
  it('configures the Task dependency selector and saves retained refs followed by the selected Task', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { closeEditor, onRefresh } = renderEditor(onSave);

    expect(selectorProps?.config).toMatchObject({
      isMultiSelect: true,
      specificEntityType: 'TASK',
      selfFilter: { entityType: 'TASK', blockId: 'task-owner' },
    });
    expect(selectorProps?.selectedOptions()).toEqual(
      new Set(['task-retained', 'task-removed'])
    );

    await fireEvent.click(
      screen.getByRole('button', { name: 'replace dependency' })
    );
    await fireEvent.click(
      screen.getByRole('button', { name: 'close dependencies' })
    );

    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave).toHaveBeenCalledWith(dependsOnProperty, {
      valueType: 'ENTITY',
      refs: [
        { entity_id: 'task-retained', entity_type: 'TASK' },
        { entity_id: 'task-added', entity_type: 'TASK' },
      ] satisfies EntityReference[],
    });
    expect(onRefresh).toHaveBeenCalledOnce();
    expect(closeEditor).toHaveBeenCalledOnce();
    expect(screen.getByText('editor closed')).toBeTruthy();
  });

  it('writes an explicit null ENTITY value when all dependencies are cleared', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { onRefresh } = renderEditor(onSave);

    await fireEvent.click(
      screen.getByRole('button', { name: 'clear dependencies' })
    );
    await fireEvent.click(
      screen.getByRole('button', { name: 'close dependencies' })
    );

    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onSave).toHaveBeenCalledWith(dependsOnProperty, {
      valueType: 'ENTITY',
      refs: null,
    });
    expect(onRefresh).toHaveBeenCalledOnce();
    expect(screen.getByText('editor closed')).toBeTruthy();
  });

  it('closes after a rejected dependency save without refreshing or surfacing backend text', async () => {
    const onSave = vi
      .fn()
      .mockRejectedValue(new Error('409 dependency validation failed'));
    const { closeEditor, onRefresh } = renderEditor(onSave);

    await fireEvent.click(
      screen.getByRole('button', { name: 'replace dependency' })
    );
    await fireEvent.click(
      screen.getByRole('button', { name: 'close dependencies' })
    );

    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    expect(onRefresh).not.toHaveBeenCalled();
    expect(closeEditor).toHaveBeenCalledOnce();
    expect(screen.getByText('editor closed')).toBeTruthy();
    expect(screen.queryByText('409 dependency validation failed')).toBeNull();
  });
});
