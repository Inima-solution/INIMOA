import { describe, expect, it } from 'vitest';
import { NOTIFICATION_EVENT_GROUPS } from '../notification-event-catalog';

describe('notification event catalog', () => {
  it('lists task_ready in the Tasks settings group', () => {
    const tasks = NOTIFICATION_EVENT_GROUPS.find(({ id }) => id === 'tasks');

    expect(tasks?.events).toContainEqual({
      type: 'task_ready',
      label: 'Ready tasks',
      description: 'When a task is ready for you',
    });
  });
});
