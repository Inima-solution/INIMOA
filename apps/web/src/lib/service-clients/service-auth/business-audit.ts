import { SERVER_HOSTS } from '@core/constant/servers';
import { platformFetch } from '@core/util/platformFetch';
import type { ResultError } from '@core/util/result';
import { err, ok, type Result } from 'neverthrow';
import { getMacroApiToken } from './fetch';
import {
  getCompleteTeamBusinessAuditExportReauthenticationMfaUrl,
  getExportTeamBusinessAuditUrl,
  getGetTeamBusinessAuditAccessUrl,
  getGetTeamBusinessAuditDetailUrl,
  getListTeamBusinessAuditUrl,
  getReauthenticateForTeamBusinessAuditExportUrl,
} from './generated/client';
import type { BusinessAuditAccessResponse } from './generated/schemas/businessAuditAccessResponse';
import type { BusinessAuditDetailResponse } from './generated/schemas/businessAuditDetailResponse';
import type { BusinessAuditListItem } from './generated/schemas/businessAuditListItem';
import type { BusinessAuditListResponse } from './generated/schemas/businessAuditListResponse';
import type { BusinessAuditRetentionFilter } from './generated/schemas/businessAuditRetentionFilter';
import type { ReauthenticateMfaMethod } from './generated/schemas/reauthenticateMfaMethod';

const authHost = SERVER_HOSTS['auth-service'];
const MAX_AUDIT_EXPORT_BYTES = 8 * 1024 * 1024;

export type {
  BusinessAuditAccessResponse,
  BusinessAuditDetailResponse,
  BusinessAuditListResponse,
  BusinessAuditRetentionFilter,
};

export type BusinessAuditClientErrorCode =
  | 'UNAUTHORIZED'
  | 'FORBIDDEN'
  | 'NOT_FOUND'
  | 'INVALID_REQUEST'
  | 'INVALID_CREDENTIALS'
  | 'INVALID_MFA'
  | 'RATE_LIMITED'
  | 'UPSTREAM_UNAVAILABLE'
  | 'CONFLICT'
  | 'EXPORT_TOO_LARGE'
  | 'SERVER_ERROR'
  | 'NETWORK_ERROR';

export type BusinessAuditReauthenticationOutcome =
  | {
      kind: 'receipt';
      reauthenticationReceipt: string;
      expiresIn: number;
    }
  | {
      kind: 'mfa_required';
      twoFactorId: string;
      methods: readonly { id: string; method: string }[];
    };

export type BusinessAuditExportInput = {
  reauthenticationReceipt: string;
  from: string;
  until: string;
  retentionClass?: BusinessAuditRetentionFilter;
  reason: string;
};

export type BusinessAuditCsvDownload = {
  bytes: Uint8Array;
  filename: 'business-audit.csv';
  contentType: 'text/csv; charset=utf-8';
};

type BusinessAuditResult<T> = Result<
  T,
  ResultError<BusinessAuditClientErrorCode>[]
>;

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null;
}

function hasString(
  value: JsonRecord,
  key: string
): value is JsonRecord & Record<string, string> {
  return typeof value[key] === 'string';
}

function safeError(status: number): ResultError<BusinessAuditClientErrorCode> {
  switch (status) {
    case 400:
      return {
        code: 'INVALID_REQUEST',
        message: 'The audit request is invalid.',
      };
    case 401:
      return { code: 'UNAUTHORIZED', message: 'Authentication is required.' };
    case 403:
      return { code: 'FORBIDDEN', message: 'You do not have audit access.' };
    case 404:
      return { code: 'NOT_FOUND', message: 'The audit record was not found.' };
    case 409:
      return {
        code: 'CONFLICT',
        message: 'The audit export receipt is no longer valid.',
      };
    case 413:
      return {
        code: 'EXPORT_TOO_LARGE',
        message: 'The audit export exceeds its size limit.',
      };
    case 429:
      return {
        code: 'RATE_LIMITED',
        message: 'Too many reauthentication attempts.',
      };
    case 502:
      return {
        code: 'UPSTREAM_UNAVAILABLE',
        message: 'Reauthentication is temporarily unavailable.',
      };
    default:
      return {
        code: 'SERVER_ERROR',
        message: 'The audit service could not complete the request.',
      };
  }
}

