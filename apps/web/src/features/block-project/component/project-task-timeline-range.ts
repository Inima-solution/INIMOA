import {
  formatLocalDate,
  parseLocalDate,
} from '@app/features/calendar/utils/calendar-date';

/** The minimum readable width reserved for one non-interactive ruler tick. */
export const PROJECT_TIMELINE_MIN_TICK_WIDTH = 72;
export const PROJECT_TIMELINE_MAX_TICKS = 64;

export type ProjectTimelineRange =
  | { kind: 'valid'; start: Date; end: Date }
  | { kind: 'not-set' }
  | { kind: 'unavailable' }
  | { kind: 'invalid' };

export type ProjectTimelineScale =
  | 'day'
  | 'week'
  | 'month'
  | 'quarter'
  | 'boundary';
export type ProjectTimelineTick = { start: Date; end: Date };

function nextDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1);
}

function compareDays(left: Date, right: Date) {
  return (
    left.getFullYear() - right.getFullYear() ||
    left.getMonth() - right.getMonth() ||
    left.getDate() - right.getDate()
  );
}

function clippedEnd(left: Date, right: Date) {
  return compareDays(left, right) < 0 ? left : right;
}

export function getProjectTimelineRange(
  startValue: string | null | undefined,
  targetValue: string | null | undefined
): ProjectTimelineRange {
  if (!startValue && !targetValue) return { kind: 'not-set' };
  if (!startValue || !targetValue) return { kind: 'invalid' };
  const start = parseLocalDate(startValue);
  const end = parseLocalDate(targetValue);
  if (!start || !end || compareDays(start, end) > 0) return { kind: 'invalid' };
  return { kind: 'valid', start, end };
}

function ticksFor(
  range: Extract<ProjectTimelineRange, { kind: 'valid' }>,
  scale: Exclude<ProjectTimelineScale, 'boundary'>,
  limit = PROJECT_TIMELINE_MAX_TICKS + 1
): ProjectTimelineTick[] {
  const ticks: ProjectTimelineTick[] = [];
  let cursor = range.start;
  while (compareDays(cursor, range.end) <= 0 && ticks.length < limit) {
    let end: Date;
    if (scale === 'day') end = cursor;
    else if (scale === 'week')
      end = new Date(
        cursor.getFullYear(),
        cursor.getMonth(),
        cursor.getDate() + 6
      );
    else if (scale === 'month')
      end = new Date(cursor.getFullYear(), cursor.getMonth() + 1, 0);
    else {
      const quarterEndMonth = Math.floor(cursor.getMonth() / 3) * 3 + 2;
      end = new Date(cursor.getFullYear(), quarterEndMonth + 1, 0);
    }
    end = clippedEnd(end, range.end);
    ticks.push({ start: cursor, end });
    cursor = nextDay(end);
  }
  return ticks;
}

export function getProjectTimelineRuler(
  range: Extract<ProjectTimelineRange, { kind: 'valid' }>,
  width: number | undefined
): { scale: ProjectTimelineScale; ticks: ProjectTimelineTick[] } {
  const available =
    Number.isFinite(width) && width! > 0
      ? Math.floor(width! / PROJECT_TIMELINE_MIN_TICK_WIDTH)
      : 0;
  const scales: Exclude<ProjectTimelineScale, 'boundary'>[] = [
    'day',
    'week',
    'month',
    'quarter',
  ];
  for (const scale of scales) {
    const ticks = ticksFor(range, scale);
    if (ticks.length <= PROJECT_TIMELINE_MAX_TICKS && ticks.length <= available)
      return { scale, ticks };
  }
  return { scale: 'boundary', ticks: [] };
}

export function getProjectTimelineToday(
  range: Extract<ProjectTimelineRange, { kind: 'valid' }>,
  now = new Date()
) {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const relation =
    compareDays(today, range.start) < 0
      ? 'before'
      : compareDays(today, range.end) > 0
        ? 'after'
        : 'inside';
  return { date: today, key: formatLocalDate(today), relation } as const;
}

