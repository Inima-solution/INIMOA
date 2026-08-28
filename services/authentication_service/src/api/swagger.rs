use github::domain::models::{
    EnrichGithubPullRequestsProxyRequest, EnrichGithubPullRequestsResponse,
    EnrichedGithubPullRequest, GithubPullRequestCheckRun, GithubPullRequestComment,
    GithubPullRequestRef, GithubPullRequestStatus,
};
use model::authentication::login::request::{AppleLoginRequest, PasswordRequest};
use teams::domain::model::{
    PatchTeamCrmSettingsRequest, PatchTeamCrmSettingsResponse, PatchTeamRequest, PatchTeamUserRole,
    Team, TeamInviteDetails, TeamMember, TeamPlan, TeamRole, TeamWithMembers,
};
use teams::inbound::axum_router::get_team_invites::TeamInvitesResponse as TeamTeamInvitesResponse;
use teams::inbound::axum_router::get_user_invites::TeamInvitesResponse as UserTeamInvitesResponse;
use teams::inbound::axum_router::toggle_auto_join_domain::ToggleAutoJoinDomainResponse;
use teams::inbound::axum_router::toggle_non_admin_invites::ToggleNonAdminInvitesResponse;
use teams::inbound::axum_router::{
    create_team::CreateTeamRequest, invite_to_team::InviteToTeamRequest,
};
use user_quota::UserQuota;
use utoipa::OpenApi;

use crate::api::business_role_change::{BusinessRoleChangeRequest, BusinessRoleChangeResponse};
use crate::api::cursor_api_key::{CursorApiKeyStatus, put_cursor_api_key::PutCursorApiKeyRequest};
use crate::api::email::generate_email_link::GenerateEmailLinkRequest;
use crate::api::email::resend_fusionauth_verify_user_email::ResendFusionauthVerifyUserEmailRequest;
use crate::api::jwt::macro_api_token::MacroApiTokenResponse;
use crate::api::link::create_in_progress_link::CreateInProgressLinkResponse;
use crate::api::link::github::{GithubLinkStatusResponse, InitGithubLinkResponse};
use crate::api::link::gmail::{GmailLinkStatusResponse, InitGmailLinkResponse};
use crate::api::link::outlook::InitOutlookLinkResponse;
use crate::api::merge::create_merge_request::CreateAccountMergeRequest;
use crate::api::reauth::{ReauthenticateRequest, ReauthenticateResponse};
use crate::api::user::create_user::CreateUserRequest;
use crate::api::user::get_legacy_user_permissions::GetLegacyUserPermissionsResponse;
use crate::api::user::get_user_link_exists::UserLinkResponse;
use crate::api::user::get_user_organization::UserOrganizationResponse;
use crate::api::user::patch_tutorial::PatchUserTutorialRequest;
use crate::api::user::patch_user_group::PatchUserGroupRequest;
use crate::api::user::patch_user_onboarding::PatchUserOnboardingRequest;
use crate::api::user::post_get_names::PostGetNamesRequestBody;
use crate::api::user::post_get_names_with_email::GetNamesWithEmailRequestBody;
use crate::api::user::stripe::StripeSessionResponse;
use crate::api::user::stripe::create_checkout_session_v2::CreateCheckoutSessionV2Request;
use crate::api::user::stripe::create_portal_session::CreatePortalSessionRequest;
use crate::api::{
    email, github_pull_requests, health, jwt, link, login, logout, merge, mobile_welcome_email,
    oauth, oauth2, permissions, session, user,
};
use model::authentication::login::response::SsoRequiredResponse;
use model::authentication::{
    login::request::PasswordlessRequest, permission::Permission, user::GetUserInfo,
};
use model::response::{EmptyResponse, ErrorResponse, UserTokensResponse};
use model::user::{
    ProfilePictureQueryParams, ProfilePictures, PutUserNameQueryParams, UserName, UserNames,
    UserProfilePicture,
};

