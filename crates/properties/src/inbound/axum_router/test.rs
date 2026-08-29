use entity_access::domain::models::TeamRole;
use std::sync::{Arc, Mutex};

use super::entities::SetEntityPropertyErr;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use entity_access::domain::{
    models::{
        AccessError, AccessLevel, BotAccessScope, BotId, CallChannelInfo, EntityAccessAuth,
        EntityAccessReceipt, EntityPermission, EntityType, RequiredPermission, UserTeamInfo,
    },
    ports::EntityAccessService,
};
#[allow(deprecated)]
use macro_authorization::{
    INTERNAL_API_KEY_HEADER, InternalAuthConfig, JwtValidator, LEGACY_DSS_INTERNAL_API_KEY_HEADER,
    MacroAuthorizationError, MacroAuthorizationExtractor, MacroAuthorizationServiceImpl,
    MacroAuthorizationState, UserOrInternal, ValidatedIdentity,
};
use macro_user_id::{
    lowercased::Lowercase,
    user_id::{MacroUserId, MacroUserIdStr},
};
use rootcause::Report;
use tower::ServiceExt;
use uuid::Uuid;

use super::{
    PropertiesRouterState, PropertyTeamExtractor,
    extract::{EditReceiptExtractor, ViewReceiptExtractor},
};
use crate::{
    PropertiesErr, PropertiesServiceImpl,
    domain::ports::{MockNotificationService, MockPermissionService, MockPropertiesRepo},
};

#[test]
fn task_dependency_errors_keep_the_frozen_statuses_and_bodies() {
    let cases = [
        (
            PropertiesErr::Validation("Depends On requires task references".to_string()),
            StatusCode::BAD_REQUEST,
            "Depends On requires task references",
        ),
        (
            PropertiesErr::PermissionDenied,
            StatusCode::FORBIDDEN,
            "Access denied",
        ),
        (
            PropertiesErr::TaskDependenciesUnavailable,
            StatusCode::NOT_FOUND,
            "One or more task dependencies are unavailable",
        ),
        (
            PropertiesErr::TaskDependencyCycle,
            StatusCode::CONFLICT,
            "Task dependencies cannot contain a cycle",
        ),
        (
            PropertiesErr::TaskHierarchyCycle,
            StatusCode::CONFLICT,
            "Task hierarchy cannot contain a cycle",
        ),
        (
            PropertiesErr::TaskTransitionBlocked,
            StatusCode::CONFLICT,
            "Task transition is blocked by dependencies",
        ),
    ];
    for (error, status, body) in cases {
        assert_eq!(super::properties_err_status(&error), status);
        assert_eq!(error.to_string(), body);
    }
}

#[tokio::test]
async fn task_dependency_set_handler_errors_render_frozen_bodies() {
    let cases = [
        (
            PropertiesErr::Validation("Depends On requires task references".to_string()),
            StatusCode::BAD_REQUEST,
            "Depends On requires task references",
        ),
        (
            PropertiesErr::PermissionDenied,
            StatusCode::FORBIDDEN,
            "Access denied",
        ),
        (
            PropertiesErr::TaskDependenciesUnavailable,
            StatusCode::NOT_FOUND,
            "One or more task dependencies are unavailable",
        ),
        (
            PropertiesErr::TaskDependencyCycle,
            StatusCode::CONFLICT,
            "Task dependencies cannot contain a cycle",
        ),
        (
            PropertiesErr::TaskHierarchyCycle,
            StatusCode::CONFLICT,
            "Task hierarchy cannot contain a cycle",
        ),
        (
            PropertiesErr::TaskTransitionBlocked,
            StatusCode::CONFLICT,
            "Task transition is blocked by dependencies",
        ),
    ];
    for (error, status, body) in cases {
        let response = SetEntityPropertyErr::from(error).into_response();
        assert_eq!(response.status(), status);
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            body
        );
    }
}

#[tokio::test]
async fn structured_task_transition_blocker_renders_exact_json_conflict() {
    let task_id = Uuid::from_u128(0x901);
    let visible_completed = Uuid::from_u128(0x902);
    let visible_blocking = Uuid::from_u128(0x903);
    let hidden_sentinel = Uuid::from_u128(0x904);
    let response = SetEntityPropertyErr::from(PropertiesErr::TaskTransitionBlockedWithReadiness(
        crate::domain::model::TaskTransitionBlockedDetails::new(
            crate::domain::model::TaskDependencyReadiness {
                task_id,
                readiness: crate::domain::model::TaskReadiness::Blocked,
                depends_on_task_ids: vec![visible_completed, visible_blocking],
                blocking_task_ids: vec![visible_blocking],
                has_unavailable_dependencies: true,
            },
        ),
    ))
    .into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "taskId": task_id,
            "readiness": "blocked",
            "dependsOnTaskIds": [visible_completed, visible_blocking],
            "blockingTaskIds": [visible_blocking],
            "hasUnavailableDependencies": true,
        })
    );
    assert!(!body.to_string().contains(&hidden_sentinel.to_string()));
}

