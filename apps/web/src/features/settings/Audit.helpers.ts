export type AuditCapabilities = {
  canRead: boolean;
  canExport: boolean;
};

export type AuditAffordances = {
  showList: boolean;
  allowDetail: boolean;
  showExport: boolean;
};

/**
 * Keep privileged controls derived from the server capability projection.
 * In particular, an auditor can scan the minimal list without a detail query
 * or export control, and a member sees no list-derived information at all.
 */
export const auditAffordances = (
  capabilities: AuditCapabilities
): AuditAffordances => ({
  showList: capabilities.canRead,
  allowDetail: capabilities.canExport,
  showExport: capabilities.canExport,
});

export type AuditExportWindow = {
  fromUtc: string;
  toUtcExclusive: string;
};

export type AuditExportValidation =
  | { valid: true; window: AuditExportWindow }
  | { valid: false; reasonError?: string; dateError?: string };

const DATE_INPUT = /^\d{4}-\d{2}-\d{2}$/;
const MAX_EXPORT_DAYS = 31;
const MAX_EXPORT_REASON_BYTES = 1000;

export type AuditSurfaceState =
  | 'loading'
  | 'offline'
  | 'team-error'
  | 'access-error'
  | 'permission-denied'
  | 'ready';

/** Explicit state priority prevents a failed capability check looking denied. */
export const auditSurfaceState = (input: {
  loading: boolean;
  online: boolean;
  teamError: boolean;
  accessError: boolean;
  canRead: boolean;
}): AuditSurfaceState => {
  if (!input.online) return 'offline';
  if (input.loading) return 'loading';
  if (input.teamError) return 'team-error';
  if (input.accessError) return 'access-error';
  return input.canRead ? 'ready' : 'permission-denied';
};

/** Converts date-only inputs to the API's UTC half-open interval. */
export function validateAuditExport(
  reason: string,
  fromDate: string,
  toDate: string
): AuditExportValidation {
  const trimmedReason = reason.trim();
  const reasonError = !trimmedReason
    ? 'Explain why you need this export.'
    : new TextEncoder().encode(trimmedReason).byteLength >
        MAX_EXPORT_REASON_BYTES
      ? 'Keep the reason to 1,000 bytes or fewer.'
      : undefined;

  if (!DATE_INPUT.test(fromDate) || !DATE_INPUT.test(toDate)) {
    return {
      valid: false,
      ...(reasonError ? { reasonError } : {}),
      dateError: 'Choose a start and end date.',
    };
  }

  const from = new Date(`${fromDate}T00:00:00.000Z`);
  const inclusiveEnd = new Date(`${toDate}T00:00:00.000Z`);
  if (Number.isNaN(from.valueOf()) || Number.isNaN(inclusiveEnd.valueOf())) {
    return {
      valid: false,
      ...(reasonError ? { reasonError } : {}),
      dateError: 'Choose valid calendar dates.',
    };
  }
  if (
    from.toISOString().slice(0, 10) !== fromDate ||
    inclusiveEnd.toISOString().slice(0, 10) !== toDate
  ) {
    return {
      valid: false,
      ...(reasonError ? { reasonError } : {}),
      dateError: 'Choose valid calendar dates.',
    };
  }

  const toExclusive = new Date(inclusiveEnd);
  toExclusive.setUTCDate(toExclusive.getUTCDate() + 1);
  const days = (toExclusive.valueOf() - from.valueOf()) / 86_400_000;
  if (days <= 0 || days > MAX_EXPORT_DAYS) {
    return {
      valid: false,
      ...(reasonError ? { reasonError } : {}),
      dateError: 'Choose a period from 1 to 31 days.',
    };
  }

  if (reasonError) return { valid: false, reasonError };

  return {
    valid: true,
    window: {
      fromUtc: from.toISOString(),
      toUtcExclusive: toExclusive.toISOString(),
    },
  };
}

type MetadataValue = string | number | boolean;

/**
 * The detail UI intentionally accepts a small, reviewed projection. Unknown
 * values are never rendered as a fallback JSON blob.
 */
const METADATA_LABELS: Record<string, string> = {
  business_role: 'Business role',
  grantee_principal: 'Grantee',
  audit_event_id: 'Audit event ID',
  from: 'From',
  until: 'Until',
  retention_class: 'Retention class',
  row_count: 'Rows exported',
};

export const knownAuditMetadata = (
  metadata: Record<string, unknown> | null | undefined
): Array<{ label: string; value: MetadataValue }> => {
  if (!metadata) return [];

  return Object.entries(METADATA_LABELS).flatMap(([key, label]) => {
    const value = metadata[key];
    return typeof value === 'string' ||
      typeof value === 'number' ||
      typeof value === 'boolean'
      ? [{ label, value }]
      : [];
  });
};