#[derive(OpenApi)]
#[openapi(
        info(
                terms_of_service = "https://macro.com/terms",
        ),
        paths(
                /// /health
                health::health_handler,

                /// /permissions
                permissions::get_permissions::handler,
                permissions::get_user_permissions::handler,

                /// /login
                login::passwordless::handler,
                login::sso::handler,
                login::password::handler,
                login::apple::handler,

                /// /logout
                logout::handler,

                /// /link
                link::create_in_progress_link::handler,
                link::github::init_github_link_handler,
                link::github::delete_github_link_handler,
                link::github::check_github_link_status_handler,
                link::gmail::init_gmail_link_handler,
                link::gmail::check_gmail_link_status_handler,
                link::outlook::init_outlook_link_handler,

                // Cursor API key (settings -> Connections). Fully qualified:
                // the `cursor_api_key` crate shadows the module of the same
                // name in this path position.
                crate::api::cursor_api_key::get_cursor_api_key::handler,
                crate::api::cursor_api_key::put_cursor_api_key::handler,
                crate::api::cursor_api_key::delete_cursor_api_key::handler,
                crate::api::cursor_api_key::list_cursor_models::handler,
                crate::api::cursor_api_key::put_cursor_default_model::handler,

                /// /github_pull_requests
                github_pull_requests::handler,

                /// /oauth
                oauth::oauth_redirect::handler,
                oauth::passwordless_callback::handler,

                oauth2::handler,

                /// /jwt
                jwt::refresh::handler,
                jwt::macro_api_token::handler,

                /// /user
                user::create_user::handler,
                user::get_user_info::handler,
                user::delete_user::handler,
                user::post_profile_pictures::handler,
                user::put_profile_picture::handler,
                user::put_name::handler,
                user::get_name::handler,
                user::patch_user_group::handler,
                user::patch_user_onboarding::handler,
                user::post_get_names::handler_external,
                user::post_get_names_with_email::handler,
                user::get_user_link_exists::handler,
                user::get_user_organization::handler,
                user::get_user_quota::handler,
                user::get_legacy_user_permissions::handler,
                user::patch_tutorial::handler,
                user::stripe::create_checkout_session_v2::create_checkout_session,
                user::stripe::create_portal_session::create_portal_session,

                /// /session
                session::session_login::handler,
                session::session_creation::handler,

                /// /email
                email::verify_fusionauth_user_email::handler,
                email::resend_fusionauth_verify_user_email::handler,
                email::generate_email_link::handler,
                email::verify_email_link::handler,

                /// /team
                teams::inbound::axum_router::create_team::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::delete_team::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::join_team::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::get_team::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::invite_to_team::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::get_team_invites::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::patch_team::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::patch_team_crm_settings::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::toggle_auto_join_domain::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::toggle_non_admin_invites::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::reject_invitation::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::get_user_invites::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::get_user_teams::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::remove_user_from_team::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                teams::inbound::axum_router::delete_team_invite::handler::<crate::api::context::TeamsServiceType, crate::api::context::EntityAccessServiceType, crate::api::context::AuthorizationService>,
                crate::api::reauth::handler,
                crate::api::business_role_change::grant_handler,
                crate::api::business_role_change::revoke_handler,

                /// /referral
                referral::inbound::axum_router::get_referral_code_handler::<crate::api::context::ReferralServiceType, crate::api::context::RateLimiter, crate::api::context::AuthorizationService>,
                referral::inbound::axum_router::post_referral_invite_handler::<crate::api::context::ReferralServiceType, crate::api::context::RateLimiter, crate::api::context::AuthorizationService>,

                /// /mobile-welcome-email
                mobile_welcome_email::handler,

                /// /merge
                merge::create_merge_request::handler,
                merge::verify_merge_request::handler,
        ),
        components(
            schemas(
                        Permission,
                        PasswordlessRequest,
                        PasswordRequest,
                        SsoRequiredResponse,
                        EmptyResponse,
                        ErrorResponse,
                        GetUserInfo,
                        ProfilePictures,
                        UserProfilePicture,
                        AppleLoginRequest,
                        ProfilePictureQueryParams,
                        PutUserNameQueryParams,
                        UserName,
                        UserNames,
                        GetNamesWithEmailRequestBody,
                        PostGetNamesRequestBody,
                        UserTokensResponse,
                        UserLinkResponse,
                        MacroApiTokenResponse,
                        CreateUserRequest,
                        ResendFusionauthVerifyUserEmailRequest,
                        GenerateEmailLinkRequest,
                        CreateInProgressLinkResponse,
                        InitGithubLinkResponse,
                        GithubLinkStatusResponse,
                        InitGmailLinkResponse,
                        GmailLinkStatusResponse,
                        InitOutlookLinkResponse,
                        CursorApiKeyStatus,
                        PutCursorApiKeyRequest,

                        // GitHub pull requests
                        EnrichGithubPullRequestsProxyRequest,
                        EnrichGithubPullRequestsResponse,
                        EnrichedGithubPullRequest,
                        GithubPullRequestCheckRun,
                        GithubPullRequestComment,
                        GithubPullRequestRef,
                        GithubPullRequestStatus,

                        UserQuota,
                        UserOrganizationResponse,
                        GetLegacyUserPermissionsResponse,
                        PatchUserTutorialRequest,

                        // Stripe
                        CreateCheckoutSessionV2Request,
                        CreatePortalSessionRequest,
                        StripeSessionResponse,

                        // User onboarding
                        PatchUserGroupRequest,
                        PatchUserOnboardingRequest,

                        // Teams
                        models_permissions::share_permission::LinkShare,
                        TeamRole,
                        TeamMember,
                        Team,
                        TeamPlan,
                        TeamWithMembers,
                        TeamInviteDetails,
                        ReauthenticateRequest,
                        ReauthenticateResponse,
                        BusinessRoleChangeRequest,
                        BusinessRoleChangeResponse,
                        CreateTeamRequest,
                        InviteToTeamRequest,
                        PatchTeamRequest,
                        PatchTeamUserRole,
                        PatchTeamCrmSettingsRequest,
                        PatchTeamCrmSettingsResponse,
                        ToggleAutoJoinDomainResponse,
                        ToggleNonAdminInvitesResponse,
                        TeamTeamInvitesResponse,
                        UserTeamInvitesResponse,

                        // Mobile welcome email
                        mobile_welcome_email::SendMobileWelcomeEmailRequest,
                        mobile_welcome_email::SendMobileWelcomeEmailResponse,

                        // Merge
                        CreateAccountMergeRequest,
                ),
        ),
        tags(
            (name = "auth service", description = "Macro Authentication Service")
        )
    )]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_pull_requests_openapi_includes_enrich_path() {
        let openapi = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let operation = &openapi["paths"]["/github_pull_requests/enrich"]["post"];

        assert_eq!(operation["operationId"], "enrich_github_pull_requests");
        assert_eq!(
            operation["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str(),
            Some("#/components/schemas/EnrichGithubPullRequestsProxyRequest")
        );
        assert_eq!(
            operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"].as_str(),
            Some("#/components/schemas/EnrichGithubPullRequestsResponse")
        );
        assert!(operation["responses"].get("428").is_some());
    }

    #[test]
    fn github_link_status_openapi_includes_path_and_schema() {
        let openapi = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let operation = &openapi["paths"]["/link/github/status"]["get"];

        assert_eq!(operation["operationId"], "check_github_link_status");
        assert_eq!(
            operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"].as_str(),
            Some("#/components/schemas/GithubLinkStatusResponse")
        );
        assert!(operation["responses"].get("428").is_some());
        assert!(
            openapi["components"]["schemas"]
                .get("GithubLinkStatusResponse")
                .is_some()
        );
    }

    #[test]
    fn github_pull_requests_openapi_includes_components() {
        let openapi = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let schemas = &openapi["components"]["schemas"];

        for schema_name in [
            "EnrichGithubPullRequestsProxyRequest",
            "EnrichGithubPullRequestsResponse",
            "EnrichedGithubPullRequest",
            "GithubPullRequestCheckRun",
            "GithubPullRequestComment",
            "GithubPullRequestRef",
            "GithubPullRequestStatus",
        ] {
            assert!(
                schemas.get(schema_name).is_some(),
                "missing schema component {schema_name}"
            );
        }

        let request_properties = &schemas["EnrichGithubPullRequestsProxyRequest"]["properties"];
        assert!(request_properties.get("pullRequests").is_some());
        assert!(request_properties.get("macroUserId").is_none());

        let response_properties = &schemas["EnrichedGithubPullRequest"]["properties"];
        assert!(response_properties.get("comments").is_some());
        assert!(response_properties.get("checks").is_some());
        assert!(
            response_properties
                .get("participantGithubUserIds")
                .is_some()
        );
    }

    #[test]
    fn team_reauthentication_openapi_includes_typed_contract() {
        let openapi = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let operation = &openapi["paths"]["/team/reauth"]["post"];

        assert_eq!(
            operation["operationId"],
            "reauthenticate_for_team_role_change"
        );
        assert_eq!(
            operation["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str(),
            Some("#/components/schemas/ReauthenticateRequest")
        );
        assert_eq!(
            operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"].as_str(),
            Some("#/components/schemas/ReauthenticateResponse")
        );
        assert!(operation["responses"].get("429").is_some());
    }

    #[test]
    fn team_business_role_change_openapi_includes_both_paths() {
        let openapi = serde_json::to_value(ApiDoc::openapi()).unwrap();

        for (path, operation_id) in [
            ("/team/business-role/grant", "grant_team_business_role"),
            ("/team/business-role/revoke", "revoke_team_business_role"),
        ] {
            let operation = &openapi["paths"][path]["post"];
            assert_eq!(operation["operationId"], operation_id);
            assert_eq!(
                operation["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str(),
                Some("#/components/schemas/BusinessRoleChangeRequest")
            );
            assert_eq!(
                operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"]
                    .as_str(),
                Some("#/components/schemas/BusinessRoleChangeResponse")
            );
            for status in ["400", "403", "404", "409", "500"] {
                assert!(
                    operation["responses"].get(status).is_some(),
                    "missing {status}"
                );
            }
        }

        // The body carries only the change payload; team and actor come from
        // the authenticated receipts and must not be request fields.
        let properties =
            &openapi["components"]["schemas"]["BusinessRoleChangeRequest"]["properties"];
        for field in [
            "target",
            "business_role",
            "reauthentication_receipt",
            "reason",
        ] {
            assert!(properties.get(field).is_some(), "missing field {field}");
        }
        assert!(properties.get("team_id").is_none());
        assert!(properties.get("actor").is_none());

        // The existing reauthentication route stays registered.
        assert!(openapi["paths"]["/team/reauth"]["post"].is_object());
    }
}
