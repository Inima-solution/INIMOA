/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { type Component, For, type ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  chatEntities: [] as Array<{ id: string; name: string; type: string }>,
  chatAction: vi.fn(),
  clipboardWriteText: vi.fn(),
  toastSuccess: vi.fn(),
  isMobile: false,
  openShare: vi.fn(),
  projectId: 'project-id',
  projectName: 'Project Atlas',
  sharingEnabled: true,
  mobileTools: [] as Array<{ action?: () => void; label: string }>,
  toolbarTools: [] as string[],
}));

vi.mock('@app/features/chat/ChatWithAgentButton', () => ({
  ChatWithAgentButton: (props: {
    entity: { id: string; name: string; type: string };
  }) => {
    mocks.chatEntities.push(props.entity);
    return <button data-testid="project-chat">Chat</button>;
  },
  ChatWithAgentIcon: () => null,
  openChatWithAgent: mocks.chatAction,
}));
vi.mock('@block-project/isSpecial', () => ({
  getIsSpecialProject: (id: string) =>
    id === 'root' || id === 'trash' || id === 'special-project',
}));
vi.mock('@block-project/signal/projectBlockData', () => ({
  projectBlockDataSignal: () => ({
    projectMetadata: { name: mocks.projectName, userId: 'owner-id' },
  }),
}));
vi.mock('@components/app/ResponsiveBlockToolbar', () => ({
  ResponsivePermissionsBadge: () => null,
  ToolButton: (props: { tool: { label: string } }) => {
    mocks.toolbarTools.push(props.tool.label);
    return <button>{props.tool.label}</button>;
  },
}));
vi.mock('@components/app/split-layout/components/PreviewButton', () => ({
  PreviewButton: () => null,
}));
vi.mock('@components/app/split-layout/components/SplitDrawerContext', () => ({
  useDrawerControl: () => ({ toggle: vi.fn() }),
}));
vi.mock('@components/app/split-layout/components/SplitFileMenu', () => ({
  SplitFileMenu: (props: {
    tools?: Array<{
      action?: () => void;
      buttonComponent?: Component;
      condition?: () => boolean;
      label: string;
    }>;
  }) => {
    const eligibleTools = (props.tools ?? []).filter(
      (tool) => !tool.condition || tool.condition()
    );
    mocks.mobileTools.push(...eligibleTools);
    for (const tool of eligibleTools) mocks.toolbarTools.push(tool.label);
    return (
      <For each={eligibleTools}>
        {(tool) =>
          tool.buttonComponent ? (
            tool.buttonComponent({})
          ) : (
            <button onClick={tool.action}>{tool.label}</button>
          )
        }
      </For>
    );
  },
}));
vi.mock('@components/app/split-layout/components/SplitHeader', () => ({
  SplitHeaderLeft: (props: ParentProps) => <>{props.children}</>,
  SplitHeaderRight: (props: ParentProps) => <>{props.children}</>,
}));
vi.mock('@components/app/split-layout/components/SplitLabel', () => ({
  BlockItemSplitLabel: () => null,
  SplitTitleFileMenu: (props: ParentProps) => <>{props.children}</>,
}));
vi.mock('@components/app/split-layout/components/SplitToolbar', () => ({
  SplitToolbarLeft: (props: ParentProps) => <>{props.children}</>,
  SplitToolbarRight: (props: ParentProps) => <>{props.children}</>,
}));
vi.mock('@core/block', () => ({ useBlockId: () => mocks.projectId }));
vi.mock('@core/component/DetailsDrawer', () => ({
  DETAILS_DRAWER_ID: 'details',
}));
vi.mock('@core/component/Toast/Toast', () => ({
  toast: { success: mocks.toastSuccess },
}));
vi.mock('@core/component/TopBar/ShareButton', () => ({
  getShareDrawerRecipientInput: () => null,
  ShareTrigger: (props: { copyLink: () => void }) => (
    <>
      <button data-testid="project-share" onClick={() => mocks.openShare()}>
        Share
      </button>
      <button data-testid="project-copy-link" onClick={() => props.copyLink()}>
        Copy Link
      </button>
    </>
  ),
  useShareDialogContext: () => ({ open: mocks.openShare }),
}));
vi.mock('@core/constant/featureFlags', () => ({
  get ENABLE_PROJECT_SHARING() {
    return mocks.sharingEnabled;
  },
}));
vi.mock('@core/mobile/isMobile', () => ({ isMobile: () => mocks.isMobile }));
vi.mock('@core/signal/permissions', () => ({
  useCanEdit: () => () => true,
  useIsDocumentOwner: () => () => true,
}));
vi.mock('@core/util/url', () => ({
  buildSimpleEntityUrl: ({ id, type }: { id: string; type: string }) =>
    `https://macro.test/${type}/${id}`,
}));
vi.mock('@icon/wide-share.svg', () => ({ default: () => null }));
vi.mock('@phosphor/info.svg', () => ({ default: () => null }));
vi.mock('./ProjectCreateMenu', () => ({
  ProjectCreateMenu: () => null,
  useProjectCreateTools: () => ({ CreateDialog: () => null, tools: [] }),
}));
vi.mock('./ProjectViewModeControl', () => ({
  ProjectViewModeControl: () => null,
}));

