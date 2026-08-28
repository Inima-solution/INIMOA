//! Direct-human, team-scoped reads from the immutable business-audit ledger.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use business_audit::{
    Actor, AuditAction, AuditDetailReadMetadata, AuditEvent, AuditExportRequest, AuditExportRow,
    AuditExportedMetadata, AuditListError, AuditListRequest, AuditReason, AuditRetentionFilter,
    AuditTarget, RequestCorrelationId, RetentionClass, detail, export_with_tx, insert_with_tx,
    list,
};
use chrono::{DateTime, Duration, Utc};
use entity_access::{
    domain::models::{ExportAuditBusiness, ReadAuditBusiness},
    inbound::axum_extractors::{MacroUserTeamExtractorV2, OptionalMacroUserTeamExtractorV2},
};
use macro_authorization::{MacroAuthorizationExtractor, UserOnly};
use macro_user_id::user_id::MacroUserIdStr;
use model::response::ErrorResponse;
use reauthentication::{PgReauthenticationReceiptRepo, ReceiptPurpose, ReceiptScope};
use std::borrow::Cow;
use tower_http::request_id::RequestId;
use uuid::Uuid;

use super::context::{ApiContext, AuthorizationService, EntityAccessServiceType};
use super::reauth::{
    ReauthenticateError, ReauthenticateMfaRequest, ReauthenticateMfaUnauthorizedResponse,
    ReauthenticateRequest, ReauthenticateResponse, ReauthenticateUnauthorizedResponse,
    reauthenticate_mfa_for_purpose, reauthenticate_password_for_purpose,
};

const MAX_AUDIT_EXPORT_DAYS: i64 = 31;
const MAX_AUDIT_EXPORT_BYTES: usize = 8 * 1024 * 1024;

/// Query parameters for a bounded business-audit page.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct BusinessAuditQuery {
    /// Opaque keyset cursor returned by the previous page.
    pub cursor: Option<String>,
    /// Optional closed retention-class filter.
    pub retention_class: Option<BusinessAuditRetentionFilter>,
    /// Requested page size, clamped into the service boundary.
    pub limit: Option<usize>,
}

/// Closed retention-class query vocabulary.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BusinessAuditRetentionFilter {
    /// Internal operational facts.
    Standard,
    /// Confidential people or approval facts.
    Confidential,
    /// Restricted high-risk facts.
    Restricted,
}

impl From<BusinessAuditRetentionFilter> for AuditRetentionFilter {
    fn from(value: BusinessAuditRetentionFilter) -> Self {
        match value {
            BusinessAuditRetentionFilter::Standard => Self::Standard,
            BusinessAuditRetentionFilter::Confidential => Self::Confidential,
            BusinessAuditRetentionFilter::Restricted => Self::Restricted,
        }
    }
}

/// One approved audit-list item. It intentionally excludes reason, request
/// correlation, receipt material, metadata, and any count.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct BusinessAuditListItem {
    /// Immutable event identity.
    pub id: Uuid,
    /// Stored action tag.
    pub action: String,
    /// Stored target kind.
    pub target_type: String,
    /// Canonical target identity.
    pub target_id: String,
    /// Mechanical actor principal.
    pub actor: String,
    /// Optional initiating human principal.
    pub delegated_actor: Option<String>,
    /// Stored outcome tag.
    pub outcome: String,
    /// Durable event time.
    pub occurred_at: DateTime<Utc>,
    /// Closed retention classification.
    pub retention_class: String,
}

/// Cursor page of approved immutable audit-list items.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct BusinessAuditListResponse {
    /// Audit rows for this page.
    pub items: Vec<BusinessAuditListItem>,
    /// Opaque next page cursor, absent on the final page.
    pub next_cursor: Option<String>,
}

/// The caller's narrowly-scoped business-audit capabilities for their current
/// team. This endpoint intentionally does not disclose the team, role, or
/// access receipt used to derive these booleans.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct BusinessAuditAccessResponse {
    /// Whether the direct human may read the team's audit ledger.
    pub can_read: bool,
    /// Whether the direct human may export the team's audit ledger.
    pub can_export: bool,
}

