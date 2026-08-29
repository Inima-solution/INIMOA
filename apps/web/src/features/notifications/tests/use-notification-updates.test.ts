import { describe, expect, it, vi } from 'vitest';

const cache = vi.hoisted(() => ({
  invalidateEntityNotifications: vi.fn(),
  invalidateSoupEntity: vi.fn(),
  refetchSoupEntity: vi.fn(),
}));

vi.mock('@queries/client', () => ({
  queryClient: { invalidateQueries: vi.fn() },
}));
vi.mock('@queries/email/keys', () => ({ emailKeys: {} }));
vi.mock('@queries/email/link', () => ({ invalidateEmailLinks: vi.fn() }));
vi.mock('@queries/notification/user-notifications', () => ({
  invalidateEntityNotifications: cache.invalidateEntityNotifications,
}));
vi.mock('@queries/soup/normalized-cache', () => ({
  invalidateSoupEntity: cache.invalidateSoupEntity,
  refetchSoupEntity: cache.refetchSoupEntity,
}));
vi.mock('@queries/team/keys', () => ({ teamKeys: {} }));

import type { UnifiedNotification } from '../types';
import { handleNotificationUpdate } from '../use-notification-updates';

describe('task_ready cache updates', () => {
  it('refreshes only the document soup entity and entity notifications', () => {
    const notification = {
      entity_id: 'task-1',
      notification_metadata: {
        tag: 'task_ready',
        content: { taskId: 'task-1', taskName: 'Prepare release notes' },
      },
    } as UnifiedNotification;

    handleNotificationUpdate(notification);

    expect(cache.refetchSoupEntity).toHaveBeenCalledWith('task-1', 'document');
    expect(cache.invalidateSoupEntity).toHaveBeenCalledWith('task-1');
    expect(cache.invalidateEntityNotifications).toHaveBeenCalledWith('task-1');
  });
});
