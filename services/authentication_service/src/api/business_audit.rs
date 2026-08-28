//! Direct-human, team-scoped reads from the immutable business-audit ledger.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use business_audit::{AuditListError, AuditListRequest, AuditRetentionFilter, list};
use chrono::{DateTime, Utc};
use entity_access::{
    domain::models::ReadAuditBusiness, inbound::axum_extractors::MacroUserTeamExtractorV2,
};
use macro_authorization::{MacroAuthorizationExtractor, UserOnly};
use model::response::ErrorResponse;
use std::borrow::Cow;
use uuid::Uuid;

use super::context::{ApiContext, AuthorizationService, EntityAccessServiceType};

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

#[derive(Debug, thiserror::Error)]
pub(crate) enum BusinessAuditError {
    /// Cursor syntax or request-filter binding was invalid.
    #[error("invalid audit cursor")]
    InvalidCursor,
    /// Direct authentication and the team access receipt disagreed.
    #[error("direct user authentication required")]
    PrincipalMismatch,
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
    Router::new().route("/business-audit", get(handler))
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
        let user = source
            .find("MacroAuthorizationExtractor<AuthorizationService, UserOnly>")
            .unwrap();
        let permission = source
            .find("MacroUserTeamExtractorV2<\n        ReadAuditBusiness")
            .unwrap();
        assert!(user < permission);
        assert!(source.contains("/business-audit"));
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
                BusinessAuditError::Internal,
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error",
            ),
        ] {
            let response = error.into_response();
            assert_eq!(response.status(), status);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body, format!(r#"{{"message":"{message}"}}"#));
        }
    }
}