#[tokio::test]
async fn structured_task_completion_blocker_renders_exact_json_conflict() {
    let task_id = Uuid::from_u128(0x911);
    let blocker = Uuid::from_u128(0x912);
    let response = SetEntityPropertyErr::from(PropertiesErr::TaskCompletionBlockedBySubtasks(
        crate::domain::model::TaskSubtaskCompletionBlockedDetails::new(
            crate::domain::model::TaskSubtaskCompletionReadiness {
                task_id,
                readiness: crate::domain::model::TaskReadiness::Blocked,
                subtask_ids: vec![blocker],
                blocking_subtask_ids: vec![blocker],
                has_unavailable_subtasks: true,
            },
        ),
    ))
    .into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "taskId": task_id,
            "readiness": "blocked",
            "subtaskIds": [blocker],
            "blockingSubtaskIds": [blocker],
            "hasUnavailableSubtasks": true,
        })
    );
}

const DEFAULT_INTERNAL_USER_ID: &str = "macro|internal@macro.com";
const INTERNAL_API_KEY: &str = "test-internal-key";
const ORGANIZATION_ID: i32 = 42;
const VALID_USER_ID: &str = "macro|valid@example.com";

type TestPropertiesService =
    PropertiesServiceImpl<MockPropertiesRepo, MockPermissionService, MockNotificationService>;
type TestAuthorizationService = MacroAuthorizationServiceImpl<FakeJwtValidator>;

#[derive(Clone, Debug, PartialEq)]
enum AccessCall {
    GenerateReceipt {
        user_id: String,
        organization_id: Option<i64>,
        entity_id: String,
        entity_type: EntityType,
    },
    PublicAccess {
        entity_id: String,
        entity_type: EntityType,
        required_level: AccessLevel,
    },
    UserTeam {
        user_id: String,
    },
}

#[derive(Clone, Debug, Default)]
struct FakeEntityAccessService {
    calls: Arc<Mutex<Vec<AccessCall>>>,
    team: Option<UserTeamInfo>,
    deny_team_receipt: bool,
}

impl FakeEntityAccessService {
    /// A service whose caller belongs to `team_id`, for the endpoints that
    /// require team membership.
    fn on_team(team_id: Uuid) -> Self {
        Self {
            calls: Arc::default(),
            team: Some(UserTeamInfo {
                team_id,
                role: TeamRole::Member,
                business_roles: Default::default(),
            }),
            deny_team_receipt: false,
        }
    }

    fn on_team_with_denied_work_receipt(team_id: Uuid) -> Self {
        Self {
            deny_team_receipt: true,
            ..Self::on_team(team_id)
        }
    }

    fn calls(&self) -> Vec<AccessCall> {
        self.calls.lock().expect("calls lock poisoned").clone()
    }

    fn record(&self, call: AccessCall) {
        self.calls.lock().expect("calls lock poisoned").push(call);
    }
}

impl EntityAccessService for FakeEntityAccessService {
    async fn generate_entity_access_receipt<T: RequiredPermission>(
        &self,
        user_id: &MacroUserId<Lowercase<'_>>,
        user_org_id: Option<i64>,
        entity_id: &str,
        entity_type: EntityType,
    ) -> Result<EntityAccessReceipt<T>, AccessError> {
        self.record(AccessCall::GenerateReceipt {
            user_id: user_id.as_ref().to_string(),
            organization_id: user_org_id,
            entity_id: entity_id.to_string(),
            entity_type,
        });

        // Entities whose id contains "denied" model the caller lacking access,
        // so receipt-minting fails for them (used by the bulk skip test).
        if entity_id.contains("denied") {
            return Err(AccessError::Unauthorized);
        }
        if self.deny_team_receipt && entity_type == EntityType::Team {
            return Err(AccessError::Unauthorized);
        }

        let user_id = MacroUserIdStr::try_from(user_id.as_ref().to_string())
            .expect("authorized test user id should be valid");
        Ok(EntityAccessReceipt::dangerously_assert_authenticated_user(
            user_id,
            entity_id,
            entity_type,
        ))
    }

