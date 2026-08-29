import { SYSTEM_PROPERTY_IDS } from '@property/constants';
import type { Property, PropertyApiValues } from '@property/types';
import { createSignal, type JSX } from 'solid-js';
import { render } from 'solid-js/web';
import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest';

const useEntityPropertiesMock = vi.hoisted(() => vi.fn());
const useBulkSaveEntityPropertiesMutationMock = vi.hoisted(() => vi.fn());
const useAllPropertiesMock = vi.hoisted(() => vi.fn());
const useTagsQueryMock = vi.hoisted(() => vi.fn());
const openPropertyEditorMock = vi.hoisted(() => vi.fn());
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

vi.mock('@components/app/side-panel/SidePanel', () => ({
  SidePanel: {
    Grid: (props: { children: JSX.Element }) => <div>{props.children}</div>,
    Loading: () => <div>Loading</div>,
    pillClass: '',
  },
}));

vi.mock('@core/auth', () => ({ useIsAuthenticated: () => () => true }));
vi.mock('@core/component/DocumentPreview', () => ({
  PopupPreview: () => null,
}));
vi.mock('@core/component/HoverCard', () => ({ HoverCard: () => null }));
vi.mock('@core/component/LexicalMarkdown/component/core/BlockLink', () => ({
  openDocument: vi.fn(),
}));
vi.mock('@core/constant/allBlocks', () => ({ itemToBlockName: () => 'Task' }));
vi.mock('@core/util/useSplitNavigationHandler', () => ({
  useSplitNavigationHandler: () => vi.fn(),
}));
vi.mock('@property/component/modal', () => ({ Modals: () => null }));
vi.mock('@property/component/propertyValue/PropertyValueIcon', () => ({
  PropertyValueIcon: () => null,
}));
vi.mock('@property', async () => {
  const { createSignal } = await import('solid-js');
  const { PropertyRootContext, useProperty } = await import(
    '@property/core/context'
  );
  const { InlineBooleanEditor } = await import(
    '@property/editors/inline/InlineBooleanEditor'
  );
  const Root = (props: {
    property: Property;
    canEdit?: boolean;
    children: JSX.Element;
    onRefresh?: () => void;
    onSave?: (property: Property, value: PropertyApiValues) => Promise<void>;
  }) => {
    const [editorOpen] = createSignal(false);
    return (
      <PropertyRootContext.Provider
        value={{
          property: () => props.property,
          canEdit: () => props.canEdit ?? false,
          onSave: props.onSave,
          onRefresh: props.onRefresh,
          editorOpen,
          openEditor: () => undefined,
          closeEditor: () => undefined,
        }}
      >
        <div data-property-id={props.property.propertyId}>{props.children}</div>
      </PropertyRootContext.Provider>
    );
  };
  return {
    Property: {
      Root,
      Display: InlineBooleanEditor,
      InlineEditor: InlineBooleanEditor,
      PopoverEditor: () => null,
    },
    useProperty,
  };
});
vi.mock('@property/editor/hooks/useAllProperties', () => ({
  useAllProperties: useAllPropertiesMock,
}));
vi.mock('@property/editor/state/propertyEditor', () => ({
  openPropertyEditor: openPropertyEditorMock,
}));
vi.mock('@property/hooks', () => ({
  useEntityProperties: useEntityPropertiesMock,
  usePropertyEntityDisplay: () => ({ icon: () => null, name: () => '' }),
}));
vi.mock('@property/tags', () => ({
  isTaggableEntityType: () => false,
  TagsRow: () => null,
}));
vi.mock('@queries/properties/entity', () => ({
  useEntityProperties: useEntityPropertiesMock,
  useBulkSaveEntityPropertiesMutation: useBulkSaveEntityPropertiesMutationMock,
}));
vi.mock('@queries/preview', () => ({
  isAccessiblePreviewItem: () => false,
  useItemPreview: () => ({ item: () => undefined }),
}));
vi.mock('@queries/properties/tags', () => ({ useTagsQuery: useTagsQueryMock }));
vi.mock('@ui', () => ({
  Button: (props: { children: JSX.Element }) => (
    <button>{props.children}</button>
  ),
  cn: (...classes: Array<string | undefined>) =>
    classes.filter(Boolean).join(' '),
  Dropdown: (props: { children: JSX.Element }) => <>{props.children}</>,
  Layer: (props: { children: JSX.Element }) => <>{props.children}</>,
}));

import { EntityPropertiesSection } from './EntityPropertiesSection';

const milestone = (value: boolean | null): Property => ({
  propertyId: 'milestone-row-1',
  propertyDefinitionId: SYSTEM_PROPERTY_IDS.MILESTONE,
  displayName: 'Milestone',
  valueType: 'BOOLEAN',
  value,
  isMultiSelect: false,
  isMetadata: false,
  isSystemProperty: true,
  owner: { scope: 'system' },
  createdAt: new Date('2026-08-30T00:00:00.000Z'),
  updatedAt: new Date('2026-08-30T00:00:00.000Z'),
});

let dispose: (() => void) | undefined;

