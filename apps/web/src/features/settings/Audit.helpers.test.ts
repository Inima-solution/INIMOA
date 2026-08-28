import { describe, expect, it } from 'vitest';
import {
  auditAffordances,
  auditSurfaceState,
  knownAuditMetadata,
  validateAuditExport,
} from './Audit.helpers';

describe('auditAffordances', () => {
  it('does not infer any list or detail for a member', () => {
    expect(auditAffordances({ canRead: false, canExport: false })).toEqual({
      showList: false,
      allowDetail: false,
      showExport: false,
    });
  });

  it('keeps auditor access list-only', () => {
    expect(auditAffordances({ canRead: true, canExport: false })).toEqual({
      showList: true,
      allowDetail: false,
      showExport: false,
    });
  });

  it('allows privileged detail and export only with export capability', () => {
    expect(auditAffordances({ canRead: true, canExport: true })).toEqual({
      showList: true,
      allowDetail: true,
      showExport: true,
    });
  });
});

describe('validateAuditExport', () => {
  it('uses an explicit UTC half-open range', () => {
    expect(
      validateAuditExport('Quarterly review', '2026-08-01', '2026-08-31')
    ).toEqual({
      valid: true,
      window: {
        fromUtc: '2026-08-01T00:00:00.000Z',
        toUtcExclusive: '2026-09-01T00:00:00.000Z',
      },
    });
  });

  it('rejects a missing reason and an overlong period', () => {
    expect(validateAuditExport('', '2026-08-01', '2026-09-01')).toEqual({
      valid: false,
      reasonError: 'Explain why you need this export.',
      dateError: 'Choose a period from 1 to 31 days.',
    });
  });

  it('rejects impossible dates and a reason above the server byte limit', () => {
    expect(
      validateAuditExport('review', '2026-02-30', '2026-03-01')
    ).toMatchObject({
      valid: false,
      dateError: 'Choose valid calendar dates.',
    });
    expect(
      validateAuditExport('a'.repeat(1001), '2026-08-01', '2026-08-01')
    ).toMatchObject({
      valid: false,
      reasonError: 'Keep the reason to 1,000 bytes or fewer.',
    });
  });
});

describe('auditSurfaceState', () => {
  it('prioritizes recoverable errors and offline before permission denial', () => {
    expect(
      auditSurfaceState({
        loading: false,
        online: false,
        teamError: false,
        accessError: false,
        canRead: false,
      })
    ).toBe('offline');
    expect(
      auditSurfaceState({
        loading: false,
        online: true,
        teamError: false,
        accessError: true,
        canRead: false,
      })
    ).toBe('access-error');
    expect(
      auditSurfaceState({
        loading: false,
        online: true,
        teamError: false,
        accessError: false,
        canRead: false,
      })
    ).toBe('permission-denied');
  });
});

describe('knownAuditMetadata', () => {
  it('renders only the reviewed role and export metadata fields', () => {
    expect(
      knownAuditMetadata({
        business_role: 'payroll_admin',
        grantee_principal: 'member-7',
        from: '2026-08-01T00:00:00.000Z',
        until: '2026-09-01T00:00:00.000Z',
        row_count: 42,
        password: 'never-render',
        unknown: 'never-render',
        nested: { secret: 'never-render' },
      })
    ).toEqual([
      { label: 'Business role', value: 'payroll_admin' },
      { label: 'Grantee', value: 'member-7' },
      { label: 'From', value: '2026-08-01T00:00:00.000Z' },
      { label: 'Until', value: '2026-09-01T00:00:00.000Z' },
      { label: 'Rows exported', value: 42 },
    ]);
  });
});