/** A DST-safe ordinal for a local calendar day; never use elapsed local hours. */
export function getLocalCalendarDayOrdinal(date: Date) {
  return (
    Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000
  );
}

export function getInclusiveLocalCalendarDays(start: Date, end: Date) {
  return Math.max(
    1,
    getLocalCalendarDayOrdinal(end) - getLocalCalendarDayOrdinal(start) + 1
  );
}

export function getProjectTimelineTodayPercent(
  range: Extract<ProjectTimelineRange, { kind: 'valid' }>,
  today: Date
) {
  return Math.max(
    0,
    Math.min(
      100,
      ((getLocalCalendarDayOrdinal(today) -
        getLocalCalendarDayOrdinal(range.start) +
        0.5) /
        getInclusiveLocalCalendarDays(range.start, range.end)) *
        100
    )
  );
}

function isValidDate(date: Date) {
  return Number.isFinite(date.getTime());
}

/**
 * Returns a local calendar day's center within a project range, or nothing
 * when the date cannot truthfully be placed on that range.
 */
export function getProjectTimelineDayCenterPercent(
  range: Extract<ProjectTimelineRange, { kind: 'valid' }>,
  date: Date
) {
  if (
    !isValidDate(range.start) ||
    !isValidDate(range.end) ||
    !isValidDate(date)
  )
    return undefined;
  const day = getLocalCalendarDayOrdinal(date);
  const start = getLocalCalendarDayOrdinal(range.start);
  const end = getLocalCalendarDayOrdinal(range.end);
  if (day < start || day > end) return undefined;
  return (
    ((day - start + 0.5) /
      getInclusiveLocalCalendarDays(range.start, range.end)) *
    100
  );
}

/**
 * Clips an inclusive local-day span to a project range for row geometry.
 */
export function getProjectTimelineClippedSpanPercent(
  range: Extract<ProjectTimelineRange, { kind: 'valid' }>,
  startDate: Date,
  endDate: Date
) {
  if (
    !isValidDate(range.start) ||
    !isValidDate(range.end) ||
    !isValidDate(startDate) ||
    !isValidDate(endDate)
  )
    return undefined;
  const start = getLocalCalendarDayOrdinal(startDate);
  const end = getLocalCalendarDayOrdinal(endDate);
  const rangeStart = getLocalCalendarDayOrdinal(range.start);
  const rangeEnd = getLocalCalendarDayOrdinal(range.end);
  if (start > end || end < rangeStart || start > rangeEnd) return undefined;
  const clippedStart = Math.max(start, rangeStart);
  const clippedEnd = Math.min(end, rangeEnd);
  const days = getInclusiveLocalCalendarDays(range.start, range.end);
  return {
    leftPercent: ((clippedStart - rangeStart) / days) * 100,
    widthPercent: ((clippedEnd - clippedStart + 1) / days) * 100,
  };
}

export function formatProjectTimelineDate(date: Date) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(
    date
  );
}

export function formatProjectTimelineTick(
  tick: ProjectTimelineTick,
  scale: ProjectTimelineScale
) {
  // Boundary has no generated ticks, but keeping this defensive makes callers
  // honest when rendering conditionals do not narrow their value types.
  if (scale === 'boundary') return formatProjectTimelineDate(tick.start);
  if (scale === 'month') {
    return new Intl.DateTimeFormat(undefined, {
      month: 'short',
      year: 'numeric',
    }).format(tick.start);
  }
  if (scale === 'quarter') {
    return `Q${Math.floor(tick.start.getMonth() / 3) + 1} ${tick.start.getFullYear()}`;
  }
  if (scale === 'week') {
    return `${formatProjectTimelineDate(tick.start)} – ${formatProjectTimelineDate(tick.end)}`;
  }
  return formatProjectTimelineDate(tick.start);
}