    async fn generate_bot_entity_access_receipt<T: RequiredPermission>(
        &self,
        _bot_id: BotId,
        _scope: BotAccessScope,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<EntityAccessReceipt<T>, AccessError> {
        panic!("unexpected bot receipt request")
    }

    async fn get_access_level(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<Option<AccessLevel>, AccessError> {
        panic!("unexpected access-level request")
    }

    async fn check_access(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
        _required_level: AccessLevel,
    ) -> Result<AccessLevel, AccessError> {
        panic!("unexpected authenticated access check")
    }

    async fn check_public_access(
        &self,
        entity_id: &str,
        entity_type: EntityType,
        required_level: AccessLevel,
    ) -> Result<AccessLevel, AccessError> {
        self.record(AccessCall::PublicAccess {
            entity_id: entity_id.to_string(),
            entity_type,
            required_level,
        });
        Ok(AccessLevel::View)
    }

    async fn get_entity_permission(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
        _user_org_id: Option<i64>,
    ) -> Result<EntityPermission, AccessError> {
        panic!("unexpected entity-permission request")
    }

    async fn get_crm_entity_permission_with_team(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<(EntityPermission, Uuid, TeamRole), AccessError> {
        panic!("unexpected CRM permission request")
    }

    async fn get_users_by_entity(
        &self,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<Vec<MacroUserIdStr<'static>>, AccessError> {
        panic!("unexpected entity-users request")
    }

    async fn get_call_channel(
        &self,
        _call_id: &Uuid,
    ) -> Result<Option<CallChannelInfo>, AccessError> {
        panic!("unexpected call-channel request")
    }

    async fn get_call_channel_by_channel_id(
        &self,
        _channel_id: &Uuid,
    ) -> Result<Option<CallChannelInfo>, AccessError> {
        panic!("unexpected channel-call request")
    }

    async fn get_user_team(
        &self,
        user_id: &MacroUserId<Lowercase<'_>>,
    ) -> Result<Option<UserTeamInfo>, AccessError> {
        self.record(AccessCall::UserTeam {
            user_id: user_id.as_ref().to_string(),
        });
        Ok(self.team)
    }
}

#[derive(Clone, Copy)]
struct FakeJwtValidator;

impl JwtValidator for FakeJwtValidator {
    fn validate(&self, jwt: &str) -> Result<ValidatedIdentity, Report<MacroAuthorizationError>> {
        match jwt {
            "valid" => Ok(ValidatedIdentity {
                user_id: VALID_USER_ID.to_string(),
                fusion_user_id: "fusion-valid-user".to_string(),
                organization_id: Some(ORGANIZATION_ID),
                permissions: None,
            }),
            "expired" => Err(Report::new(MacroAuthorizationError::CredentialsExpired)),
            _ => Err(Report::new(MacroAuthorizationError::InvalidCredentials)),
        }
    }
}

fn no_op_properties_service() -> TestPropertiesService {
    PropertiesServiceImpl::new(
        MockPropertiesRepo::new(),
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    )
}

fn authorization_state() -> MacroAuthorizationState<TestAuthorizationService> {
    let service = MacroAuthorizationServiceImpl::new(
        FakeJwtValidator,
        InternalAuthConfig {
            api_key: INTERNAL_API_KEY.to_string(),
            default_user_id: Some(DEFAULT_INTERNAL_USER_ID.to_string()),
        },
        macro_authorization::NoBotAuthorizer,
    );
    MacroAuthorizationState::new(Arc::new(service))
}

fn test_router(entity_access_service: FakeEntityAccessService) -> Router {
    let state = PropertiesRouterState::new(
        Arc::new(no_op_properties_service()),
        Arc::new(entity_access_service),
        authorization_state(),
    );

    Router::new()
        .route("/required", get(required_auth_handler))
        .route("/team", get(team_handler))
        .route("/view/{entity_type}/{entity_id}", get(view_handler))
        .route("/edit/{entity_type}/{entity_id}", get(edit_handler))
        .with_state(state)
}

async fn required_auth_handler(
    authorization: MacroAuthorizationExtractor<TestAuthorizationService, UserOrInternal>,
) -> String {
    authorization.authorization.user.macro_user_id.to_string()
}

async fn team_handler(
    team: PropertyTeamExtractor<FakeEntityAccessService, TestAuthorizationService>,
) -> &'static str {
    if team.entity_access_receipt.is_some() {
        "team"
    } else {
        "no-team"
    }
}

async fn view_handler(ViewReceiptExtractor(receipt): ViewReceiptExtractor) -> &'static str {
    match receipt.auth() {
        EntityAccessAuth::Authenticated(_) => "authenticated",
        EntityAccessAuth::Unauthenticated => "unauthenticated",
        EntityAccessAuth::Bot(_) => "bot",
        EntityAccessAuth::Internal => "internal",
    }
}

async fn edit_handler(EditReceiptExtractor(_receipt): EditReceiptExtractor) -> StatusCode {
    StatusCode::OK
}

fn request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request should be valid")
}

fn bearer_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request should be valid")
}

