import type { TaskEntityWithProperties } from '@entity/types/entity';
import { SYSTEM_PROPERTY_IDS } from '@property/constants';
import type { SoupProperty } from '@service-storage/generated/schemas';
import { describe, expect, it, vi } from 'vitest';

vi.mock('../predicates', () => ({
  hasNoPriority: () => false,
  isCanceled: () => false,
  isCompleted: () => false,
  isHighPriority: () => false,
  isInProgress: () => false,
  isInReview: () => false,
  isLowPriority: () => false,
  isMediumPriority: () => false,
  isNotStarted: () => false,
  isOpen: () => true,
  isUrgentPriority: () => false,
  taskAssignedToUserFilter: () => () => false,
  taskFilter: () => true,
}));
vi.mock('./my-tasks', () => ({
  getMyTasksQuery: () => ({}),
  isMyTask: () => false,
}));

import { taskMilestoneFilter } from './task';

const task = (milestone: unknown): TaskEntityWithProperties => ({
  id: 'task-1',
  name: 'Task',
  ownerId: 'owner-1',
  type: 'document',
  fileType: 'md',
  subType: { type: 'task' },
  properties: [
    {
      definition: { id: SYSTEM_PROPERTY_IDS.MILESTONE },
      value: milestone,
    } as SoupProperty,
  ],
});

describe('taskMilestoneFilter', () => {
  it('matches only tasks with the exact true milestone value', () => {
    expect(
      taskMilestoneFilter.predicate(task({ type: 'Boolean', value: true }), {})
    ).toBe(true);
    expect(
      taskMilestoneFilter.predicate(task({ type: 'Boolean', value: false }), {})
    ).toBe(false);
    expect(taskMilestoneFilter.predicate(task(null), {})).toBe(false);
    expect(
      taskMilestoneFilter.predicate(
        {
          ...task({ type: 'Boolean', value: true }),
          subType: undefined,
        } as unknown as TaskEntityWithProperties,
        {}
      )
    ).toBe(false);
  });

  it('uses the exact milestone definition and boolean query value', () => {
    expect(taskMilestoneFilter.query).toEqual({
      include: {
        subType: ['task'],
        properties: [
          {
            propertyId: SYSTEM_PROPERTY_IDS.MILESTONE,
            type: 'boolean',
            value: true,
          },
        ],
      },
    });
  });
});
