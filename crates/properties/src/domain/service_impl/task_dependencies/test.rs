use super::{PropertiesErr, parse_dependencies};
use crate::domain::model::{EditReceipt, TaskDependencyMutationOutcome, ViewReceipt};
use crate::domain::ports::{MockNotificationService, MockPermissionService, MockPropertiesRepo};
use crate::domain::service_impl::PropertiesServiceImpl;
use entity_access::domain::models::EntityType as AccessEntityType;
use entity_access::domain::models::{
    AccessLevel, BotId, BotReceiptScope, Entity, EntityAccessAuth, EntityPermission,
};
use macro_user_id::cowlike::CowLike;
use macro_user_id::user_id::MacroUserIdStr;
use models_properties::api::requests::SetPropertyValue;
use models_properties::{EntityReference, EntityType};
use uuid::Uuid;

#[test]
fn parser_preserves_order_and_rejects_duplicate_before_conversion() {
    let task = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let parsed = parse_dependencies(
        task,
        Some(SetPropertyValue::MultiEntityReference {
            references: vec![
                EntityReference::new(first.to_string(), EntityType::Task),
                EntityReference::new(second.to_string(), EntityType::Task),
            ],
        }),
    )
    .unwrap();
    assert_eq!(parsed, vec![first, second]);

    let duplicate = parse_dependencies(
        task,
        Some(SetPropertyValue::MultiEntityReference {
            references: vec![
                EntityReference::new(first.to_string(), EntityType::Task),
                EntityReference::new(first.to_string(), EntityType::Task),
            ],
        }),
    );
    assert!(
        matches!(duplicate, Err(PropertiesErr::Validation(message)) if message == "A task dependency may only be listed once")
    );
}

#[test]
fn parser_rejects_wrong_reference_shape_and_self() {
    let task = Uuid::new_v4();
    assert!(matches!(
        parse_dependencies(task, Some(SetPropertyValue::EntityReference { reference: EntityReference::new(task.to_string(), EntityType::Task) })),
        Err(PropertiesErr::Validation(message)) if message == "Depends On requires task references"
    ));
    assert!(matches!(
        parse_dependencies(task, Some(SetPropertyValue::MultiEntityReference { references: vec![EntityReference::new(task.to_string(), EntityType::Task)] })),
        Err(PropertiesErr::Validation(message)) if message == "A task cannot depend on itself"
    ));
    for reference in [
        EntityReference::new(Uuid::new_v4().to_string(), EntityType::Document),
        EntityReference::with_message_id(
            Uuid::new_v4().to_string(),
            EntityType::Task,
            Uuid::new_v4(),
        ),
        EntityReference::new("not-a-uuid", EntityType::Task),
    ] {
        assert!(matches!(
            parse_dependencies(task, Some(SetPropertyValue::MultiEntityReference { references: vec![reference] })),
            Err(PropertiesErr::Validation(message)) if message == "Depends On requires task references"
        ));
    }
}

#[tokio::test]
async fn self_and_duplicate_rejections_never_reach_repository() {
    let task = Uuid::new_v4();
    let dependency = Uuid::new_v4();
    for references in [
        vec![EntityReference::new(task.to_string(), EntityType::Task)],
        vec![
            EntityReference::new(dependency.to_string(), EntityType::Task),
            EntityReference::new(dependency.to_string(), EntityType::Task),
        ],
    ] {
        let mut repo = MockPropertiesRepo::new();
        repo.expect_replace_task_dependencies().never();
        let service = PropertiesServiceImpl::new(
            repo,
            None::<MockPermissionService>,
            None::<MockNotificationService>,
        );
        let access = EditReceipt::dangerously_assert_internal_user(
            &task.to_string(),
            AccessEntityType::Document,
        );
        assert!(matches!(
            service
                .handle_task_dependencies_property(
                    &access,
                    Some(SetPropertyValue::MultiEntityReference { references }),
                )
                .await,
            Err(PropertiesErr::Validation(_))
        ));
    }
}