async fn response_body(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(body.to_vec()).expect("response body should be UTF-8")
}

#[tokio::test]
async fn required_auth_rejects_bad_credentials_and_accepts_valid_bearer() {
    let router = test_router(FakeEntityAccessService::default());

    let response = router
        .clone()
        .oneshot(request("/required"))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_body(response).await,
        r#"{"message":"unauthorized"}"#
    );

    let response = router
        .clone()
        .oneshot(bearer_request("/required", "invalid"))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_body(response).await,
        r#"{"message":"unauthorized"}"#
    );

    let response = router
        .clone()
        .oneshot(bearer_request("/required", "expired"))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_body(response).await,
        r#"{"message":"jwt expired"}"#
    );

    let response = router
        .oneshot(bearer_request("/required", "valid"))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_body(response).await, VALID_USER_ID);
}

#[tokio::test]
async fn property_team_extractor_uses_authorized_user() {
    let entity_access_service = FakeEntityAccessService::default();
    let response = test_router(entity_access_service.clone())
        .oneshot(bearer_request("/team", "valid"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_body(response).await, "no-team");
    assert_eq!(
        entity_access_service.calls(),
        [AccessCall::UserTeam {
            user_id: VALID_USER_ID.to_string(),
        }]
    );
}

#[allow(deprecated)]
#[tokio::test]
async fn standard_and_legacy_internal_headers_use_the_default_user() {
    for key_header in [INTERNAL_API_KEY_HEADER, LEGACY_DSS_INTERNAL_API_KEY_HEADER] {
        let request = Request::builder()
            .uri("/required")
            .header(key_header, INTERNAL_API_KEY)
            .body(Body::empty())
            .expect("request should be valid");
        let response = test_router(FakeEntityAccessService::default())
            .oneshot(request)
            .await
            .expect("request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response_body(response).await, DEFAULT_INTERNAL_USER_ID);
    }
}

#[tokio::test]
async fn anonymous_view_mints_public_receipt_but_invalid_token_is_rejected() {
    let entity_access_service = FakeEntityAccessService::default();
    let router = test_router(entity_access_service.clone());

    let response = router
        .clone()
        .oneshot(request("/view/DOCUMENT/public-document"))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_body(response).await, "unauthenticated");
    assert_eq!(
        entity_access_service.calls(),
        [AccessCall::PublicAccess {
            entity_id: "public-document".to_string(),
            entity_type: EntityType::Document,
            required_level: AccessLevel::View,
        }]
    );

    let response = router
        .oneshot(bearer_request("/view/DOCUMENT/public-document", "invalid"))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response_body(response).await, "unauthorized");
    assert_eq!(
        entity_access_service.calls().len(),
        1,
        "invalid credentials must not fall back to public authorization"
    );
}

