import { SYSTEM_PROPERTY_IDS } from '@property/constants';
import { describe, expect, it, vi } from 'vitest';
import { compileToAst, defineQueryFilters, queryStateFrom } from './compile';
import { removeFieldValues } from './field-values';
import { resolveDueDateBucket } from './task-due-date';
import type { DocumentFilterExpression, QueryState } from './types';

const NIL_UUID = '00000000-0000-0000-0000-000000000000';

const withFixedLocalTime = (
  timezone: string,
  instant: string,
  test: () => void
) => {
  const previousTimezone = process.env.TZ;
  process.env.TZ = timezone;
  vi.useFakeTimers();
  vi.setSystemTime(new Date(instant));

  try {
    test();
  } finally {
    vi.useRealTimers();
    if (previousTimezone === undefined) {
      delete process.env.TZ;
    } else {
      process.env.TZ = previousTimezone;
    }
  }
};

describe('defineQueryFilters', () => {
  it('treats emailView as referencing the email target', () => {
    const query = defineQueryFilters({ emailView: 'sent' });

    // No match-nothing filter on the email target itself...
    expect(query.include?.threadId).toBeUndefined();
    // ...while other entity targets are still excluded.
    expect(query.include?.documentId).toEqual([NIL_UUID]);
    expect(query.include?.calendarEventId).toEqual([NIL_UUID]);
    expect(query.include?.chatId).toEqual([NIL_UUID]);
  });

  it('stuffs match-nothing filters on all targets when nothing is referenced', () => {
    const query = defineQueryFilters({});

    expect(query.include?.threadId).toEqual([NIL_UUID]);
    expect(query.include?.documentId).toEqual([NIL_UUID]);
    expect(query.include?.calendarEventId).toEqual([NIL_UUID]);
  });

  it('treats calendar event ids as referencing the calendar target', () => {
    const query = defineQueryFilters({
      exclude: { calendarEventId: [NIL_UUID] },
    });

    expect(query.include?.calendarEventId).toBeUndefined();
    expect(query.include?.documentId).toEqual([NIL_UUID]);
  });
});