/// Privileged detail projection for one immutable audit fact.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct BusinessAuditDetailResponse {
    /// Immutable event identity.
    pub id: Uuid,
    /// Stored action tag.
    pub action: String,
    /// Stored target kind.
    pub target_type: String,
    /// Canonical target identity.
    pub target_id: String,
    /// Mechanical actor principal.
    pub actor: String,
    /// Optional initiating human principal.
    pub delegated_actor: Option<String>,
    /// Stored outcome tag.
    pub outcome: String,
    /// Durable event time.
    pub occurred_at: DateTime<Utc>,
    /// Correlation identifier for this immutable fact.
    pub request_id: String,
    /// Optional human rationale.
    pub reason: Option<String>,
    /// Fixed safe action metadata.
    pub metadata: serde_json::Value,
    /// Closed retention classification.
    pub retention_class: String,
}

/// Input for a bounded, privileged audit CSV export.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct BusinessAuditExportRequest {
    /// One-time reauthentication receipt scoped to this export purpose.
    reauthentication_receipt: Uuid,
    /// Inclusive UTC export start.
    from: DateTime<Utc>,
    /// Exclusive UTC export end.
    until: DateTime<Utc>,
    /// Optional closed retention-class filter.
    retention_class: Option<BusinessAuditRetentionFilter>,
    /// Human rationale recorded on the successful export audit fact.
    reason: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BusinessAuditError {
    /// Cursor syntax or request-filter binding was invalid.
    #[error("invalid audit cursor")]
    InvalidCursor,
    /// Direct authentication and the team access receipt disagreed.
    #[error("direct user authentication required")]
    PrincipalMismatch,
    /// A privileged detail identifier is missing or outside the caller's team.
    #[error("audit record not found")]
    NotFound,
    /// A bounded export request did not satisfy the closed input contract.
    #[error("invalid audit export request")]
    InvalidExport,
    /// The one-time receipt was absent, expired, consumed, or differently scoped.
    #[error("invalid audit export receipt")]
    InvalidReceipt,
    /// The requested export has too many rows or exceeds the response byte bound.
    #[error("audit export exceeds its maximum size")]
    ExportTooLarge,
    /// Receipt or database state could not safely serve the ledger.
    #[error("internal server error")]
    Internal,
}

impl IntoResponse for BusinessAuditError {
    fn into_response(self) -> Response {
        let (status, message): (StatusCode, Cow<'static, str>) = match self {
            Self::InvalidCursor => (StatusCode::BAD_REQUEST, "invalid audit cursor".into()),
            Self::PrincipalMismatch => (
                StatusCode::FORBIDDEN,
                "direct user authentication required".into(),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "audit record not found".into()),
            Self::InvalidExport => (
                StatusCode::BAD_REQUEST,
                "invalid audit export request".into(),
            ),
            Self::InvalidReceipt => (StatusCode::CONFLICT, "invalid audit export receipt".into()),
            Self::ExportTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "audit export exceeds its maximum size".into(),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".into(),
            ),
        };
        (status, Json(ErrorResponse { message })).into_response()
    }
}

/// Builds the team-scoped business-audit read route.
pub fn router() -> Router<ApiContext> {
    Router::new()
        .route("/business-audit/access", get(access_handler))
        .route("/business-audit", get(handler))
        .route("/business-audit/{id}", get(detail_handler))
        .route("/business-audit/reauth", post(reauth_handler))
        .route("/business-audit/reauth/mfa", post(reauth_mfa_handler))
        .route("/business-audit/export", post(export_handler))
}

fn matching_team_receipt<T>(
    receipt: Option<entity_access::domain::models::EntityAccessReceipt<T>>,
    principal: &MacroUserIdStr<'_>,
    team_id: Option<Uuid>,
) -> Result<Option<Uuid>, BusinessAuditError>
where
    T: entity_access::inbound::axum_extractors::RequiredPermission,
{
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    let receipt_principal = receipt
        .get_authenticated_user()
        .map_err(|_| BusinessAuditError::PrincipalMismatch)?;
    if receipt_principal != principal {
        return Err(BusinessAuditError::PrincipalMismatch);
    }
    let receipt_team_id = receipt
        .entity()
        .entity_id
        .parse::<Uuid>()
        .map_err(|_| BusinessAuditError::Internal)?;
    if team_id.is_some_and(|expected| expected != receipt_team_id) {
        return Err(BusinessAuditError::PrincipalMismatch);
    }
    Ok(Some(receipt_team_id))
}

