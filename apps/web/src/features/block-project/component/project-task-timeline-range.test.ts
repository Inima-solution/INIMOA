import { describe, expect, it } from 'vitest';
import {
  formatProjectTimelineTick,
  getInclusiveLocalCalendarDays,
  getLocalCalendarDayOrdinal,
  getProjectTimelineRange,
  getProjectTimelineRuler,
  getProjectTimelineToday,
  getProjectTimelineTodayPercent,
  PROJECT_TIMELINE_MAX_TICKS,
} from './project-task-timeline-range';

describe('project task timeline range', () => {
  it('accepts only complete, ordered local date-only ranges', () => {
    expect(getProjectTimelineRange(undefined, undefined).kind).toBe('not-set');
    expect(getProjectTimelineRange('2026-01-01', undefined).kind).toBe(
      'invalid'
    );
    expect(getProjectTimelineRange('invalid', '2026-01-01').kind).toBe(
      'invalid'
    );
    expect(getProjectTimelineRange('2026-01-02', '2026-01-01').kind).toBe(
      'invalid'
    );
    const equal = getProjectTimelineRange('2024-02-29', '2024-02-29');
    expect(equal.kind).toBe('valid');
  });

  it('creates clipped day, anchored-week, month, and quarter intervals', () => {
    const range = getProjectTimelineRange('2026-01-30', '2026-04-02');
    expect(range.kind).toBe('valid');
    if (range.kind !== 'valid') return;
    expect(getProjectTimelineRuler(range, 10_000).ticks).toHaveLength(63);
    expect(getProjectTimelineRuler(range, 4_000).scale).toBe('week');
    expect(getProjectTimelineRuler(range, 150).scale).toBe('quarter');
    expect(getProjectTimelineRuler(range, 0).scale).toBe('boundary');
  });

  it('uses an honest bounded fallback and reports today relation', () => {
    const range = getProjectTimelineRange('2020-01-01', '2040-12-31');
    expect(range.kind).toBe('valid');
    if (range.kind !== 'valid') return;
    const ruler = getProjectTimelineRuler(range, 20_000);
    expect(ruler.scale).toBe('boundary');
    expect(ruler.ticks.length).toBeLessThanOrEqual(PROJECT_TIMELINE_MAX_TICKS);
    expect(
      getProjectTimelineToday(range, new Date(2019, 11, 31)).relation
    ).toBe('before');
    expect(getProjectTimelineToday(range, new Date(2026, 0, 1)).relation).toBe(
      'inside'
    );
    expect(getProjectTimelineToday(range, new Date(2041, 0, 1)).relation).toBe(
      'after'
    );
  });

  it('advances calendar ordinals across a DST boundary without elapsed-hour math', () => {
    expect(
      getLocalCalendarDayOrdinal(new Date(2026, 2, 9)) -
        getLocalCalendarDayOrdinal(new Date(2026, 2, 8))
    ).toBe(1);
  });

  it('defensively formats a boundary tick without inventing an interval', () => {
    const date = new Date(2026, 0, 1);
    expect(
      formatProjectTimelineTick({ start: date, end: date }, 'boundary')
    ).toContain('2026');
  });

  it('uses inclusive local-day spans and centered Today positions', () => {
    const range = getProjectTimelineRange('2026-01-01', '2026-04-02');
    expect(range.kind).toBe('valid');
    if (range.kind !== 'valid') return;
    expect(
      getInclusiveLocalCalendarDays(range.start, new Date(2026, 2, 31))
    ).toBe(90);
    expect(
      getProjectTimelineTodayPercent(range, new Date(2026, 0, 1))
    ).toBeCloseTo((0.5 / 92) * 100);
  });

  it('keeps local-day spans and Today centering stable across DST', () => {
    const range = getProjectTimelineRange('2026-03-07', '2026-03-10');
    expect(range.kind).toBe('valid');
    if (range.kind !== 'valid') return;
    expect(getInclusiveLocalCalendarDays(range.start, range.end)).toBe(4);
    expect(getProjectTimelineTodayPercent(range, new Date(2026, 2, 9))).toBe(
      62.5
    );
  });
});