describe('compileToAst', () => {
  it('NIL-excludes calendar events from query states that predate the target', () => {
    const ast = compileToAst(
      queryStateFrom({ include: { threadId: ['thread-1'] } })
    );

    expect(ast.calf).toEqual({ l: { id: NIL_UUID } });
  });

  it('compiles calendar event ids to the calendar AST target', () => {
    const ast = compileToAst(
      queryStateFrom({ include: { calendarEventId: ['event-1'] } })
    );

    expect(ast.calf).toEqual({ l: { id: 'event-1' } });
  });

  it('keeps existing flat include and exclude document filters unchanged', () => {
    const state: QueryState = {
      include: {
        fileType: ['pdf', 'md'],
        subType: ['snippet', 'task'],
      },
      exclude: {
        documentOwnerId: ['user-1'],
      },
    };

    expect(compileToAst(state).df).toEqual({
      '&': [
        {
          '|': [{ l: { ft: 'pdf' } }, { l: { ft: 'md' } }],
        },
        {
          '&': [
            {
              '|': [{ l: { dst: 'snippet' } }, { l: { dst: 'task' } }],
            },
            {
              '!': { l: { o: 'user-1' } },
            },
          ],
        },
      ],
    });
  });

  it('compiles nested documentWhere OR across file type and subtype groups', () => {
    const expression: DocumentFilterExpression = {
      op: 'or',
      clauses: [
        { include: { fileType: ['pdf'] } },
        {
          op: 'and',
          clauses: [
            { include: { fileType: ['md'] } },
            { include: { subType: ['snippet', 'task'] } },
          ],
        },
      ],
    };

    expect(
      compileToAst({
        include: {},
        exclude: {},
        documentWhere: [expression],
      }).df
    ).toEqual({
      '|': [
        { l: { ft: 'pdf' } },
        {
          '&': [
            { l: { ft: 'md' } },
            {
              '|': [{ l: { dst: 'snippet' } }, { l: { dst: 'task' } }],
            },
          ],
        },
      ],
    });
  });

  it('ANDs documentWhere with top-level document filters', () => {
    expect(
      compileToAst({
        include: { projectId: ['project-1'] },
        exclude: {},
        documentWhere: [{ include: { fileType: ['pdf'] } }],
      }).df
    ).toEqual({
      '&': [{ l: { pid: 'project-1' } }, { l: { ft: 'pdf' } }],
    });
  });

  it('supports NOT groups in documentWhere', () => {
    expect(
      compileToAst({
        include: {},
        exclude: {},
        documentWhere: [
          {
            op: 'not',
            clause: { include: { subType: ['task'] } },
          },
        ],
      }).df
    ).toEqual({
      '!': { l: { dst: 'task' } },
    });
  });

  it('normalizes query documentWhere into QueryState', () => {
    expect(
      queryStateFrom({
        documentWhere: { include: { fileType: ['pdf'] } },
      }).documentWhere
    ).toEqual([{ include: { fileType: ['pdf'] } }]);
  });

  it('compiles foreign entity source filters to the backend AST source literal', () => {
    const ast = compileToAst(
      queryStateFrom({
        include: {
          foreignEntitySource: ['github_pull_request'],
          foreignEntityDone: false,
        },
      })
    );

    expect(ast.fef).toEqual({
      '&': [{ l: { fes: 'github_pull_request' } }, { l: { nd: false } }],
    });
  });

  it('compiles the reminder opt-in to a bare Include literal', () => {
    const ast = compileToAst(
      queryStateFrom({ include: { includeReminders: true } })
    );

    // Reminders are off in Soup unless a view asks; this literal is the ask.
    expect(ast.remf).toEqual({ l: 'inc' });
  });

  it('leaves reminders unrequested when a view does not opt in', () => {
    const ast = compileToAst(
      queryStateFrom({ include: { documentDone: false } })
    );

    expect(ast.remf).toBeUndefined();
  });

  it('compiles channel message thread ids onto regular channel filters', () => {
    const ast = compileToAst(
      queryStateFrom({
        include: {
          channelMessageThreadId: ['00000000-0000-0000-0000-000000000001'],
        },
      })
    );

    expect(ast.chanf).toEqual({
      l: { ThreadId: '00000000-0000-0000-0000-000000000001' },
    });
  });

  it('compiles channel-thread root sender excludes onto channel-thread filters', () => {
    const ast = compileToAst(
      queryStateFrom({
        exclude: {
          channelThreadRootSenderId: ['macro|me@example.com'],
        },
      })
    );

    expect(ast.cthf).toEqual({
      '!': { l: { RootSender: 'macro|me@example.com' } },
    });
  });

  it('compiles tag filters as one OR group across definitions by default', () => {
    const ast = compileToAst(
      queryStateFrom({
        include: {
          tagFilters: [
            { propertyId: 'def-1', type: 'select', value: 'opt-1' },
            { propertyId: 'def-2', type: 'select', value: 'opt-2' },
          ],
        },
      })
    );

    expect(ast.propf).toEqual({
      '|': [
        { l: { pd: 'def-1', v: { so: 'opt-1' } } },
        { l: { pd: 'def-2', v: { so: 'opt-2' } } },
      ],
    });
  });

  it('compiles tag filters as an AND group when tagFilterMode is all', () => {
    const ast = compileToAst(
      queryStateFrom({
        include: {
          tagFilterMode: 'all',
          tagFilters: [
            { propertyId: 'def-1', type: 'select', value: 'opt-1' },
            { propertyId: 'def-2', type: 'select', value: 'opt-2' },
          ],
        },
      })
    );

    expect(ast.propf).toEqual({
      '&': [
        { l: { pd: 'def-1', v: { so: 'opt-1' } } },
        { l: { pd: 'def-2', v: { so: 'opt-2' } } },
      ],
    });
  });

  it('compiles boolean properties with exact true and false values', () => {
    expect(
      compileToAst(
        queryStateFrom({
          include: {
            properties: [
              { propertyId: 'milestone', type: 'boolean', value: true },
              { propertyId: 'archived', type: 'boolean', value: false },
            ],
          },
        })
      ).propf
    ).toEqual({
      '&': [
        { l: { pd: 'milestone', v: { b: true } } },
        { l: { pd: 'archived', v: { b: false } } },
      ],
    });
  });

  it('keeps select and entity property AST semantics while filtering boolean excludes', () => {
    expect(
      compileToAst(
        queryStateFrom({
          include: {
            properties: [
              { propertyId: 'select', type: 'select', value: 'option-1' },
              { propertyId: 'entity', type: 'entity', value: 'entity-1' },
              { propertyId: 'milestone', type: 'boolean', value: true },
            ],
          },
          exclude: {
            properties: [
              { propertyId: 'milestone', type: 'boolean', value: false },
            ],
          },
        })
      ).propf
    ).toEqual({
      '&': [
        { l: { pd: 'select', v: { so: 'option-1' } } },
        {
          '&': [
            { l: { pd: 'entity', v: { er: 'entity-1' } } },
            {
              '&': [
                { l: { pd: 'milestone', v: { b: true } } },
                { '!': { l: { pd: 'milestone', v: { b: false } } } },
              ],
            },
          ],
        },
      ],
    });
  });

  it('resolves Due Date buckets from local calendar starts, including DST', () => {
    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      expect(resolveDueDateBucket('overdue')).toEqual({
        lt: '2026-08-29T15:00:00.000Z',
      });
      expect(resolveDueDateBucket('today')).toEqual({
        gte: '2026-08-29T15:00:00.000Z',
        lt: '2026-08-30T15:00:00.000Z',
      });
      expect(resolveDueDateBucket('upcoming')).toEqual({
        gte: '2026-08-30T15:00:00.000Z',
      });
      expect(resolveDueDateBucket('no-due')).toEqual({ exclude: true });
    });

    withFixedLocalTime(
      'America/Los_Angeles',
      '2026-03-08T19:00:00.000Z',
      () => {
        expect(resolveDueDateBucket('today')).toEqual({
          gte: '2026-03-08T08:00:00.000Z',
          lt: '2026-03-09T07:00:00.000Z',
        });
      }
    );
  });

  it('compiles every Due Date bucket to the compact TASK property AST', () => {
    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      const compileBucket = (
        value: 'overdue' | 'today' | 'upcoming' | 'no-due'
      ) =>
        compileToAst(
          queryStateFrom({
            include: {
              properties: [
                {
                  propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
                  type: 'date',
                  value,
                },
              ],
            },
          })
        ).propf;

      expect(compileBucket('overdue')).toEqual({
        l: {
          pd: SYSTEM_PROPERTY_IDS.DUE_DATE,
          et: 'TASK',
          v: { dr: { lt: '2026-08-29T15:00:00.000Z' } },
        },
      });
      expect(compileBucket('today')).toEqual({
        l: {
          pd: SYSTEM_PROPERTY_IDS.DUE_DATE,
          et: 'TASK',
          v: {
            dr: {
              gte: '2026-08-29T15:00:00.000Z',
              lt: '2026-08-30T15:00:00.000Z',
            },
          },
        },
      });
      expect(compileBucket('upcoming')).toEqual({
        l: {
          pd: SYSTEM_PROPERTY_IDS.DUE_DATE,
          et: 'TASK',
          v: { dr: { gte: '2026-08-30T15:00:00.000Z' } },
        },
      });
      expect(compileBucket('no-due')).toEqual({
        '!': {
          l: {
            pd: SYSTEM_PROPERTY_IDS.DUE_DATE,
            et: 'TASK',
            v: { dr: {} },
          },
        },
      });
    });
  });

  it('keeps the Due Date bucket stable while its resolved range crosses midnight', () => {
    const filter = {
      propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
      type: 'date' as const,
      value: 'today' as const,
    };

    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      expect(filter.value).toBe('today');
      expect(resolveDueDateBucket(filter.value)).not.toEqual(
        resolveDueDateBucket(filter.value, new Date('2026-08-30T16:00:00.000Z'))
      );
      expect(filter.value).toBe('today');
      expect(
        removeFieldValues(
          { properties: [filter] },
          { properties: [{ ...filter }] }
        )
      ).toEqual({});
    });
  });

  it('compiles custom Task Date filters with their exact property id', () => {
    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      expect(
        compileToAst(
          queryStateFrom({
            include: {
              properties: [
                { propertyId: 'custom-date', type: 'date', value: 'today' },
              ],
            },
          })
        ).propf
      ).toEqual({
        l: {
          pd: 'custom-date',
          et: 'TASK',
          v: {
            dr: {
              gte: '2026-08-29T15:00:00.000Z',
              lt: '2026-08-30T15:00:00.000Z',
            },
          },
        },
      });
    });
  });

  it('ANDs mixed and duplicate Due Date filters without broadening results', () => {
    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      const compileProperties = (
        properties: NonNullable<QueryState['include']['properties']>
      ) => compileToAst(queryStateFrom({ include: { properties } })).propf;

      expect(
        compileProperties([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'today',
          },
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'select',
            value: 'option-1',
          },
        ])
      ).toEqual({
        '&': [
          {
            l: {
              pd: SYSTEM_PROPERTY_IDS.DUE_DATE,
              et: 'TASK',
              v: {
                dr: {
                  gte: '2026-08-29T15:00:00.000Z',
                  lt: '2026-08-30T15:00:00.000Z',
                },
              },
            },
          },
          { l: { pd: SYSTEM_PROPERTY_IDS.DUE_DATE, v: { so: 'option-1' } } },
        ],
      });

      expect(
        compileProperties([
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'today',
          },
          {
            propertyId: SYSTEM_PROPERTY_IDS.DUE_DATE,
            type: 'date',
            value: 'upcoming',
          },
        ])
      ).toEqual({
        '&': [
          {
            l: {
              pd: SYSTEM_PROPERTY_IDS.DUE_DATE,
              et: 'TASK',
              v: {
                dr: {
                  gte: '2026-08-29T15:00:00.000Z',
                  lt: '2026-08-30T15:00:00.000Z',
                },
              },
            },
          },
          {
            l: {
              pd: SYSTEM_PROPERTY_IDS.DUE_DATE,
              et: 'TASK',
              v: { dr: { gte: '2026-08-30T15:00:00.000Z' } },
            },
          },
        ],
      });
    });
  });

  it('ANDs multiple persisted custom Date filters fail-closed', () => {
    withFixedLocalTime('Asia/Seoul', '2026-08-30T03:00:00.000Z', () => {
      expect(
        compileToAst(
          queryStateFrom({
            include: {
              properties: [
                { propertyId: 'custom-date', type: 'date', value: 'today' },
                {
                  propertyId: 'custom-date',
                  type: 'date',
                  value: 'upcoming',
                },
              ],
            },
          })
        ).propf
      ).toEqual({
        '&': [
          {
            l: {
              pd: 'custom-date',
              et: 'TASK',
              v: {
                dr: {
                  gte: '2026-08-29T15:00:00.000Z',
                  lt: '2026-08-30T15:00:00.000Z',
                },
              },
            },
          },
          {
            l: {
              pd: 'custom-date',
              et: 'TASK',
              v: { dr: { gte: '2026-08-30T15:00:00.000Z' } },
            },
          },
        ],
      });
    });
  });
});