#[tokio::test]
async fn document_target_mints_a_document_receipt_for_a_task_shaped_id() {
    let entity_access_service = FakeEntityAccessService::default();
    let response = test_router(entity_access_service.clone())
        .oneshot(bearer_request("/view/DOCUMENT/task-shaped-id", "valid"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_body(response).await, "authenticated");
    assert_eq!(
        entity_access_service.calls(),
        [AccessCall::GenerateReceipt {
            user_id: VALID_USER_ID.to_string(),
            organization_id: None,
            entity_id: "task-shaped-id".to_string(),
            entity_type: EntityType::Document,
        }]
    );
}

#[tokio::test]
async fn task_target_is_rejected_before_authorization_or_entity_access() {
    let entity_access_service = FakeEntityAccessService::default();
    let response = test_router(entity_access_service.clone())
        .oneshot(request("/view/TASK/task-shaped-id"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_body(response).await,
        "Missing or invalid entity_type / entity_id in path"
    );
    assert!(
        entity_access_service.calls().is_empty(),
        "invalid task targets must not reach authorization or entity access"
    );
}

#[tokio::test]
async fn edit_receipt_omits_organization_from_access_check() {
    let entity_access_service = FakeEntityAccessService::default();
    let response = test_router(entity_access_service.clone())
        .oneshot(bearer_request("/edit/DOCUMENT/document-id", "valid"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        entity_access_service.calls(),
        [AccessCall::GenerateReceipt {
            user_id: VALID_USER_ID.to_string(),
            organization_id: None,
            entity_id: "document-id".to_string(),
            entity_type: EntityType::Document,
        }]
    );
}

#[tokio::test]
async fn receipt_rejection_preserves_expired_token_message() {
    let response = test_router(FakeEntityAccessService::default())
        .oneshot(bearer_request("/edit/DOCUMENT/document-id", "expired"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response_body(response).await, "jwt expired");
}

#[tokio::test]
async fn malformed_typed_path_is_rejected_before_missing_authentication() {
    let response = test_router(FakeEntityAccessService::default())
        .oneshot(request("/edit/not-an-entity/document-id"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_body(response).await,
        "Missing or invalid entity_type / entity_id in path"
    );
}

fn multi_select_def(
    id: Uuid,
) -> models_properties::service::property_definition::PropertyDefinition {
    models_properties::service::property_definition::PropertyDefinition {
        id,
        owner: models_properties::PropertyOwner::System,
        display_name: "Tags".to_string(),
        data_type: models_properties::DataType::SelectString,
        is_multi_select: true,
        specific_entity_type: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        is_system: false,
        is_metadata: false,
    }
}

/// Builds the full properties router over a mock-repo service so the bulk
/// cross-entity endpoint can be exercised end to end.
fn properties_router(
    service: TestPropertiesService,
    entity_access_service: FakeEntityAccessService,
) -> Router {
    let state = PropertiesRouterState::new(
        Arc::new(service),
        Arc::new(entity_access_service),
        authorization_state(),
    );
    super::router::<TestPropertiesService, FakeEntityAccessService, TestAuthorizationService>()
        .with_state(state)
}

#[tokio::test]
async fn bulk_options_across_entities_skips_denied_and_applies_granted() {
    use crate::domain::model::EntityPropertyOptionSelection;

    let property_id = Uuid::new_v4();
    let option_id = Uuid::new_v4();

    let mut repo = MockPropertiesRepo::new();
    repo.expect_get_property_definition()
        .returning(move |_| Box::pin(async move { Ok(Some(multi_select_def(property_id))) }));
    repo.expect_count_valid_property_options()
        .returning(|_, _| Box::pin(async { Ok(1) }));
    // Only the granted entity reaches the write path.
    repo.expect_bulk_update_entity_property_options()
        .times(1)
        .withf(|entity_id, _, _| entity_id == "doc-granted")
        .returning(move |_, _, _| {
            Box::pin(async move {
                Ok(vec![EntityPropertyOptionSelection {
                    property_definition_id: property_id,
                    option_ids: vec![option_id],
                    mutation: None,
                }])
            })
        });

    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let router = properties_router(service, FakeEntityAccessService::default());

    let body = serde_json::json!({
        "entities": [
            {"entity_type": "DOCUMENT", "entity_id": "doc-denied"},
            {"entity_type": "DOCUMENT", "entity_id": "doc-granted"},
        ],
        "property_id": property_id,
        "add_option_ids": [option_id],
        "remove_option_ids": [],
    });
    let request = Request::builder()
        .method("POST")
        .uri("/options/bulk")
        .header(header::AUTHORIZATION, "Bearer valid")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should be valid");

    let response = router
        .oneshot(request)
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let json: serde_json::Value =
        serde_json::from_str(&response_body(response).await).expect("json body");
    let results = json["results"].as_array().expect("results array");
    assert_eq!(results.len(), 2, "one result per requested entity");

    // Results are returned in request order.
    assert_eq!(results[0]["entity_id"], "doc-denied");
    assert_eq!(results[0]["status"], "skipped_no_permission");
    assert_eq!(results[1]["entity_id"], "doc-granted");
    assert_eq!(results[1]["status"], "applied");
    assert_eq!(results[1]["option_ids"], serde_json::json!([option_id]));
}

/// A tag definition owned by `owner`, for the promote/merge endpoint tests.
fn tag_def(
    id: Uuid,
    owner: models_properties::PropertyOwner,
) -> models_properties::service::property_definition::PropertyDefinition {
    models_properties::service::property_definition::PropertyDefinition {
        id,
        owner,
        display_name: "Tags".to_string(),
        data_type: models_properties::DataType::Tag,
        is_multi_select: true,
        specific_entity_type: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        is_system: false,
        is_metadata: false,
    }
}

fn tag_option(
    id: Uuid,
    property_definition_id: Uuid,
    value: &str,
    color: &str,
) -> models_properties::service::property_option::PropertyOption {
    models_properties::service::property_option::PropertyOption {
        id,
        property_definition_id,
        display_order: 0,
        value: models_properties::service::property_option::PropertyOptionValue::String(
            value.to_string(),
        ),
        color: Some(color.to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Wire a mock repo up to the reads promote makes before it writes.
fn tag_promotion_repo(
    personal_definition_id: Uuid,
    team_definition_id: Uuid,
    option_id: Uuid,
) -> MockPropertiesRepo {
    let mut repo = MockPropertiesRepo::new();
    repo.expect_get_property_option().returning(move |_| {
        Box::pin(async move {
            Ok(Some(tag_option(
                option_id,
                personal_definition_id,
                "Urgent",
                "#ff0000",
            )))
        })
    });
    repo.expect_get_tag_definition().returning(move |_| {
        Box::pin(async move {
            Ok(Some(tag_def(
                personal_definition_id,
                models_properties::PropertyOwner::User {
                    user_id: VALID_USER_ID.to_string(),
                },
            )))
        })
    });
    repo.expect_get_or_create_tag_definition()
        .returning(move |_| {
            Box::pin(async move {
                Ok(crate::domain::model::GetOrCreateTagDefinitionResult {
                    definition: tag_def(
                        team_definition_id,
                        models_properties::PropertyOwner::Team {
                            team_id: Uuid::from_u128(0x7EA3),
                        },
                    ),
                    created: false,
                })
            })
        });
    repo
}

fn json_post(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer valid")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should be valid")
}

#[tokio::test]
async fn promote_tag_conflict_returns_the_team_label_the_caller_can_merge_into() {
    let personal_definition_id = Uuid::from_u128(0x9001);
    let team_definition_id = Uuid::from_u128(0x9002);
    let option_id = Uuid::from_u128(0x9003);
    let conflict_id = Uuid::from_u128(0x9004);

    let mut repo = tag_promotion_repo(personal_definition_id, team_definition_id, option_id);
    repo.expect_promote_tag_option().returning(move |_, _, _| {
        Box::pin(async move {
            Ok(crate::domain::model::TagPromotionOutcome::Conflict(
                tag_option(conflict_id, team_definition_id, "urgent", "#00ff00"),
            ))
        })
    });

    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let router = properties_router(
        service,
        FakeEntityAccessService::on_team(Uuid::from_u128(0x7EA3)),
    );

    let response = router
        .oneshot(json_post(
            "/tags/promote",
            serde_json::json!({ "option_id": option_id }),
        ))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // The front end prompts with this label, then confirms via /tags/merge.
    let json: serde_json::Value =
        serde_json::from_str(&response_body(response).await).expect("json body");
    assert_eq!(json["conflicting_option"]["id"], conflict_id.to_string());
    assert_eq!(
        json["conflicting_option"]["value"],
        serde_json::json!({"type": "string", "value": "urgent"})
    );
    assert_eq!(json["conflicting_option"]["color"], "#00ff00");
    assert!(json["message"].is_string());
}

#[tokio::test]
async fn promote_tag_returns_the_label_hanging_off_the_team_definition() {
    let personal_definition_id = Uuid::from_u128(0x9101);
    let team_definition_id = Uuid::from_u128(0x9102);
    let option_id = Uuid::from_u128(0x9103);

    let mut repo = tag_promotion_repo(personal_definition_id, team_definition_id, option_id);
    repo.expect_promote_tag_option().returning(move |_, _, _| {
        Box::pin(async move {
            Ok(crate::domain::model::TagPromotionOutcome::Promoted(
                crate::domain::model::TagRemapOutcome {
                    option: tag_option(option_id, team_definition_id, "Urgent", "#ff0000"),
                    mutations: Vec::new(),
                },
            ))
        })
    });

    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let router = properties_router(
        service,
        FakeEntityAccessService::on_team(Uuid::from_u128(0x7EA3)),
    );

    let response = router
        .oneshot(json_post(
            "/tags/promote",
            serde_json::json!({ "option_id": option_id }),
        ))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let json: serde_json::Value =
        serde_json::from_str(&response_body(response).await).expect("json body");
    assert_eq!(json["id"], option_id.to_string());
    assert_eq!(
        json["propertyDefinitionId"],
        team_definition_id.to_string(),
        "the option id survives, the owning definition is the team's"
    );
}

#[tokio::test]
async fn promote_tag_without_a_team_is_forbidden() {
    let personal_definition_id = Uuid::from_u128(0x9201);
    let option_id = Uuid::from_u128(0x9202);

    let mut repo = MockPropertiesRepo::new();
    repo.expect_get_property_option().returning(move |_| {
        Box::pin(async move {
            Ok(Some(tag_option(
                option_id,
                personal_definition_id,
                "Urgent",
                "#ff0000",
            )))
        })
    });
    repo.expect_get_tag_definition().returning(move |_| {
        Box::pin(async move {
            Ok(Some(tag_def(
                personal_definition_id,
                models_properties::PropertyOwner::User {
                    user_id: VALID_USER_ID.to_string(),
                },
            )))
        })
    });
    repo.expect_promote_tag_option().never();

    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let router = properties_router(service, FakeEntityAccessService::default());

    let response = router
        .oneshot(json_post(
            "/tags/promote",
            serde_json::json!({ "option_id": option_id }),
        ))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

fn dependency_readiness_router(
    service: TestPropertiesService,
    entity_access_service: FakeEntityAccessService,
) -> Router {
    let state = PropertiesRouterState::new(
        Arc::new(service),
        Arc::new(entity_access_service),
        authorization_state(),
    );
    super::project_dependency_readiness::project_dependency_readiness_router::<
        TestPropertiesService,
        FakeEntityAccessService,
        TestAuthorizationService,
    >()
    .with_state(state)
}

fn dependency_readiness_request(
    project_id: &str,
    authorization: Option<&str>,
    body: impl Into<Body>,
) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/{project_id}/task-dependency-readiness"))
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    request.body(body.into()).expect("request should be valid")
}

async fn assert_readiness_error(response: axum::response::Response, status: StatusCode) {
    assert_eq!(response.status(), status);
    let expected_message = match status {
        StatusCode::BAD_REQUEST => "invalid request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not found",
        StatusCode::INTERNAL_SERVER_ERROR => "internal server error",
        _ => panic!("unexpected readiness status"),
    };
    let body: serde_json::Value =
        serde_json::from_str(&response_body(response).await).expect("readiness error must be JSON");
    let object = body.as_object().expect("readiness error must be an object");
    assert_eq!(
        object
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        ["error", "message"].into_iter().collect()
    );
    assert_eq!(object["error"], true);
    assert_eq!(object["message"], expected_message);
}

#[tokio::test]
async fn task_dependency_readiness_forwards_duplicates_in_order_and_returns_exact_raw_shape() {
    use crate::domain::model::{TaskDependencyReadiness, TaskReadiness};

    let team_id = Uuid::from_u128(0xD355);
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);
    let requested = vec![first, first, second];
    let expected_requested = requested.clone();
    let mut repo = MockPropertiesRepo::new();
    repo.expect_get_task_dependency_readiness()
        .times(1)
        .withf(move |project_id, actual_team_id, task_ids| {
            project_id == "project-readiness"
                && *actual_team_id == team_id
                && task_ids == expected_requested.as_slice()
        })
        .returning(move |_, _, _| {
            Box::pin(async move {
                Ok(Some(vec![TaskDependencyReadiness {
                    task_id: first,
                    readiness: TaskReadiness::Ready,
                    depends_on_task_ids: vec![second],
                    blocking_task_ids: vec![],
                    has_unavailable_dependencies: false,
                }]))
            })
        });
    let entity_access_service = FakeEntityAccessService::on_team(team_id);
    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let response = dependency_readiness_router(service, entity_access_service.clone())
        .oneshot(dependency_readiness_request(
            "project-readiness",
            Some("Bearer valid"),
            serde_json::json!({ "taskIds": requested }).to_string(),
        ))
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&response_body(response).await).unwrap();
    let items = body.as_array().expect("raw array response");
    let item = items
        .first()
        .expect("one readiness item")
        .as_object()
        .unwrap();
    assert_eq!(
        item.keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "blockingTaskIds",
            "dependsOnTaskIds",
            "hasUnavailableDependencies",
            "readiness",
            "taskId",
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(item["readiness"], "ready");
    assert_eq!(
        entity_access_service.calls(),
        [
            AccessCall::GenerateReceipt {
                user_id: VALID_USER_ID.to_owned(),
                organization_id: None,
                entity_id: "project-readiness".to_owned(),
                entity_type: EntityType::Project,
            },
            AccessCall::UserTeam {
                user_id: VALID_USER_ID.to_owned(),
            },
            AccessCall::GenerateReceipt {
                user_id: VALID_USER_ID.to_owned(),
                organization_id: None,
                entity_id: team_id.to_string(),
                entity_type: EntityType::Team,
            },
        ]
    );
}

#[tokio::test]
async fn task_dependency_readiness_empty_and_invalid_requests_never_reach_the_repository() {
    let team_id = Uuid::from_u128(0xD356);
    for body in [
        r#"{"taskIds":["not-a-uuid"]}"#.to_owned(),
        r#"{"taskIds":[],"unknown":true}"#.to_owned(),
        "{".to_owned(),
        serde_json::json!({ "taskIds": vec![Uuid::nil(); 201] }).to_string(),
    ] {
        let mut repo = MockPropertiesRepo::new();
        repo.expect_get_task_dependency_readiness().never();
        let service = PropertiesServiceImpl::new(
            repo,
            None::<MockPermissionService>,
            None::<MockNotificationService>,
        );
        let response =
            dependency_readiness_router(service, FakeEntityAccessService::on_team(team_id))
                .oneshot(dependency_readiness_request(
                    "project-readiness",
                    Some("Bearer valid"),
                    body,
                ))
                .await
                .expect("router should respond");
        assert_readiness_error(response, StatusCode::BAD_REQUEST).await;
    }

    let mut repo = MockPropertiesRepo::new();
    repo.expect_get_task_dependency_readiness().never();
    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let response = dependency_readiness_router(service, FakeEntityAccessService::on_team(team_id))
        .oneshot(dependency_readiness_request(
            "project-readiness",
            Some("Bearer valid"),
            r#"{"taskIds":[]}"#,
        ))
        .await
        .expect("router should respond");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_body(response).await, "[]");
}

#[tokio::test]
async fn task_dependency_readiness_rejects_auth_and_receipt_denials_without_repository_calls() {
    for (project_id, authorization, access, status) in [
        (
            "project-readiness",
            None,
            FakeEntityAccessService::on_team(Uuid::new_v4()),
            StatusCode::UNAUTHORIZED,
        ),
        (
            "project-readiness",
            Some("Bearer invalid"),
            FakeEntityAccessService::on_team(Uuid::new_v4()),
            StatusCode::UNAUTHORIZED,
        ),
        (
            "project-denied",
            Some("Bearer valid"),
            FakeEntityAccessService::on_team(Uuid::new_v4()),
            StatusCode::FORBIDDEN,
        ),
        (
            "project-readiness",
            Some("Bearer valid"),
            FakeEntityAccessService::default(),
            StatusCode::FORBIDDEN,
        ),
        (
            "project-readiness",
            Some("Bearer valid"),
            FakeEntityAccessService::on_team_with_denied_work_receipt(Uuid::new_v4()),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let mut repo = MockPropertiesRepo::new();
        repo.expect_get_task_dependency_readiness().never();
        let service = PropertiesServiceImpl::new(
            repo,
            None::<MockPermissionService>,
            None::<MockNotificationService>,
        );
        let observed_access = access.clone();
        let response = dependency_readiness_router(service, access)
            .oneshot(dependency_readiness_request(
                project_id,
                authorization,
                r#"{"taskIds":[]}"#,
            ))
            .await
            .expect("router should respond");
        assert_readiness_error(response, status).await;
        if status == StatusCode::UNAUTHORIZED {
            assert!(
                observed_access.calls().is_empty(),
                "UserOnly must reject missing/invalid credentials before entity access"
            );
        }
    }

    let mut repo = MockPropertiesRepo::new();
    repo.expect_get_task_dependency_readiness().never();
    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let request = Request::builder()
        .method("POST")
        .uri("/project-readiness/task-dependency-readiness")
        .header(INTERNAL_API_KEY_HEADER, INTERNAL_API_KEY)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"taskIds":[]}"#))
        .expect("request should be valid");
    let internal_access = FakeEntityAccessService::on_team(Uuid::new_v4());
    let response = dependency_readiness_router(service, internal_access.clone())
        .oneshot(request)
        .await
        .expect("router should respond");
    assert_readiness_error(response, StatusCode::FORBIDDEN).await;
    assert!(
        internal_access.calls().is_empty(),
        "UserOnly must reject internal callers before entity access"
    );
}

#[tokio::test]
async fn task_dependency_readiness_maps_unavailable_and_repo_failures_without_leaking_sentinel() {
    let team_id = Uuid::from_u128(0xD357);
    let mut unavailable = MockPropertiesRepo::new();
    unavailable
        .expect_get_task_dependency_readiness()
        .returning(|_, _, _| Box::pin(async { Ok(None) }));
    let service = PropertiesServiceImpl::new(
        unavailable,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let response = dependency_readiness_router(service, FakeEntityAccessService::on_team(team_id))
        .oneshot(dependency_readiness_request(
            "project-readiness",
            Some("Bearer valid"),
            serde_json::json!({ "taskIds": [Uuid::nil()] }).to_string(),
        ))
        .await
        .expect("router should respond");
    assert_readiness_error(response, StatusCode::NOT_FOUND).await;

    let mut failing = MockPropertiesRepo::new();
    failing
        .expect_get_task_dependency_readiness()
        .returning(|_, _, _| Box::pin(async { Err(anyhow::anyhow!("readiness-secret-sentinel")) }));
    let service = PropertiesServiceImpl::new(
        failing,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let response = dependency_readiness_router(service, FakeEntityAccessService::on_team(team_id))
        .oneshot(dependency_readiness_request(
            "project-readiness",
            Some("Bearer valid"),
            serde_json::json!({ "taskIds": [Uuid::nil()] }).to_string(),
        ))
        .await
        .expect("router should respond");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response_body(response).await;
    let value: serde_json::Value = serde_json::from_str(&body).expect("redacted JSON error");
    let object = value.as_object().expect("redacted error object");
    assert_eq!(
        object
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        ["error", "message"].into_iter().collect()
    );
    assert_eq!(value["error"], true);
    assert_eq!(value["message"], "internal server error");
    assert!(!body.contains("readiness-secret-sentinel"));
}
