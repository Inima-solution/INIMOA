import { describe, expect, it } from 'vitest';

import { isBusinessAuditListEnabled } from './business-audit';
import { businessAuditKeys } from './keys';

describe('business audit query keys', () => {
  it('separates current team, retention filter, and detail identity', () => {
    expect(businessAuditKeys.access('team-a').queryKey).toEqual([
      'businessAudit',
      'access',
      'team-a',
      'access',
    ]);
    expect(businessAuditKeys.list('team-a', 'standard').queryKey).not.toEqual(
      businessAuditKeys.list('team-a', 'restricted').queryKey
    );
    expect(businessAuditKeys.detail('team-a', 'event-a').queryKey).not.toEqual(
      businessAuditKeys.detail('team-a', 'event-b').queryKey
    );
  });
});

describe('business audit list enablement', () => {
  it('fails closed until the current team has confirmed read access', () => {
    expect(isBusinessAuditListEnabled(undefined, true)).toBe(false);
    expect(isBusinessAuditListEnabled('team-a', undefined)).toBe(false);
    expect(isBusinessAuditListEnabled('team-a', false)).toBe(false);
    expect(isBusinessAuditListEnabled('team-a', true)).toBe(true);
  });
});