fn business_audit_access_response(
    read_team_id: Option<Uuid>,
    export_team_id: Option<Uuid>,
) -> Result<BusinessAuditAccessResponse, BusinessAuditError> {
    if export_team_id.is_some() && read_team_id.is_none() {
        return Err(BusinessAuditError::PrincipalMismatch);
    }
    if let (Some(read_team_id), Some(export_team_id)) = (read_team_id, export_team_id)
        && read_team_id != export_team_id
    {
        return Err(BusinessAuditError::PrincipalMismatch);
    }
    Ok(BusinessAuditAccessResponse {
        can_read: read_team_id.is_some(),
        can_export: export_team_id.is_some(),
    })
}

/// Returns only the direct human's audit capabilities for their current team.
#[utoipa::path(
    get,
    operation_id = "get_team_business_audit_access",
    path = "/team/business-audit/access",
    responses(
        (status = 200, body = BusinessAuditAccessResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn access_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    read_access: OptionalMacroUserTeamExtractorV2<
        ReadAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
    export_access: OptionalMacroUserTeamExtractorV2<
        ExportAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
) -> Result<Json<BusinessAuditAccessResponse>, BusinessAuditError> {
    let principal = user.authorization.macro_user_id;
    let read_team_id = matching_team_receipt(read_access.entity_access_receipt, &principal, None)?;
    let export_team_id = matching_team_receipt(
        export_access.entity_access_receipt,
        &principal,
        read_team_id,
    )?;
    Ok(Json(business_audit_access_response(
        read_team_id,
        export_team_id,
    )?))
}

fn export_scope(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<
        ExportAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
) -> Result<(Uuid, MacroUserIdStr<'static>), BusinessAuditError> {
    let access_principal = access
        .entity_access_receipt
        .get_authenticated_user()
        .map_err(|_| BusinessAuditError::PrincipalMismatch)?;
    let principal = user.authorization.macro_user_id;
    if access_principal != &principal {
        return Err(BusinessAuditError::PrincipalMismatch);
    }
    let team_id = access
        .entity_access_receipt
        .entity()
        .entity_id
        .parse::<Uuid>()
        .map_err(|_| BusinessAuditError::Internal)?;
    Ok((team_id, principal))
}

fn request_correlation_id(
    request_id: RequestId,
) -> Result<RequestCorrelationId, BusinessAuditError> {
    let value = request_id
        .header_value()
        .to_str()
        .map_err(|_| BusinessAuditError::Internal)?;
    RequestCorrelationId::try_new(value).map_err(|_| BusinessAuditError::Internal)
}

fn valid_export_request(request: &BusinessAuditExportRequest) -> bool {
    request.from < request.until
        && request.until - request.from <= Duration::days(MAX_AUDIT_EXPORT_DAYS)
}

fn csv_cell(value: &str) -> String {
    let needs_neutralization = matches!(
        value.as_bytes().first(),
        Some(b'=' | b'+' | b'-' | b'@' | b'\t' | b'\r' | b'\n')
    );
    let value = if needs_neutralization {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn fixed_safe_metadata(action: &str, metadata: &serde_json::Value) -> Option<serde_json::Value> {
    let object = metadata.as_object()?;
    let only = |keys: &[&str]| object.keys().all(|key| keys.contains(&key.as_str()));
    match action {
        "role_granted" | "role_revoked" if only(&["business_role", "grantee_principal"]) => {
            Some(serde_json::json!({
                "business_role": object.get("business_role")?.as_str()?,
                "grantee_principal": object.get("grantee_principal")?.as_str()?,
            }))
        }
        "audit_detail_read" if only(&["audit_event_id"]) => Some(serde_json::json!({
            "audit_event_id": object.get("audit_event_id")?.as_str()?,
        })),
        "audit_exported" if only(&["from", "until", "retention_class", "row_count"]) => {
            let retention_class = object.get("retention_class")?;
            if !(retention_class.is_null() || retention_class.is_string()) {
                return None;
            }
            Some(serde_json::json!({
                "from": object.get("from")?.as_str()?,
                "until": object.get("until")?.as_str()?,
                "retention_class": retention_class,
                "row_count": object.get("row_count")?.as_u64()?,
            }))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsvRenderError {
    TooLarge,
    UnsafeMetadata,
}

fn push_csv_record(
    bytes: &mut Vec<u8>,
    fields: impl IntoIterator<Item = String>,
) -> Result<(), CsvRenderError> {
    let record = fields
        .into_iter()
        .map(|field| csv_cell(&field))
        .collect::<Vec<_>>()
        .join(",");
    let next_len = bytes.len().saturating_add(record.len()).saturating_add(2);
    if next_len > MAX_AUDIT_EXPORT_BYTES {
        return Err(CsvRenderError::TooLarge);
    }
    bytes.extend_from_slice(record.as_bytes());
    bytes.extend_from_slice(b"\r\n");
    Ok(())
}

fn render_csv(rows: &[AuditExportRow]) -> Result<Vec<u8>, CsvRenderError> {
    let mut bytes = Vec::new();
    push_csv_record(
        &mut bytes,
        [
            "id",
            "actor",
            "delegated_actor",
            "action",
            "target_type",
            "target_id",
            "outcome",
            "occurred_at",
            "request_id",
            "reason",
            "metadata",
            "retention_class",
        ]
        .into_iter()
        .map(str::to_owned),
    )?;
    for row in rows {
        let metadata = fixed_safe_metadata(&row.action, &row.metadata)
            .ok_or(CsvRenderError::UnsafeMetadata)?;
        push_csv_record(
            &mut bytes,
            [
                row.id.to_string(),
                row.actor.clone(),
                row.delegated_actor.clone().unwrap_or_default(),
                row.action.clone(),
                row.target_type.clone(),
                row.target_id.clone(),
                row.outcome.clone(),
                row.occurred_at.to_rfc3339(),
                row.request_id.clone(),
                row.reason.clone().unwrap_or_default(),
                metadata.to_string(),
                row.retention_class.as_str().to_owned(),
            ],
        )?;
    }
    Ok(bytes)
}

fn csv_download_response(csv: Vec<u8>) -> Response {
    let mut response = Response::new(axum::body::Body::from(csv));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"business-audit.csv\""),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Lists immutable business-audit facts for the receipt's team.
#[utoipa::path(
    get,
    operation_id = "list_team_business_audit",
    path = "/team/business-audit",
    params(BusinessAuditQuery),
    responses(
        (status = 200, body = BusinessAuditListResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<
        ReadAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
    State(ctx): State<ApiContext>,
    Query(query): Query<BusinessAuditQuery>,
) -> Result<Json<BusinessAuditListResponse>, BusinessAuditError> {
    let actor = user.authorization.macro_user_id;
    let access_principal = access
        .entity_access_receipt
        .get_authenticated_user()
        .map_err(|_| BusinessAuditError::PrincipalMismatch)?;
    if access_principal != &actor {
        return Err(BusinessAuditError::PrincipalMismatch);
    }
    let team_id = access
        .entity_access_receipt
        .entity()
        .entity_id
        .parse::<Uuid>()
        .map_err(|_| BusinessAuditError::Internal)?;
    let page = list(
        &ctx.db,
        AuditListRequest {
            team_id,
            cursor: query.cursor,
            retention_class: query.retention_class.map(Into::into),
            limit: query.limit,
        },
    )
    .await
    .map_err(|error| match error {
        AuditListError::InvalidCursor => BusinessAuditError::InvalidCursor,
        AuditListError::Storage => BusinessAuditError::Internal,
    })?;
    Ok(Json(BusinessAuditListResponse {
        items: page
            .items
            .into_iter()
            .map(|item| BusinessAuditListItem {
                id: item.id,
                action: item.action,
                target_type: item.target_type,
                target_id: item.target_id,
                actor: item.actor,
                delegated_actor: item.delegated_actor,
                outcome: item.outcome,
                occurred_at: item.occurred_at,
                retention_class: item.retention_class.as_str().to_owned(),
            })
            .collect(),
        next_cursor: page.next_cursor,
    }))
}

/// Returns privileged detail for one immutable fact and records the successful
/// read in the same ledger without exposing the newly written fact.
#[utoipa::path(
    get,
    operation_id = "get_team_business_audit_detail",
    path = "/team/business-audit/{id}",
    params(("id" = Uuid, Path, description = "Immutable audit event identifier")),
    responses(
        (status = 200, body = BusinessAuditDetailResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn detail_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<
        ExportAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<BusinessAuditDetailResponse>, BusinessAuditError> {
    let (team_id, principal) = export_scope(user, access)?;
    let detail = detail(&ctx.db, business_audit::AuditDetailRequest { team_id, id })
        .await
        .map_err(|_| BusinessAuditError::Internal)?
        .ok_or(BusinessAuditError::NotFound)?;
    let metadata = fixed_safe_metadata(&detail.action, &detail.metadata)
        .ok_or(BusinessAuditError::Internal)?;
    let response = BusinessAuditDetailResponse {
        id: detail.id,
        action: detail.action,
        target_type: detail.target_type,
        target_id: detail.target_id,
        actor: detail.actor,
        delegated_actor: detail.delegated_actor,
        outcome: detail.outcome,
        occurred_at: detail.occurred_at,
        request_id: detail.request_id,
        reason: detail.reason,
        metadata,
        retention_class: detail.retention_class.as_str().to_owned(),
    };
    let event = AuditEvent::new(
        team_id,
        Actor::new_from_user(principal),
        None,
        AuditAction::DetailRead(AuditDetailReadMetadata::new(id)),
        AuditTarget::Team(team_id),
        business_audit::AuditOutcome::Success,
        Utc::now(),
        request_correlation_id(request_id)?,
        None,
        RetentionClass::Confidential,
    )
    .map_err(|_| BusinessAuditError::Internal)?;
    let mut tx = ctx
        .db
        .begin()
        .await
        .map_err(|_| BusinessAuditError::Internal)?;
    insert_with_tx(&mut tx, &event)
        .await
        .map_err(|_| BusinessAuditError::Internal)?;
    tx.commit()
        .await
        .map_err(|_| BusinessAuditError::Internal)?;
    Ok(Json(response))
}

/// Mints a purpose-scoped receipt for a bounded audit export.
#[utoipa::path(
    post,
    operation_id = "reauthenticate_for_team_business_audit_export",
    path = "/team/business-audit/reauth",
    request_body = ReauthenticateRequest,
    responses(
        (status = 200, body = ReauthenticateResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ReauthenticateUnauthorizedResponse),
        (status = 403, body = ErrorResponse),
        (status = 429, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
        (status = 502, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn reauth_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<
        ExportAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Json(request): Json<ReauthenticateRequest>,
) -> Result<Json<ReauthenticateResponse>, ReauthenticateError> {
    let (team_id, principal) = export_scope(user, access).map_err(|error| match error {
        BusinessAuditError::PrincipalMismatch => ReauthenticateError::DirectUserRequired,
        _ => ReauthenticateError::Internal,
    })?;
    reauthenticate_password_for_purpose(
        &ctx,
        request_id,
        team_id,
        principal,
        &request,
        ReceiptPurpose::BusinessAuditExport,
    )
    .await
}

/// Completes an MFA challenge for a purpose-scoped audit-export receipt.
#[utoipa::path(
    post,
    operation_id = "complete_team_business_audit_export_reauthentication_mfa",
    path = "/team/business-audit/reauth/mfa",
    request_body = ReauthenticateMfaRequest,
    responses(
        (status = 200, body = ReauthenticateResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ReauthenticateMfaUnauthorizedResponse),
        (status = 403, body = ErrorResponse),
        (status = 429, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
        (status = 502, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn reauth_mfa_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<
        ExportAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Json(request): Json<ReauthenticateMfaRequest>,
) -> Result<Json<ReauthenticateResponse>, ReauthenticateError> {
    let (team_id, principal) = export_scope(user, access).map_err(|error| match error {
        BusinessAuditError::PrincipalMismatch => ReauthenticateError::DirectUserRequired,
        _ => ReauthenticateError::Internal,
    })?;
    reauthenticate_mfa_for_purpose(
        &ctx,
        request_id,
        team_id,
        principal,
        &request,
        ReceiptPurpose::BusinessAuditExport,
    )
    .await
}

/// Consumes one exact export receipt, renders the bounded CSV, records the
/// successful export fact, and commits all three effects atomically.
#[utoipa::path(
    post,
    operation_id = "export_team_business_audit",
    path = "/team/business-audit/export",
    request_body = BusinessAuditExportRequest,
    responses(
        (status = 200, content_type = "text/csv", description = "RFC4180 CSV download", headers(
            ("Content-Disposition" = String, description = "attachment; filename=business-audit.csv"),
            ("Cache-Control" = String, description = "no-store")
        )),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 413, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn export_handler(
    user: MacroAuthorizationExtractor<AuthorizationService, UserOnly>,
    access: MacroUserTeamExtractorV2<
        ExportAuditBusiness,
        EntityAccessServiceType,
        AuthorizationService,
    >,
    Extension(request_id): Extension<RequestId>,
    State(ctx): State<ApiContext>,
    Json(request): Json<BusinessAuditExportRequest>,
) -> Result<Response, BusinessAuditError> {
    if !valid_export_request(&request) {
        return Err(BusinessAuditError::InvalidExport);
    }
    let (team_id, principal) = export_scope(user, access)?;
    let retention_class = request.retention_class.map(Into::into);
    let reason =
        AuditReason::try_new(request.reason).map_err(|_| BusinessAuditError::InvalidExport)?;
    let correlation_id = request_correlation_id(request_id)?;
    let mut tx = ctx
        .db
        .begin()
        .await
        .map_err(|_| BusinessAuditError::Internal)?;
    let scope = ReceiptScope::new(
        team_id,
        principal.clone(),
        ReceiptPurpose::BusinessAuditExport,
    );
    let consumed = PgReauthenticationReceiptRepo::consume_with_tx(
        &mut tx,
        request.reauthentication_receipt,
        &scope,
    )
    .await
    .map_err(|_| BusinessAuditError::Internal)?;
    if !consumed {
        return Err(BusinessAuditError::InvalidReceipt);
    }
    let export_request = AuditExportRequest {
        team_id,
        from: request.from,
        until: request.until,
        retention_class,
    };
    let rows = export_with_tx(&mut tx, &export_request)
        .await
        .map_err(|_| BusinessAuditError::Internal)?;
    if rows.len() > business_audit::MAX_AUDIT_EXPORT_ROWS {
        return Err(BusinessAuditError::ExportTooLarge);
    }
    let csv = render_csv(&rows).map_err(|error| match error {
        CsvRenderError::TooLarge => BusinessAuditError::ExportTooLarge,
        CsvRenderError::UnsafeMetadata => BusinessAuditError::Internal,
    })?;
    let row_count = u16::try_from(rows.len()).expect("export row cap fits u16");
    let event = AuditEvent::new(
        team_id,
        Actor::new_from_user(principal),
        None,
        AuditAction::Exported(AuditExportedMetadata::new(
            export_request.from,
            export_request.until,
            export_request.retention_class.map(|value| match value {
                AuditRetentionFilter::Standard => RetentionClass::Standard,
                AuditRetentionFilter::Confidential => RetentionClass::Confidential,
                AuditRetentionFilter::Restricted => RetentionClass::Restricted,
            }),
            row_count,
        )),
        AuditTarget::Team(team_id),
        business_audit::AuditOutcome::Success,
        Utc::now(),
        correlation_id,
        Some(reason),
        RetentionClass::Confidential,
    )
    .map_err(|_| BusinessAuditError::Internal)?;
    insert_with_tx(&mut tx, &event)
        .await
        .map_err(|_| BusinessAuditError::Internal)?;
    tx.commit()
        .await
        .map_err(|_| BusinessAuditError::Internal)?;

    Ok(csv_download_response(csv))
}

#[cfg(test)]
mod test {
    use super::*;
    use business_audit::{DEFAULT_AUDIT_PAGE_SIZE, MAX_AUDIT_PAGE_SIZE};
    use http_body_util::BodyExt;

    #[test]
    fn response_has_only_the_approved_ledger_fields() {
        let response = BusinessAuditListResponse {
            items: vec![BusinessAuditListItem {
                id: Uuid::nil(),
                action: "role_granted".into(),
                target_type: "principal".into(),
                target_id: "macro|target@example.com".into(),
                actor: "macro|actor@example.com".into(),
                delegated_actor: None,
                outcome: "success".into(),
                occurred_at: Utc::now(),
                retention_class: "standard".into(),
            }],
            next_cursor: None,
        };
        let value = serde_json::to_value(response).unwrap();
        let item = &value["items"][0];
        for forbidden in [
            "reason",
            "request_id",
            "metadata",
            "receipt",
            "count",
            "total",
        ] {
            assert!(item.get(forbidden).is_none());
        }
        assert!(value.get("count").is_none());
        assert!(value.get("total").is_none());
    }

    #[test]
    fn endpoint_contract_is_bounded_and_source_orders_direct_human_authentication() {
        assert_eq!(DEFAULT_AUDIT_PAGE_SIZE, 50);
        assert_eq!(MAX_AUDIT_PAGE_SIZE, 100);
        let source = include_str!("business_audit.rs");
        let list_start = source.find("pub(crate) async fn handler(").unwrap();
        let list_body = &source[list_start..];
        let user = list_body
            .find("MacroAuthorizationExtractor<AuthorizationService, UserOnly>")
            .unwrap();
        let permission = list_body
            .find("MacroUserTeamExtractorV2<\n        ReadAuditBusiness")
            .unwrap();
        assert!(user < permission);
        let list_call = list_body.find("let page = list(").unwrap();
        assert!(
            permission < list_call,
            "list must run after both extractors"
        );
        assert!(source.contains("/business-audit"));
        for handler in [
            "detail_handler",
            "reauth_handler",
            "reauth_mfa_handler",
            "export_handler",
        ] {
            let declaration = format!("pub(crate) async fn {handler}(");
            let start = source.find(&declaration).unwrap();
            let body = &source[start..];
            let user = body
                .find("MacroAuthorizationExtractor<AuthorizationService, UserOnly>")
                .unwrap();
            let permission = body.find("ExportAuditBusiness").unwrap();
            assert!(user < permission, "{handler} must extract UserOnly first");
        }
    }

    #[test]
    fn access_capability_truth_table_is_team_scoped_and_receipt_free() {
        use models_team::{BusinessRole, BusinessRoleSet};
        use roles_and_permissions::domain::model::{PermissionId, has_business_permission};

        let team_id = Uuid::new_v4();
        for (role, can_read, can_export) in [
            (BusinessRole::Member, false, false),
            (BusinessRole::Auditor, true, false),
            (BusinessRole::OrgAdmin, true, true),
        ] {
            let roles = BusinessRoleSet::from_role(role);
            assert_eq!(
                has_business_permission(roles, PermissionId::ReadAuditBusiness),
                can_read,
            );
            assert_eq!(
                has_business_permission(roles, PermissionId::ExportAuditBusiness),
                can_export,
            );
        }
        assert_eq!(
            serde_json::to_value(business_audit_access_response(None, None).unwrap()).unwrap(),
            serde_json::json!({ "can_read": false, "can_export": false }),
        );
        assert_eq!(
            serde_json::to_value(business_audit_access_response(Some(team_id), None).unwrap())
                .unwrap(),
            serde_json::json!({ "can_read": true, "can_export": false }),
        );
        assert_eq!(
            serde_json::to_value(
                business_audit_access_response(Some(team_id), Some(team_id)).unwrap()
            )
            .unwrap(),
            serde_json::json!({ "can_read": true, "can_export": true }),
        );
        assert!(matches!(
            business_audit_access_response(Some(team_id), Some(Uuid::new_v4())),
            Err(BusinessAuditError::PrincipalMismatch),
        ));
        assert!(matches!(
            business_audit_access_response(None, Some(team_id)),
            Err(BusinessAuditError::PrincipalMismatch),
        ));
    }

    #[test]
    fn access_handler_requires_direct_human_before_optional_company_receipts() {
        let source = include_str!("business_audit.rs");
        let start = source.find("pub(crate) async fn access_handler(").unwrap();
        let body = &source[start..];
        let user = body
            .find("MacroAuthorizationExtractor<AuthorizationService, UserOnly>")
            .unwrap();
        let read = body
            .find("OptionalMacroUserTeamExtractorV2<\n        ReadAuditBusiness")
            .unwrap();
        let export = body
            .find("OptionalMacroUserTeamExtractorV2<\n        ExportAuditBusiness")
            .unwrap();
        assert!(user < read && read < export);
        assert!(body.contains("matching_team_receipt"));
        assert!(body.contains("business_audit_access_response"));
    }

    #[test]
    fn csv_is_rfc4180_quoted_formula_safe_and_utf8_preserving() {
        assert_eq!(csv_cell("comma,value"), "\"comma,value\"");
        assert_eq!(csv_cell("say \"hello\""), "\"say \"\"hello\"\"\"");
        assert_eq!(csv_cell("line1\r\nline2"), "\"line1\r\nline2\"");
        for value in ["=1+1", "+1", "-1", "@cmd", "\t=1", "\r=1", "\n=1"] {
            assert!(csv_cell(value).starts_with("\"'"), "{value}");
        }
        assert_eq!(csv_cell("한글"), "\"한글\"");

        let response = csv_download_response("\"한글\"\r\n".as_bytes().to_vec());
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/csv; charset=utf-8"
        );
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"business-audit.csv\""
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }

    #[test]
    fn csv_rejects_a_record_or_rendering_above_the_eight_mib_bound() {
        let mut bytes = vec![b'x'; MAX_AUDIT_EXPORT_BYTES - 1];
        assert_eq!(
            push_csv_record(&mut bytes, ["x".to_owned()]),
            Err(CsvRenderError::TooLarge)
        );

        let row = AuditExportRow {
            id: Uuid::nil(),
            actor: "x".repeat(MAX_AUDIT_EXPORT_BYTES),
            delegated_actor: None,
            action: "role_granted".into(),
            target_type: "principal".into(),
            target_id: "macro|target@example.com".into(),
            outcome: "success".into(),
            occurred_at: Utc::now(),
            request_id: "request".into(),
            reason: None,
            metadata: serde_json::json!({
                "business_role": "manager",
                "grantee_principal": "macro|target@example.com"
            }),
            retention_class: AuditRetentionFilter::Standard,
        };
        assert_eq!(render_csv(&[row]), Err(CsvRenderError::TooLarge));
    }

    #[test]
    fn privileged_metadata_is_allowlisted_and_export_validation_is_bounded() {
        assert!(fixed_safe_metadata(
            "role_granted",
            &serde_json::json!({"business_role":"hr_admin", "grantee_principal":"macro|a@example.com"}),
        )
        .is_some());
        assert!(
            fixed_safe_metadata(
                "role_granted",
                &serde_json::json!({"business_role":"hr_admin", "payload":"secret"}),
            )
            .is_none()
        );
        assert!(fixed_safe_metadata(
            "audit_exported",
            &serde_json::json!({"from":"2026-08-01T00:00:00Z", "until":"2026-08-02T00:00:00Z", "retention_class":null, "row_count":1}),
        )
        .is_some());

        let valid = BusinessAuditExportRequest {
            reauthentication_receipt: Uuid::nil(),
            from: "2026-08-01T00:00:00Z".parse().unwrap(),
            until: "2026-09-01T00:00:00Z".parse().unwrap(),
            retention_class: None,
            reason: "review needed".into(),
        };
        assert!(valid_export_request(&valid));
        let invalid = BusinessAuditExportRequest {
            until: valid.from,
            ..valid
        };
        assert!(!valid_export_request(&invalid));
    }

    #[tokio::test]
    async fn errors_have_stable_obfuscated_http_mappings() {
        for (error, status, message) in [
            (
                BusinessAuditError::InvalidCursor,
                StatusCode::BAD_REQUEST,
                "invalid audit cursor",
            ),
            (
                BusinessAuditError::PrincipalMismatch,
                StatusCode::FORBIDDEN,
                "direct user authentication required",
            ),
            (
                BusinessAuditError::NotFound,
                StatusCode::NOT_FOUND,
                "audit record not found",
            ),
            (
                BusinessAuditError::InvalidExport,
                StatusCode::BAD_REQUEST,
                "invalid audit export request",
            ),
            (
                BusinessAuditError::InvalidReceipt,
                StatusCode::CONFLICT,
                "invalid audit export receipt",
            ),
            (
                BusinessAuditError::ExportTooLarge,
                StatusCode::PAYLOAD_TOO_LARGE,
                "audit export exceeds its maximum size",
            ),
            (
                BusinessAuditError::Internal,
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error",
            ),
        ] {
            let response = error.into_response();
            assert_eq!(response.status(), status);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body, format!(r#"{{"message":"{message}"}}"#));
            let body = std::str::from_utf8(&body).unwrap();
            for forbidden in [
                "items",
                "count",
                "total",
                "reason",
                "request_id",
                "metadata",
                "actor",
                "target_id",
            ] {
                assert!(!body.contains(forbidden), "error leaked {forbidden}");
            }
        }
    }
}
