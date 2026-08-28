//! PostgreSQL keyset reads for the immutable business-audit ledger.

#[cfg(test)]
mod test;

use sqlx::{PgPool, postgres::PgTransaction};

use crate::domain::query::{
    AuditDetail, AuditDetailRequest, AuditExportRequest, AuditExportRow, AuditListError,
    AuditListItem, AuditListPage, AuditListRequest, AuditRetentionFilter, MAX_AUDIT_EXPORT_ROWS,
    decode_cursor, encode_cursor,
};

/// Reads one bounded, team-scoped page from the immutable ledger.
pub async fn list(
    pool: &PgPool,
    request: AuditListRequest,
) -> Result<AuditListPage, AuditListError> {
    let page_size = request.page_size();
    let retention_class = request.retention_class;
    let cursor = decode_cursor(request.cursor, retention_class)?;
    let filter = retention_class.map(AuditRetentionFilter::as_str);
    let cursor_at = cursor.as_ref().map(|cursor| cursor.occurred_at);
    let cursor_id = cursor.as_ref().map(|cursor| cursor.id);
    let rows = sqlx::query!(
        r#"
        SELECT id, actor, delegated_actor, action, target_type, target_id, outcome,
               occurred_at, retention_class
        FROM business_audit_events
        WHERE team_id = $1
          AND ($2::text IS NULL OR retention_class = $2)
          AND ($3::timestamptz IS NULL OR occurred_at < $3
               OR (occurred_at = $3 AND id < $4))
        ORDER BY occurred_at DESC, id DESC
        LIMIT $5
        "#,
        request.team_id,
        filter,
        cursor_at,
        cursor_id,
        i64::try_from(page_size + 1).expect("bounded page size fits i64"),
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "business audit ledger read failed");
        AuditListError::Storage
    })?;

    let mut items: Vec<AuditListItem> = rows
        .into_iter()
        .map(|row| AuditListItem {
            id: row.id,
            actor: row.actor,
            delegated_actor: row.delegated_actor,
            action: row.action,
            target_type: row.target_type,
            target_id: row.target_id,
            outcome: row.outcome,
            occurred_at: row.occurred_at,
            retention_class: match row.retention_class.as_str() {
                "standard" => AuditRetentionFilter::Standard,
                "confidential" => AuditRetentionFilter::Confidential,
                "restricted" => AuditRetentionFilter::Restricted,
                _ => unreachable!("database retention constraint is closed"),
            },
        })
        .collect();
    let has_next = items.len() > page_size;
    items.truncate(page_size);
    let next_cursor = has_next
        .then(|| {
            items
                .last()
                .map(|item| encode_cursor(item, retention_class))
        })
        .flatten();
    Ok(AuditListPage { items, next_cursor })
}

fn retention_class(value: String) -> AuditRetentionFilter {
    match value.as_str() {
        "standard" => AuditRetentionFilter::Standard,
        "confidential" => AuditRetentionFilter::Confidential,
        "restricted" => AuditRetentionFilter::Restricted,
        _ => unreachable!("database retention constraint is closed"),
    }
}

/// Returns one privileged fact scoped to the given team. Missing and
/// cross-team identifiers intentionally have the same `None` result.
pub async fn detail(
    pool: &PgPool,
    request: AuditDetailRequest,
) -> Result<Option<AuditDetail>, AuditListError> {
    let row = sqlx::query!(
        r#"
        SELECT id, actor, delegated_actor, action, target_type, target_id, outcome,
               occurred_at, request_id, reason, metadata, retention_class
        FROM business_audit_events
        WHERE team_id = $1 AND id = $2
        "#,
        request.team_id,
        request.id,
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "business audit detail read failed");
        AuditListError::Storage
    })?;
    Ok(row.map(|row| AuditDetail {
        id: row.id,
        actor: row.actor,
        delegated_actor: row.delegated_actor,
        action: row.action,
        target_type: row.target_type,
        target_id: row.target_id,
        outcome: row.outcome,
        occurred_at: row.occurred_at,
        request_id: row.request_id,
        reason: row.reason,
        metadata: row.metadata,
        retention_class: retention_class(row.retention_class),
    }))
}

/// Reads at most `MAX_AUDIT_EXPORT_ROWS + 1` full facts inside a caller-owned
/// transaction. The extra row lets the HTTP boundary reject oversized exports
/// without consuming their one-time receipt.
pub async fn export_with_tx(
    tx: &mut PgTransaction<'_>,
    request: &AuditExportRequest,
) -> Result<Vec<AuditExportRow>, AuditListError> {
    let retention_filter = request.retention_class.map(AuditRetentionFilter::as_str);
    let rows = sqlx::query!(
        r#"
        SELECT id, actor, delegated_actor, action, target_type, target_id, outcome,
               occurred_at, request_id, reason, metadata, retention_class
        FROM business_audit_events
        WHERE team_id = $1
          AND occurred_at >= $2
          AND occurred_at < $3
          AND ($4::text IS NULL OR retention_class = $4)
        ORDER BY occurred_at ASC, id ASC
        LIMIT $5
        "#,
        request.team_id,
        request.from,
        request.until,
        retention_filter,
        i64::try_from(MAX_AUDIT_EXPORT_ROWS + 1).expect("export bound fits i64"),
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "business audit export read failed");
        AuditListError::Storage
    })?;
    Ok(rows
        .into_iter()
        .map(|row| AuditExportRow {
            id: row.id,
            actor: row.actor,
            delegated_actor: row.delegated_actor,
            action: row.action,
            target_type: row.target_type,
            target_id: row.target_id,
            outcome: row.outcome,
            occurred_at: row.occurred_at,
            request_id: row.request_id,
            reason: row.reason,
            metadata: row.metadata,
            retention_class: retention_class(row.retention_class),
        })
        .collect())
}