fn receipt(task: Uuid) -> EditReceipt {
    EditReceipt::dangerously_assert_authenticated_user(
        MacroUserIdStr::parse_from_str("macro|dependency@test.com")
            .unwrap()
            .into_owned(),
        &task.to_string(),
        AccessEntityType::Document,
    )
}

#[tokio::test]
async fn human_second_receipt_denial_maps_unavailable_without_repo_write() {
    let task = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut repo = MockPropertiesRepo::new();
    repo.expect_replace_task_dependencies().never();
    let mut permission = MockPermissionService::new();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = calls.clone();
    permission
        .expect_mint_view_receipt()
        .times(2)
        .withf(move |_, id, ty| {
            (id == first.to_string() || id == second.to_string())
                && *ty == AccessEntityType::Document
        })
        .returning(move |_, id, ty| {
            let call = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call == 0 {
                assert_eq!(id, first.to_string());
                let receipt = ViewReceipt::dangerously_assert_internal_user(id, ty);
                Box::pin(async move { Ok(receipt) })
            } else {
                assert_eq!(id, second.to_string());
                Box::pin(async { Err(anyhow::anyhow!("denied")) })
            }
        });
    let service =
        PropertiesServiceImpl::new(repo, Some(permission), None::<MockNotificationService>);
    let result = service
        .handle_task_dependencies_property(
            &receipt(task),
            Some(SetPropertyValue::MultiEntityReference {
                references: vec![
                    EntityReference::new(first.to_string(), EntityType::Task),
                    EntityReference::new(second.to_string(), EntityType::Task),
                ],
            }),
        )
        .await;
    assert!(matches!(
        result,
        Err(PropertiesErr::TaskDependenciesUnavailable)
    ));
}

#[tokio::test]
async fn bot_and_unauthenticated_dependency_writes_are_denied_without_repo() {
    let task = Uuid::new_v4();
    let target = Uuid::new_v4();
    let bot = EditReceipt::dangerously_assert_bot(
        BotId::new_from_uuid(Uuid::new_v4()).into_storage_id(),
        BotReceiptScope::Team {
            team_id: Uuid::new_v4(),
        },
        &task.to_string(),
        AccessEntityType::Document,
    );
    let unauthenticated = EditReceipt::try_new(
        EntityAccessAuth::Unauthenticated,
        Entity {
            entity_id: task.to_string(),
            entity_type: AccessEntityType::Document,
        },
        EntityPermission::AccessLevel {
            access_level: AccessLevel::Owner,
        },
    )
    .unwrap();
    for access in [bot, unauthenticated] {
        let mut repo = MockPropertiesRepo::new();
        repo.expect_replace_task_dependencies().never();
        let service = PropertiesServiceImpl::new(
            repo,
            None::<MockPermissionService>,
            None::<MockNotificationService>,
        );
        assert!(matches!(
            service
                .handle_task_dependencies_property(
                    &access,
                    Some(SetPropertyValue::MultiEntityReference {
                        references: vec![EntityReference::new(
                            target.to_string(),
                            EntityType::Task
                        )]
                    })
                )
                .await,
            Err(PropertiesErr::PermissionDenied)
        ));
    }
}

#[tokio::test]
async fn repository_dependency_outcomes_map_to_frozen_errors() {
    let task = Uuid::new_v4();
    for (outcome, expected) in [
        (
            TaskDependencyMutationOutcome::Unavailable,
            PropertiesErr::TaskDependenciesUnavailable,
        ),
        (
            TaskDependencyMutationOutcome::Cycle,
            PropertiesErr::TaskDependencyCycle,
        ),
    ] {
        let mut repo = MockPropertiesRepo::new();
        repo.expect_replace_task_dependencies()
            .return_once(move |_, _| Box::pin(async move { Ok(outcome) }));
        let service = PropertiesServiceImpl::new(
            repo,
            None::<MockPermissionService>,
            None::<MockNotificationService>,
        );
        let access = EditReceipt::dangerously_assert_internal_user(
            &task.to_string(),
            AccessEntityType::Document,
        );
        let result = service
            .handle_task_dependencies_property(&access, None)
            .await;
        assert_eq!(result.unwrap_err().to_string(), expected.to_string());
    }
}

