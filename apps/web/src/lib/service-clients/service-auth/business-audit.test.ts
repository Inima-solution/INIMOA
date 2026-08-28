import { platformFetch } from '@core/util/platformFetch';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { getMacroApiToken } from './fetch';

vi.mock('@core/util/platformFetch', () => ({ platformFetch: vi.fn() }));
vi.mock('./fetch', () => ({ getMacroApiToken: vi.fn() }));

import { businessAuditClient } from './business-audit';

const mockedPlatformFetch = vi.mocked(platformFetch);
const mockedGetMacroApiToken = vi.mocked(getMacroApiToken);

function response(body: unknown, init: ResponseInit = {}) {
  return new Response(JSON.stringify(body), {
    status: init.status ?? 200,
    headers: { 'content-type': 'application/json', ...init.headers },
  });
}

beforeEach(() => {
  mockedGetMacroApiToken.mockResolvedValue('test-token');
  mockedPlatformFetch.mockReset();
});

describe('businessAuditClient', () => {
  it('uses the auth service URL and bearer header for access', async () => {
    mockedPlatformFetch.mockResolvedValue(
      response({ can_read: true, can_export: false })
    );

    const result = await businessAuditClient.getAccess();

    expect(result._unsafeUnwrap()).toEqual({
      can_read: true,
      can_export: false,
    });
    expect(String(mockedPlatformFetch.mock.calls[0]?.[0])).toContain(
      '/team/business-audit/access'
    );
    expect(mockedPlatformFetch.mock.calls[0]?.[1]?.headers).toMatchObject({
      Authorization: 'Bearer test-token',
    });
  });

  it('keeps invalid passwords typed and maps MFA challenges to the safe outcome', async () => {
    mockedPlatformFetch.mockResolvedValueOnce(
      response(
        {
          code: 'invalid_credentials',
          message: 'provider secret must not leak',
        },
        { status: 401 }
      )
    );
    const invalid = await businessAuditClient.reauthenticatePassword('bad');
    expect(invalid._unsafeUnwrapErr()[0]).toEqual({
      code: 'INVALID_CREDENTIALS',
      message: 'Reauthentication failed.',
    });

    mockedPlatformFetch.mockResolvedValueOnce(
      response(
        {
          code: 'mfa_required',
          message: 'safe challenge',
          two_factor_id: 'challenge-id',
          methods: [{ id: 'method-id', method: 'authenticator' }],
        },
        { status: 401 }
      )
    );
    const challenge =
      await businessAuditClient.reauthenticatePassword('correct');
    expect(challenge._unsafeUnwrap()).toEqual({
      kind: 'mfa_required',
      twoFactorId: 'challenge-id',
      methods: [{ id: 'method-id', method: 'authenticator' }],
    });
  });

  it('sends the MFA request shape and keeps an invalid proof typed', async () => {
    mockedPlatformFetch.mockResolvedValue(
      response(
        { code: 'invalid_mfa', message: 'provider detail' },
        { status: 401 }
      )
    );

    const result = await businessAuditClient.reauthenticateMfa({
      twoFactorId: 'challenge-id',
      code: '123456',
    });

    expect(result._unsafeUnwrapErr()[0]).toEqual({
      code: 'INVALID_MFA',
      message: 'Multi-factor authentication failed.',
    });
    expect(mockedPlatformFetch.mock.calls[0]?.[1]?.body).toBe(
      JSON.stringify({ two_factor_id: 'challenge-id', code: '123456' })
    );
  });

  it('accepts only the bounded no-store CSV download contract', async () => {
    mockedPlatformFetch.mockResolvedValueOnce(
      new Response('id\r\n', {
        status: 200,
        headers: {
          'content-type': 'text/csv; charset=utf-8',
          'content-disposition': 'attachment; filename="business-audit.csv"',
          'cache-control': 'no-store',
          'content-length': '4',
        },
      })
    );
    const accepted = await businessAuditClient.exportCsv({
      reauthenticationReceipt: 'receipt',
      from: '2026-01-01T00:00:00Z',
      until: '2026-01-02T00:00:00Z',
      reason: 'audit',
    });
    expect(accepted._unsafeUnwrap()).toMatchObject({
      filename: 'business-audit.csv',
      contentType: 'text/csv; charset=utf-8',
    });

    mockedPlatformFetch.mockResolvedValueOnce(
      new Response('unsafe', {
        status: 200,
        headers: {
          'content-type': 'text/csv; charset=utf-8',
          'content-disposition': 'attachment; filename="business-audit.csv"',
          'cache-control': 'no-store',
          'content-length': String(8 * 1024 * 1024 + 1),
        },
      })
    );
    const rejected = await businessAuditClient.exportCsv({
      reauthenticationReceipt: 'receipt',
      from: '2026-01-01T00:00:00Z',
      until: '2026-01-02T00:00:00Z',
      reason: 'audit',
    });
    expect(rejected._unsafeUnwrapErr()[0]?.code).toBe('EXPORT_TOO_LARGE');
  });
});