function renderSection(options: {
  canEdit?: boolean;
  property?: Property;
  onPinned?: (propertyId: string) => void;
}) {
  const [properties] = createSignal(options.property ? [options.property] : []);
  const addProperty = vi.fn();
  const removeProperty = vi.fn();
  const refetch = vi.fn();
  useEntityPropertiesMock.mockReturnValue({
    properties,
    isLoading: () => false,
    error: () => undefined,
    refetch,
    addProperty,
    removeProperty,
  });

  const container = document.createElement('div');
  document.body.appendChild(container);
  dispose = render(
    () => (
      <EntityPropertiesSection
        entityId="task-1"
        entityType="TASK"
        canEdit={options.canEdit ?? true}
        defaultPinnedPropertyIds={() => [SYSTEM_PROPERTY_IDS.MILESTONE]}
        onPropertyPinned={options.onPinned}
      />
    ),
    container
  );
  return { addProperty, container, refetch, removeProperty };
}

describe('EntityPropertiesSection required Task Milestone', () => {
  beforeEach(() => {
    useAllPropertiesMock.mockReturnValue(() => []);
    useTagsQueryMock.mockReturnValue({ data: [] });
    useBulkSaveEntityPropertiesMutationMock.mockReturnValue({
      mutateAsync: vi.fn().mockResolvedValue(undefined),
    });
  });

  afterEach(() => {
    dispose?.();
    dispose = undefined;
    document.body.replaceChildren();
    vi.clearAllMocks();
  });

  it('pins the existing Milestone row selected from Add property without attaching it again', async () => {
    const onPinned = vi.fn();
    const { addProperty, container, removeProperty } = renderSection({
      property: milestone(false),
      onPinned,
    });

    (
      [...container.querySelectorAll('button')].find((button) =>
        button.textContent?.includes('Add property')
      ) as HTMLButtonElement
    ).click();
    await openPropertyEditorMock.mock.calls[0][3].onPropertyAdded([
      SYSTEM_PROPERTY_IDS.MILESTONE,
    ]);

    expect(onPinned).toHaveBeenCalledWith('milestone-row-1');
    expect(addProperty).not.toHaveBeenCalled();
    expect(removeProperty).not.toHaveBeenCalled();
  });

  it.each([
    [null, true],
    [false, true],
    [true, false],
  ] as const)(
    'writes Milestone %s as %s through the shared Task Boolean mutation',
    async (current, next) => {
      const mutateAsync = vi.fn().mockResolvedValue(undefined);
      useBulkSaveEntityPropertiesMutationMock.mockReturnValue({ mutateAsync });
      const { container } = renderSection({ property: milestone(current) });

      (
        container.querySelector(
          '[data-property-id="milestone-row-1"] button'
        ) as HTMLButtonElement
      ).click();

      await vi.waitFor(() =>
        expect(mutateAsync).toHaveBeenCalledWith({
          properties: [
            {
              entityId: 'task-1',
              entityType: 'TASK',
              property: milestone(current),
              apiValues: { valueType: 'BOOLEAN', value: next },
            },
          ],
        })
      );
    }
  );

  it('hides Add property and disables the Milestone Boolean editor when editing is forbidden', () => {
    const mutateAsync = vi.fn();
    useBulkSaveEntityPropertiesMutationMock.mockReturnValue({
      mutateAsync,
    });
    const { container } = renderSection({
      canEdit: false,
      property: milestone(false),
    });

    expect(container.textContent).not.toContain('Add property');
    expect(
      container.querySelector('[data-property-id="milestone-row-1"] button')
    ).toHaveProperty('disabled', true);
    (
      container.querySelector(
        '[data-property-id="milestone-row-1"] button'
      ) as HTMLButtonElement
    ).click();
    expect(mutateAsync).not.toHaveBeenCalled();
  });

  it('submits only once while the Boolean save is pending and refreshes after success', async () => {
    let resolve!: () => void;
    const mutateAsync = vi.fn(
      () =>
        new Promise<void>((done) => {
          resolve = done;
        })
    );
    useBulkSaveEntityPropertiesMutationMock.mockReturnValue({ mutateAsync });
    const { container, refetch } = renderSection({
      property: milestone(false),
    });
    const button = container.querySelector(
      '[data-property-id="milestone-row-1"] button'
    ) as HTMLButtonElement;

    button.click();
    button.click();
    await vi.waitFor(() => expect(mutateAsync).toHaveBeenCalledOnce());
    expect(refetch).not.toHaveBeenCalled();

    resolve();
    await vi.waitFor(() => expect(refetch).toHaveBeenCalledOnce());
  });

  it('keeps the authoritative Boolean state and skips refresh after a permanent save failure', async () => {
    const mutateAsync = vi
      .fn()
      .mockRejectedValue(new Error('invalid property'));
    useBulkSaveEntityPropertiesMutationMock.mockReturnValue({ mutateAsync });
    const { container, refetch, removeProperty } = renderSection({
      property: milestone(false),
    });
    const button = container.querySelector(
      '[data-property-id="milestone-row-1"] button'
    ) as HTMLButtonElement;

    expect(button.querySelector('svg')).toBeNull();
    button.click();
    await vi.waitFor(() => expect(mutateAsync).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(button.disabled).toBe(false));

    expect(button.querySelector('svg')).toBeNull();
    expect(refetch).not.toHaveBeenCalled();
    expect(removeProperty).not.toHaveBeenCalled();
  });
});

afterAll(() => {
  vi.unstubAllGlobals();
});
