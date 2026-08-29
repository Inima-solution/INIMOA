export type DueDateBucket = 'overdue' | 'today' | 'upcoming' | 'no-due';

export type ResolvedDueDateRange = {
  gt?: string;
  gte?: string;
  lt?: string;
  lte?: string;
  exclude?: boolean;
};

/**
 * Resolves a user-facing Due Date bucket against local calendar days. Creating
 * the boundaries from local calendar components preserves midnight and DST
 * transitions before the instants are serialized to UTC for backend matching.
 */
export function resolveDueDateBucket(
  bucket: DueDateBucket,
  now = new Date()
): ResolvedDueDateRange {
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const tomorrowStart = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1
  );

  switch (bucket) {
    case 'overdue':
      return { lt: todayStart.toISOString() };
    case 'today':
      return {
        gte: todayStart.toISOString(),
        lt: tomorrowStart.toISOString(),
      };
    case 'upcoming':
      return { gte: tomorrowStart.toISOString() };
    case 'no-due':
      return { exclude: true };
  }
}