async function authenticatedFetch(
  url: string,
  init?: RequestInit
): Promise<BusinessAuditResult<Response>> {
  let token: string;
  try {
    token = await getMacroApiToken();
  } catch {
    return err([
      { code: 'UNAUTHORIZED', message: 'Authentication is required.' },
    ]);
  }
  if (!token) {
    return err([
      { code: 'UNAUTHORIZED', message: 'Authentication is required.' },
    ]);
  }
  try {
    return ok(
      await platformFetch(`${authHost}${url}`, {
        ...init,
        credentials: 'include',
        headers: {
          Accept: 'application/json',
          Authorization: `Bearer ${token}`,
          ...init?.headers,
        },
      })
    );
  } catch {
    return err([
      { code: 'NETWORK_ERROR', message: 'The audit service is unavailable.' },
    ]);
  }
}

async function readJson(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return null;
  }
}

function mfaOutcome(
  value: unknown
): BusinessAuditReauthenticationOutcome | null {
  if (!isRecord(value) || value.code !== 'mfa_required') return null;
  if (!hasString(value, 'two_factor_id') || !Array.isArray(value.methods))
    return null;
  const methods = value.methods.filter(
    (method): method is ReauthenticateMfaMethod =>
      isRecord(method) && hasString(method, 'id') && hasString(method, 'method')
  );
  if (methods.length !== value.methods.length) return null;
  return {
    kind: 'mfa_required',
    twoFactorId: value.two_factor_id,
    methods: methods.map(({ id, method }) => ({ id, method })),
  };
}

function receiptOutcome(
  value: unknown
): BusinessAuditReauthenticationOutcome | null {
  if (!isRecord(value) || !hasString(value, 'reauthentication_receipt'))
    return null;
  if (typeof value.expires_in !== 'number') return null;
  return {
    kind: 'receipt',
    reauthenticationReceipt: value.reauthentication_receipt,
    expiresIn: value.expires_in,
  };
}

function listItem(value: unknown): BusinessAuditListItem | null {
  if (!isRecord(value)) return null;
  const id = value.id;
  const action = value.action;
  const targetType = value.target_type;
  const targetId = value.target_id;
  const actor = value.actor;
  const outcome = value.outcome;
  const occurredAt = value.occurred_at;
  const retentionClass = value.retention_class;
  if (
    typeof id !== 'string' ||
    typeof action !== 'string' ||
    typeof targetType !== 'string' ||
    typeof targetId !== 'string' ||
    typeof actor !== 'string' ||
    typeof outcome !== 'string' ||
    typeof occurredAt !== 'string' ||
    typeof retentionClass !== 'string'
  )
    return null;
  const delegatedActor = value.delegated_actor;
  if (
    delegatedActor !== undefined &&
    delegatedActor !== null &&
    typeof delegatedActor !== 'string'
  ) {
    return null;
  }
  return {
    id,
    action,
    target_type: targetType,
    target_id: targetId,
    actor,
    delegated_actor: delegatedActor,
    outcome,
    occurred_at: occurredAt,
    retention_class: retentionClass,
  };
}

function listResponse(value: unknown): BusinessAuditListResponse | null {
  if (!isRecord(value) || !Array.isArray(value.items)) return null;
  const items: BusinessAuditListItem[] = [];
  for (const itemValue of value.items) {
    const item = listItem(itemValue);
    if (!item) return null;
    items.push(item);
  }
  const nextCursor = value.next_cursor;
  if (
    nextCursor !== undefined &&
    nextCursor !== null &&
    typeof nextCursor !== 'string'
  ) {
    return null;
  }
  return { items, next_cursor: nextCursor };
}

function detailResponse(value: unknown): BusinessAuditDetailResponse | null {
  const item = listItem(value);
  if (!item || !isRecord(value) || !hasString(value, 'request_id')) return null;
  if (
    value.reason !== undefined &&
    value.reason !== null &&
    typeof value.reason !== 'string'
  ) {
    return null;
  }
  if (!('metadata' in value)) return null;
  const reason = value.reason;
  return {
    ...item,
    request_id: value.request_id,
    reason,
    metadata: value.metadata,
  };
}

