import {
  getBuiltinPropertyIds,
  PROPERTY_OPTION_IDS,
  SYSTEM_PROPERTY_IDS,
} from '@property/constants';
import type { SoupProperty } from '@service-storage/generated/schemas';
import { describe, expect, it } from 'vitest';
import type { TaskEntityWithProperties } from '../types/entity';
import {
  getLocalDateKey,
  getTaskAssigneeIds,
  getTaskDueDate,
  getTaskMilestoneState,
  getTaskScheduleProjection,
  getTaskStartDate,
  getTaskStatusOptionId,
  isCurrentUserAssigned,
  isTaskClosed,
  isTaskMilestone,
} from '../utils/task-properties';

const createSoupProperty = (
  definitionId: string,
  value: unknown
): SoupProperty => {
  return {
    definition: { id: definitionId },
    value,
  } as unknown as SoupProperty;
};

const createTask = (props?: {
  isCompleted?: boolean;
  properties?: SoupProperty[];
}): TaskEntityWithProperties => {
  return {
    id: 'task-1',
    name: 'Task',
    ownerId: 'owner-1',
    type: 'document',
    fileType: 'md',
    subType: {
      type: 'task',
      is_completed: props?.isCompleted,
    },
    properties: props?.properties ?? [],
  };
};

describe('task property helpers', () => {
  describe('task dates and schedule projection', () => {
    it('keeps Start Date stable and includes it in task builtins', () => {
      expect(SYSTEM_PROPERTY_IDS.START_DATE).toBe(
        '00000001-0000-0000-0000-000000000014'
      );
      expect(getBuiltinPropertyIds('task')).toContain(
        SYSTEM_PROPERTY_IDS.START_DATE
      );
      expect(getBuiltinPropertyIds('task').slice(-2)).toEqual([
        SYSTEM_PROPERTY_IDS.MILESTONE,
        SYSTEM_PROPERTY_IDS.START_DATE,
      ]);
    });

    it('parses only valid Date values for start and due dates', () => {
      const valid = '2026-08-30T12:00:00.000Z';
      const entity = createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.START_DATE, {
            type: 'Date',
            value: valid,
          }),
          createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
            type: 'Date',
            value: valid,
          }),
        ],
      });

      expect(getTaskStartDate(entity)?.toISOString()).toBe(valid);
      expect(getTaskDueDate(entity)?.toISOString()).toBe(valid);
      expect(
        getTaskStartDate(
          createTask({
            properties: [
              createSoupProperty(SYSTEM_PROPERTY_IDS.START_DATE, {
                type: 'Date',
                value: 'not-a-date',
              }),
            ],
          })
        )
      ).toBeUndefined();
      expect(
        getTaskStartDate(
          createTask({
            properties: [
              createSoupProperty(SYSTEM_PROPERTY_IDS.START_DATE, {
                type: 'Text',
                value: valid,
              }),
            ],
          })
        )
      ).toBeUndefined();
    });

    it('projects every start and due date schedule state', () => {
      const date = (definitionId: string, value: string) =>
        createSoupProperty(definitionId, { type: 'Date', value });
      const start = '2026-08-30T12:00:00.000Z';
      const due = '2026-08-31T12:00:00.000Z';

      expect(
        getTaskScheduleProjection(
          createTask({
            properties: [date(SYSTEM_PROPERTY_IDS.START_DATE, start)],
          })
        )
      ).toEqual({ kind: 'unscheduled' });
      expect(
        getTaskScheduleProjection(
          createTask({
            properties: [date(SYSTEM_PROPERTY_IDS.DUE_DATE, due)],
          })
        )
      ).toMatchObject({ kind: 'deadline' });
      expect(getLocalDateKey(new Date('2026-08-30T23:00:00'))).toBe(
        getLocalDateKey(new Date('2026-08-30T01:00:00'))
      );
      expect(
        getTaskScheduleProjection(
          createTask({
            properties: [
              date(SYSTEM_PROPERTY_IDS.START_DATE, '2026-08-30T23:00:00'),
              date(SYSTEM_PROPERTY_IDS.DUE_DATE, '2026-08-30T01:00:00'),
            ],
          })
        )
      ).toMatchObject({ kind: 'deadline' });
      expect(
        getTaskScheduleProjection(
          createTask({
            properties: [
              date(SYSTEM_PROPERTY_IDS.START_DATE, 'not-a-date'),
              date(SYSTEM_PROPERTY_IDS.DUE_DATE, due),
            ],
          })
        )
      ).toMatchObject({ kind: 'deadline' });
      expect(
        getTaskScheduleProjection(
          createTask({
            properties: [
              date(SYSTEM_PROPERTY_IDS.START_DATE, due),
              date(SYSTEM_PROPERTY_IDS.DUE_DATE, due),
            ],
          })
        )
      ).toMatchObject({ kind: 'deadline' });
      expect(
        getTaskScheduleProjection(
          createTask({
            properties: [
              date(SYSTEM_PROPERTY_IDS.START_DATE, start),
              date(SYSTEM_PROPERTY_IDS.DUE_DATE, due),
            ],
          })
        )
      ).toMatchObject({ kind: 'span' });
      expect(
        getTaskScheduleProjection(
          createTask({
            properties: [
              date(SYSTEM_PROPERTY_IDS.START_DATE, due),
              date(SYSTEM_PROPERTY_IDS.DUE_DATE, start),
            ],
          })
        )
      ).toMatchObject({ kind: 'invalid-range' });
    });
  });

  describe('getTaskAssigneeIds', () => {
    it('returns only USER entity ids from assignees property', () => {
      const entity = createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.ASSIGNEES, {
            type: 'EntityReference',
            value: [
              { entity_type: 'USER', entity_id: 'user-1' },
              { entity_type: 'CHANNEL', entity_id: 'channel-1' },
              { entity_type: 'USER', entity_id: 'user-2' },
            ],
          }),
        ],
      });

      expect(getTaskAssigneeIds(entity)).toEqual(['user-1', 'user-2']);
    });

    it('returns empty array when assignees property is missing', () => {
      const entity = createTask();
      expect(getTaskAssigneeIds(entity)).toEqual([]);
    });
  });

  describe('getTaskStatusOptionId', () => {
    it('returns first selected status option id', () => {
      const entity = createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.STATUS, {
            type: 'SelectOption',
            value: [
              PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS,
              PROPERTY_OPTION_IDS.STATUS.IN_REVIEW,
            ],
          }),
        ],
      });

      expect(getTaskStatusOptionId(entity)).toBe(
        PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS
      );
    });

    it('returns undefined when status property is missing', () => {
      const entity = createTask();
      expect(getTaskStatusOptionId(entity)).toBeUndefined();
    });
  });

  describe('isTaskClosed', () => {
    it('returns true when subtype is marked completed', () => {
      const entity = createTask({ isCompleted: true });
      expect(isTaskClosed(entity)).toBe(true);
    });

    it('returns true for completed status option', () => {
      const entity = createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.STATUS, {
            type: 'SelectOption',
            value: [PROPERTY_OPTION_IDS.STATUS.COMPLETED],
          }),
        ],
      });

      expect(isTaskClosed(entity)).toBe(true);
    });

    it('returns false for in-progress status option', () => {
      const entity = createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.STATUS, {
            type: 'SelectOption',
            value: [PROPERTY_OPTION_IDS.STATUS.IN_PROGRESS],
          }),
        ],
      });

      expect(isTaskClosed(entity)).toBe(false);
    });
  });

  describe('milestone helpers', () => {
    const now = new Date('2026-08-30T12:00:00.000Z');
    const authoritativeReady = {
      isAuthoritative: true,
      readiness: 'ready',
    } as const;
    const authoritativeBlocked = {
      isAuthoritative: true,
      readiness: 'blocked',
    } as const;

    const milestoneTask = (properties: SoupProperty[] = []) =>
      createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.MILESTONE, {
            type: 'Boolean',
            value: true,
          }),
          ...properties,
        ],
      });

    it('recognizes only the exact true Boolean milestone marker', () => {
      expect(isTaskMilestone(milestoneTask())).toBe(true);
      expect(
        isTaskMilestone(
          createTask({
            properties: [
              createSoupProperty(SYSTEM_PROPERTY_IDS.MILESTONE, {
                type: 'Boolean',
                value: false,
              }),
            ],
          })
        )
      ).toBe(false);
      expect(
        isTaskMilestone(
          createTask({
            properties: [
              createSoupProperty(SYSTEM_PROPERTY_IDS.MILESTONE, null),
            ],
          })
        )
      ).toBe(false);
      expect(
        isTaskMilestone(
          createTask({
            properties: [
              createSoupProperty(SYSTEM_PROPERTY_IDS.MILESTONE, {
                type: 'Boolean',
                value: 'true',
              }),
            ],
          })
        )
      ).toBe(false);
    });

    it('returns undefined for missing or malformed due dates', () => {
      expect(getTaskDueDate(createTask())).toBeUndefined();
      expect(
        getTaskDueDate(
          createTask({
            properties: [
              createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
                type: 'Date',
                value: 'not-a-date',
              }),
            ],
          })
        )
      ).toBeUndefined();
      expect(
        getTaskDueDate(
          createTask({
            properties: [
              createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
                type: 'String',
                value: now.toISOString(),
              }),
            ],
          })
        )
      ).toBeUndefined();
    });

    it('returns a valid Date property instant', () => {
      const dueDate = getTaskDueDate(
        createTask({
          properties: [
            createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
              type: 'Date',
              value: now.toISOString(),
            }),
          ],
        })
      );

      expect(dueDate?.toISOString()).toBe(now.toISOString());
    });

    it('returns undefined for non-milestones', () => {
      expect(
        getTaskMilestoneState(createTask(), now, authoritativeBlocked)
      ).toBeUndefined();
    });

    it('derives overdue only for a due date strictly before now', () => {
      const before = milestoneTask([
        createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
          type: 'Date',
          value: '2026-08-30T11:59:59.999Z',
        }),
      ]);
      const equal = milestoneTask([
        createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
          type: 'Date',
          value: now.toISOString(),
        }),
      ]);
      const after = milestoneTask([
        createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
          type: 'Date',
          value: '2026-08-30T12:00:00.001Z',
        }),
      ]);

      expect(getTaskMilestoneState(before, now, authoritativeReady)).toBe(
        'overdue'
      );
      expect(getTaskMilestoneState(equal, now, authoritativeReady)).toBe(
        'milestone'
      );
      expect(getTaskMilestoneState(after, now, authoritativeReady)).toBe(
        'milestone'
      );
    });

    it('gives Completed precedence over overdue and blocked', () => {
      const entity = milestoneTask([
        createSoupProperty(SYSTEM_PROPERTY_IDS.STATUS, {
          type: 'SelectOption',
          value: [PROPERTY_OPTION_IDS.STATUS.COMPLETED],
        }),
        createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
          type: 'Date',
          value: '2026-08-30T11:59:59.999Z',
        }),
      ]);

      expect(getTaskMilestoneState(entity, now, authoritativeBlocked)).toBe(
        'complete'
      );
    });

    it('gives overdue precedence over blocked', () => {
      const entity = milestoneTask([
        createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
          type: 'Date',
          value: '2026-08-30T11:59:59.999Z',
        }),
      ]);

      expect(getTaskMilestoneState(entity, now, authoritativeBlocked)).toBe(
        'overdue'
      );
    });

    it('keeps Canceled milestones neutral', () => {
      const entity = milestoneTask([
        createSoupProperty(SYSTEM_PROPERTY_IDS.STATUS, {
          type: 'SelectOption',
          value: [PROPERTY_OPTION_IDS.STATUS.CANCELED],
        }),
        createSoupProperty(SYSTEM_PROPERTY_IDS.DUE_DATE, {
          type: 'Date',
          value: '2026-08-30T11:59:59.999Z',
        }),
      ]);

      expect(getTaskMilestoneState(entity, now, authoritativeBlocked)).toBe(
        'milestone'
      );
    });

    it('uses blocked readiness only when it is authoritative', () => {
      const entity = milestoneTask();

      expect(getTaskMilestoneState(entity, now, authoritativeReady)).toBe(
        'milestone'
      );
      expect(getTaskMilestoneState(entity, now, authoritativeBlocked)).toBe(
        'at-risk'
      );
      expect(
        getTaskMilestoneState(entity, now, {
          isAuthoritative: false,
          readiness: 'blocked',
        })
      ).toBe('milestone');
    });
  });

  describe('isCurrentUserAssigned', () => {
    it('returns false when current user is undefined', () => {
      const entity = createTask();
      expect(isCurrentUserAssigned(entity, undefined)).toBe(false);
    });

    it('returns true when task has no assignees', () => {
      const entity = createTask();
      expect(isCurrentUserAssigned(entity, 'user-1')).toBe(true);
    });

    it('returns true when current user is assigned', () => {
      const entity = createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.ASSIGNEES, {
            type: 'EntityReference',
            value: [{ entity_type: 'USER', entity_id: 'user-1' }],
          }),
        ],
      });

      expect(isCurrentUserAssigned(entity, 'user-1')).toBe(true);
    });

    it('returns false when current user is not assigned', () => {
      const entity = createTask({
        properties: [
          createSoupProperty(SYSTEM_PROPERTY_IDS.ASSIGNEES, {
            type: 'EntityReference',
            value: [{ entity_type: 'USER', entity_id: 'user-2' }],
          }),
        ],
      });

      expect(isCurrentUserAssigned(entity, 'user-1')).toBe(false);
    });
  });
});
