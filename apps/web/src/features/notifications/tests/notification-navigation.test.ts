import { describe, expect, it, vi } from 'vitest';

vi.mock('@block-calendar/calendar-range', () => ({
  createCalendarBlockRange: vi.fn(),
}));
vi.mock('@block-calendar/types', () => ({ CALENDAR_BLOCK_ID: 'calendar' }));
vi.mock('@block-channel/utils/link', () => ({
  getChannelParams: vi.fn(),
  navigateToChannelMessage: vi.fn(),
}));
vi.mock('@block-md/constants', () => ({
  URL_PARAMS: { commentId: 'comment' },
}));
vi.mock('@block-pdf/constants', () => ({
  URL_PARAMS: { annotationId: 'annotation' },
}));
vi.mock('@core/constant/allBlocks', () => ({
  itemToBlockName: vi.fn(),
  resolveBlockAlias: (type: string) => type,
}));
vi.mock('@core/constant/featureFlags', () => ({
  ENABLE_CALENDAR_UI: false,
  ENABLE_REMINDERS: false,
  USE_MACRO_PR_SUMMARY_BLOCK: false,
}));
vi.mock('@core/util/url', () => ({ openExternalUrl: vi.fn() }));
vi.mock('@queries/notification/user-notifications', () => ({
  getNotificationById: vi.fn(),
}));
vi.mock('@queries/reminders/reminders', () => ({ getReminderById: vi.fn() }));
vi.mock('../github-event-types', () => ({ GITHUB_EVENT_TYPES: [] }));
vi.mock('../notification-helpers', () => ({ isChannelNotification: vi.fn() }));
vi.mock('../notification-resolvers', () => ({
  DefaultNotificationBlockNameResolver: class {},
}));
vi.mock('../notification-source', () => ({ CHANNEL_EVENT_TYPES: [] }));
vi.mock('../notification-stacking', () => ({
  getMostRecentNotification: vi.fn(),
  stackNotifications: vi.fn(),
}));

import { openNotification } from '../notification-navigation';
import type { UnifiedNotification } from '../types';

describe('task_ready navigation', () => {
  it('opens the existing task block using metadata taskId', async () => {
    const layoutManager = {
      getSplitByContent: vi.fn(() => undefined),
      openWithSplit: vi.fn(),
    } as any;
    const notification = {
      id: 'notification-1',
      entity_id: 'document-1',
      notification_metadata: {
        tag: 'task_ready',
        content: { taskId: 'task-1', taskName: 'Prepare release notes' },
      },
    } as UnifiedNotification;

    await openNotification(notification, layoutManager).match(
      () => undefined,
      (error) => {
        throw error;
      }
    );

    expect(layoutManager.openWithSplit).toHaveBeenCalledWith(
      { type: 'task', id: 'task-1' },
      expect.objectContaining({ preferNewSplit: false })
    );
  });
});