#[tokio::test]
async fn malformed_source_id_is_rejected_before_repository_access() {
    let mut repo = MockPropertiesRepo::new();
    repo.expect_replace_task_dependencies().never();
    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let access = EditReceipt::dangerously_assert_internal_user(
        "not-a-task-uuid",
        AccessEntityType::Document,
    );

    assert!(matches!(
        service
            .handle_task_dependencies_property(&access, None)
            .await,
        Err(PropertiesErr::Validation(message))
            if message == "Depends On requires task references"
    ));
}

#[tokio::test]
async fn none_and_empty_dependency_values_reach_repo_as_empty_slice() {
    let task = Uuid::new_v4();
    for value in [
        None,
        Some(SetPropertyValue::MultiEntityReference { references: vec![] }),
    ] {
        let mut repo = MockPropertiesRepo::new();
        repo.expect_replace_task_dependencies()
            .withf(move |id, ids| *id == task && ids.is_empty())
            .return_once(|_, _| Box::pin(async { Ok(TaskDependencyMutationOutcome::Unavailable) }));
        let permission = MockPermissionService::new();
        let service =
            PropertiesServiceImpl::new(repo, Some(permission), None::<MockNotificationService>);
        assert!(matches!(
            service
                .handle_task_dependencies_property(&receipt(task), value)
                .await,
            Err(PropertiesErr::TaskDependenciesUnavailable)
        ));
    }
}

#[tokio::test]
async fn human_success_mints_two_document_receipts_then_calls_repo_in_order() {
    let task = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut repo = MockPropertiesRepo::new();
    repo.expect_replace_task_dependencies()
        .withf(move |id, ids| *id == task && ids == [first, second])
        .return_once(|_, _| Box::pin(async { Ok(TaskDependencyMutationOutcome::Unavailable) }));
    let mut permission = MockPermissionService::new();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = calls.clone();
    permission
        .expect_mint_view_receipt()
        .times(2)
        .withf(move |_, id, ty| {
            (id == first.to_string() || id == second.to_string())
                && *ty == AccessEntityType::Document
        })
        .returning(move |_, id, ty| {
            let call = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert_eq!(id, if call == 0 { first } else { second }.to_string());
            let receipt = ViewReceipt::dangerously_assert_internal_user(id, ty);
            Box::pin(async move { Ok(receipt) })
        });
    let service =
        PropertiesServiceImpl::new(repo, Some(permission), None::<MockNotificationService>);
    let result = service
        .handle_task_dependencies_property(
            &receipt(task),
            Some(SetPropertyValue::MultiEntityReference {
                references: vec![
                    EntityReference::new(first.to_string(), EntityType::Task),
                    EntityReference::new(second.to_string(), EntityType::Task),
                ],
            }),
        )
        .await;
    assert!(matches!(
        result,
        Err(PropertiesErr::TaskDependenciesUnavailable)
    ));
}

#[tokio::test]
async fn internal_dependency_write_skips_permission_service() {
    let task = Uuid::new_v4();
    let target = Uuid::new_v4();
    let mut repo = MockPropertiesRepo::new();
    repo.expect_replace_task_dependencies()
        .withf(move |id, ids| *id == task && ids == [target])
        .return_once(|_, _| Box::pin(async { Ok(TaskDependencyMutationOutcome::Unavailable) }));
    let service = PropertiesServiceImpl::new(
        repo,
        None::<MockPermissionService>,
        None::<MockNotificationService>,
    );
    let access = EditReceipt::dangerously_assert_internal_user(
        &task.to_string(),
        AccessEntityType::Document,
    );
    assert!(matches!(
        service
            .handle_task_dependencies_property(
                &access,
                Some(SetPropertyValue::MultiEntityReference {
                    references: vec![EntityReference::new(target.to_string(), EntityType::Task)]
                })
            )
            .await,
        Err(PropertiesErr::TaskDependenciesUnavailable)
    ));
}
