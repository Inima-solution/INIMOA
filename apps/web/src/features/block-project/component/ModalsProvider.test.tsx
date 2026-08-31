/** @vitest-environment jsdom */

import { cleanup, render } from '@solidjs/testing-library';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  modalProps: [] as Array<{ name?: string; owner?: string }>,
  projectId: 'project-id',
  sharingEnabled: true,
}));

vi.mock('@block-project/isSpecial', () => ({
  getIsSpecialProject: (id: string) =>
    id === 'root' || id === 'trash' || id === 'special-project',
}));
vi.mock('@block-project/signal/projectBlockData', () => ({
  projectBlockDataSignal: () => ({
    projectMetadata: { name: 'Project Atlas', userId: 'owner-id' },
  }),
}));
vi.mock('@core/block', () => ({ useBlockId: () => mocks.projectId }));
vi.mock('@core/component/DetailsDrawer', () => ({ DetailsDrawer: () => null }));
vi.mock('@core/component/TopBar/ShareButton', async () => {
  const { createContext } = await import('solid-js');
  return {
    ShareBlockModal: (props: { name?: string; owner?: string }) => {
      mocks.modalProps.push(props);
      return <div data-testid="project-share-modal" />;
    },
    ShareDialogContext: createContext(),
  };
});
vi.mock('@core/constant/featureFlags', () => ({
  get ENABLE_PROJECT_SHARING() {
    return mocks.sharingEnabled;
  },
}));

import { ModalsProvider } from './ModalsProvider';

afterEach(cleanup);

beforeEach(() => {
  mocks.modalProps = [];
  mocks.projectId = 'project-id';
  mocks.sharingEnabled = true;
});

describe('project ModalsProvider sharing surface', () => {
  it('mounts one ShareBlockModal with exact project metadata when enabled', () => {
    render(() => (
      <ModalsProvider>
        <span>content</span>
      </ModalsProvider>
    ));

    expect(mocks.modalProps).toEqual([
      { name: 'Project Atlas', owner: 'owner-id' },
    ]);
  });

  it.each(['root', 'trash', 'special-project'])(
    'does not mount a ShareBlockModal for special project %s',
    (id) => {
      mocks.projectId = id;
      render(() => <ModalsProvider />);

      expect(mocks.modalProps).toEqual([]);
    }
  );

  it('does not mount a ShareBlockModal when project sharing is disabled', () => {
    mocks.sharingEnabled = false;
    render(() => <ModalsProvider />);

    expect(mocks.modalProps).toEqual([]);
  });
});