import { TopBar } from './TopBar';

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.chatEntities = [];
  mocks.chatAction.mockReset();
  mocks.clipboardWriteText.mockReset();
  mocks.isMobile = false;
  mocks.projectId = 'project-id';
  mocks.projectName = 'Project Atlas';
  mocks.sharingEnabled = true;
  mocks.mobileTools = [];
  mocks.toastSuccess.mockReset();
  mocks.toolbarTools = [];
  Object.assign(navigator, {
    clipboard: { writeText: mocks.clipboardWriteText },
  });
});

function renderTopBar() {
  return render(() => (
    <TopBar mode="list" onChange={() => {}} selectorVisible={true} />
  ));
}

describe('project TopBar sharing and agent surfaces', () => {
  it.each([false, true])(
    'exposes exactly one Share trigger and canonical copy behavior on %s',
    (mobile) => {
      mocks.isMobile = mobile;
      renderTopBar();

      expect(screen.getAllByTestId('project-share')).toHaveLength(1);
      expect(mocks.toolbarTools).not.toContain('Share');
      expect(mocks.mobileTools.map((tool) => tool.label)).not.toContain(
        'Share'
      );
      fireEvent.click(screen.getByTestId('project-share'));
      expect(mocks.openShare).toHaveBeenCalledTimes(1);
      fireEvent.click(screen.getByTestId('project-copy-link'));
      expect(mocks.clipboardWriteText).toHaveBeenCalledWith(
        'https://macro.test/project/project-id'
      );
      expect(mocks.clipboardWriteText).toHaveBeenCalledTimes(1);
      expect(mocks.toastSuccess).toHaveBeenCalledWith(
        'Link copied to clipboard'
      );
    }
  );

  it.each(
    ['root', 'trash', 'special-project'].flatMap((id) =>
      [false, true].map((mobile) => [id, mobile] as const)
    )
  )(
    'omits Share and Chat for special project %s on mobile %s',
    (id, mobile) => {
      mocks.projectId = id;
      mocks.isMobile = mobile;
      renderTopBar();

      expect(screen.queryByTestId('project-share')).toBeNull();
      expect(screen.queryByTestId('project-copy-link')).toBeNull();
      expect(screen.queryByTestId('project-chat')).toBeNull();
      expect(mocks.mobileTools.map((tool) => tool.label)).not.toContain('Chat');
    }
  );

  it('omits Share when project sharing is disabled without adding it to the mobile menu', () => {
    mocks.isMobile = true;
    mocks.sharingEnabled = false;
    renderTopBar();

    expect(screen.queryByTestId('project-share')).toBeNull();
    expect(screen.queryByTestId('project-copy-link')).toBeNull();
    expect(mocks.toolbarTools).not.toContain('Share');
  });

  it('passes exact project identity to desktop Chat', () => {
    renderTopBar();

    expect(mocks.chatEntities).toEqual([
      { type: 'project', id: 'project-id', name: 'Project Atlas' },
    ]);
  });

  it('passes exact project identity to the mobile Chat action', () => {
    mocks.isMobile = true;
    renderTopBar();

    const chatTool = mocks.mobileTools.find((tool) => tool.label === 'Chat');
    chatTool?.action?.();

    expect(mocks.chatAction).toHaveBeenCalledWith({
      type: 'project',
      id: 'project-id',
      name: 'Project Atlas',
    });
    expect(chatTool).toMatchObject({ label: 'Chat' });
  });
});