export const businessAuditClient = {
  async getAccess(): Promise<BusinessAuditResult<BusinessAuditAccessResponse>> {
    const response = await authenticatedFetch(
      getGetTeamBusinessAuditAccessUrl()
    );
    if (response.isErr()) return err(response.error);
    if (!response.value.ok) return err([safeError(response.value.status)]);
    const data = await readJson(response.value);
    if (
      !isRecord(data) ||
      typeof data.can_read !== 'boolean' ||
      typeof data.can_export !== 'boolean'
    ) {
      return err([safeError(500)]);
    }
    return ok({ can_read: data.can_read, can_export: data.can_export });
  },

  async list(input: {
    cursor?: string;
    retentionClass?: BusinessAuditRetentionFilter;
  }): Promise<BusinessAuditResult<BusinessAuditListResponse>> {
    const response = await authenticatedFetch(
      getListTeamBusinessAuditUrl({
        cursor: input.cursor,
        retention_class: input.retentionClass,
        limit: 50,
      })
    );
    if (response.isErr()) return err(response.error);
    if (!response.value.ok) return err([safeError(response.value.status)]);
    const data = await readJson(response.value);
    const parsed = listResponse(data);
    return parsed ? ok(parsed) : err([safeError(500)]);
  },

  async getDetail(
    id: string
  ): Promise<BusinessAuditResult<BusinessAuditDetailResponse>> {
    const response = await authenticatedFetch(
      getGetTeamBusinessAuditDetailUrl(id)
    );
    if (response.isErr()) return err(response.error);
    if (!response.value.ok) return err([safeError(response.value.status)]);
    const data = await readJson(response.value);
    const parsed = detailResponse(data);
    return parsed ? ok(parsed) : err([safeError(500)]);
  },

  async reauthenticatePassword(
    password: string
  ): Promise<BusinessAuditResult<BusinessAuditReauthenticationOutcome>> {
    const response = await authenticatedFetch(
      getReauthenticateForTeamBusinessAuditExportUrl(),
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ password }),
      }
    );
    if (response.isErr()) return err(response.error);
    const data = await readJson(response.value);
    if (response.value.status === 401) {
      const challenge = mfaOutcome(data);
      if (challenge) return ok(challenge);
      return err([
        { code: 'INVALID_CREDENTIALS', message: 'Reauthentication failed.' },
      ]);
    }
    if (!response.value.ok) return err([safeError(response.value.status)]);
    const receipt = receiptOutcome(data);
    return receipt ? ok(receipt) : err([safeError(500)]);
  },

  async reauthenticateMfa(input: {
    twoFactorId: string;
    code: string;
  }): Promise<BusinessAuditResult<BusinessAuditReauthenticationOutcome>> {
    const response = await authenticatedFetch(
      getCompleteTeamBusinessAuditExportReauthenticationMfaUrl(),
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          two_factor_id: input.twoFactorId,
          code: input.code,
        }),
      }
    );
    if (response.isErr()) return err(response.error);
    if (response.value.status === 401) {
      return err([
        { code: 'INVALID_MFA', message: 'Multi-factor authentication failed.' },
      ]);
    }
    if (!response.value.ok) return err([safeError(response.value.status)]);
    const receipt = receiptOutcome(await readJson(response.value));
    return receipt ? ok(receipt) : err([safeError(500)]);
  },

  async exportCsv(
    input: BusinessAuditExportInput
  ): Promise<BusinessAuditResult<BusinessAuditCsvDownload>> {
    const response = await authenticatedFetch(getExportTeamBusinessAuditUrl(), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'text/csv' },
      body: JSON.stringify({
        reauthentication_receipt: input.reauthenticationReceipt,
        from: input.from,
        until: input.until,
        retention_class: input.retentionClass,
        reason: input.reason,
      }),
    });
    if (response.isErr()) return err(response.error);
    if (!response.value.ok) return err([safeError(response.value.status)]);
    const contentType = response.value.headers.get('content-type');
    const contentDisposition = response.value.headers.get(
      'content-disposition'
    );
    const cacheControl = response.value.headers.get('cache-control');
    const contentLength = response.value.headers.get('content-length');
    if (
      contentType !== 'text/csv; charset=utf-8' ||
      contentDisposition !== 'attachment; filename="business-audit.csv"' ||
      cacheControl !== 'no-store'
    ) {
      return err([safeError(500)]);
    }
    if (contentLength !== null) {
      if (!/^\d+$/.test(contentLength)) return err([safeError(500)]);
      if (Number(contentLength) > MAX_AUDIT_EXPORT_BYTES)
        return err([safeError(413)]);
    }
    let bytes: Uint8Array;
    try {
      bytes = new Uint8Array(await response.value.arrayBuffer());
    } catch {
      return err([
        { code: 'NETWORK_ERROR', message: 'The audit service is unavailable.' },
      ]);
    }
    if (bytes.byteLength > MAX_AUDIT_EXPORT_BYTES) return err([safeError(413)]);
    return ok({
      bytes,
      filename: 'business-audit.csv',
      contentType: 'text/csv; charset=utf-8',
    });
  },
};
