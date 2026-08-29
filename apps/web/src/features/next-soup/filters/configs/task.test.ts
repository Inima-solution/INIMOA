import type {
  EntityData,
  TaskEntityWithProperties,
} from '@entity/types/entity';
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

import { TASK_DUE_DATE_FILTERS, taskMilestoneFilter } from './task';

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

const dueTask = (value: unknown): TaskEntityWithProperties => ({
  ...task(null),
  properties: [
    {
      definition: { id: SYSTEM_PROPERTY_IDS.DUE_DATE },
      value,
    } as SoupProperty,
  ],
});

describe('TASK_DUE_DATE_FILTERS', () => {
  it('uses the stable Due Date bucket query values and task subtype', () => {
    expect(
      TASK_DUE_DATE_FILTERS.map((filter) => [filter.id, filter.query])
    ).toEqual([
      [
        'task-due-overdue',
        {
          include: {
            subType: ['task'],
            properties: [
              {
                propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
                type: 'date',
                value: 'overdue',
              },
            ],
          },
        },
      ],
      [
        'task-due-today',
        {
          include: {
            subType: ['task'],
            properties: [
              {
                propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
                type: 'date',
                value: 'today',
              },
            ],
          },
        },
      ],
      [
        'task-due-upcoming',
        {
          include: {
            subType: ['task'],
            properties: [
              {
                propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
                type: 'date',
                value: 'upcoming',
              },
            ],
          },
        },
      ],
      [
        'task-due-none',
        {
          include: {
            subType: ['task'],
            properties: [
              {
                propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
                type: 'date',
                value: 'no-due',
              },
            ],
          },
        },
      ],
    ]);
  });

  it('matches local-day due-date buckets and only treats invalid dates as no due date', () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date(2026, 7, 30, 12));

      const local = (dayOffset: number, hour = 0) =>
        new Date(2026, 7, 30 + dayOffset, hour).toISOString();
      const predicates = Object.fromEntries(
        TASK_DUE_DATE_FILTERS.map((filter) => [filter.id, filter.predicate])
      );

      expect(
        predicates['task-due-overdue']!(
          dueTask({ type: 'Date', value: local(-1, 23) }),
          {}
        )
      ).toBe(true);
      expect(
        predicates['task-due-today']!(
          dueTask({ type: 'Date', value: local(0) }),
          {}
        )
      ).toBe(true);
      expect(
        predicates['task-due-today']!(
          dueTask({ type: 'Date', value: local(1) }),
          {}
        )
      ).toBe(false);
      expect(
        predicates['task-due-upcoming']!(
          dueTask({ type: 'Date', value: local(1) }),
          {}
        )
      ).toBe(true);
      expect(predicates['task-due-none']!(dueTask(undefined), {})).toBe(true);
      expect(
        predicates['task-due-none']!(
          dueTask({ type: 'Date', value: 'not-a-date' }),
          {}
        )
      ).toBe(true);
      expect(
        predicates['task-due-none']!(
          dueTask({ type: 'Date', value: local(0) }),
          {}
        )
      ).toBe(false);
      const nonTask = {
        ...dueTask({ type: 'Date', value: local(0) }),
        subType: undefined,
      } as unknown as EntityData;
      expect(predicates['task-due-today']!(nonTask, {})).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });
});
