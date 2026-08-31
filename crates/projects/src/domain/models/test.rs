use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::json;

use super::{
    ProjectOperationalStatus, ProjectOperations, ProjectOperationsValidationError, ProjectOverview,
    ProjectOverviewImmediateChildren, ProjectPriority, ProjectTaskProgress,
    ProjectTaskProgressValidationError, ReplaceProjectOperationsArgs,
    is_valid_operations_transition,
};
use model::project::Project;
use models_permissions::share_permission::access_level::AccessLevel;
use utoipa::PartialSchema;

fn timestamp(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

#[test]
fn task_progress_serializes_only_aggregate_totals_and_enforces_invariants() {
    let zero = ProjectTaskProgress::new(0, 0, false).unwrap();
    assert_eq!(
        serde_json::to_value(&zero).unwrap(),
        json!({"completedTasks": 0, "includedTasks": 0, "hasUnavailableStatuses": false})
    );
    assert!(matches!(
        ProjectTaskProgress::new(2, 1, false),
        Err(ProjectTaskProgressValidationError::InvalidTotals)
    ));
    assert!(matches!(
        ProjectTaskProgress::new(0, 0, true),
        Err(ProjectTaskProgressValidationError::UnavailableWithoutIncludedTask)
    ));
}

fn operations(
    status: ProjectOperationalStatus,
    completed_at: Option<DateTime<Utc>>,
) -> ProjectOperations {
    ProjectOperations {
        project_id: "project-1".to_owned(),
        status,
        priority: ProjectPriority::Normal,
        lead_user_id: None,
        start_date: None,
        target_date: None,
        completed_at,
        created_at: timestamp("2026-08-28T00:00:00Z"),
        updated_at: timestamp("2026-08-28T01:00:00Z"),
        policy: None,
    }
}

fn replacement(status: ProjectOperationalStatus) -> ReplaceProjectOperationsArgs {
    ReplaceProjectOperationsArgs {
        status,
        priority: ProjectPriority::Normal,
        lead_user_id: None,
        start_date: None,
        target_date: None,
        policy: None,
        expected_updated_at: timestamp("2026-08-28T01:00:00Z"),
    }
}

#[test]
fn project_overview_serializes_only_the_fixed_sections_and_immediate_child_counts() {
    let overview = ProjectOverview {
        project: Project {
            id: "project-1".to_owned(),
            name: "Project".to_owned(),
            user_id: "macro|owner@example.com".to_owned(),
            parent_id: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        },
        user_access_level: AccessLevel::View,
        operations: operations(ProjectOperationalStatus::Planned, None),
        immediate_children: ProjectOverviewImmediateChildren {
            child_projects: 1,
            tasks: 2,
            non_task_documents: 3,
            chats: 4,
        },
    };

    let value = serde_json::to_value(overview).unwrap();
    let object = value.as_object().unwrap();
    assert_eq!(
        object.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "project",
            "userAccessLevel",
            "operations",
            "immediateChildren"
        ]
    );
    let children = value["immediateChildren"].as_object().unwrap();
    assert_eq!(
        children.keys().map(String::as_str).collect::<Vec<_>>(),
        ["childProjects", "tasks", "nonTaskDocuments", "chats"]
    );
    assert!(children.values().all(serde_json::Value::is_i64));

    let schema = serde_json::to_value(ProjectOverview::schema()).unwrap();
    let schema_properties = schema["properties"].as_object().unwrap();
    assert_eq!(
        schema_properties
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "immediateChildren",
            "operations",
            "project",
            "userAccessLevel"
        ]
    );
    let child_schema = serde_json::to_value(ProjectOverviewImmediateChildren::schema()).unwrap();
    let child_properties = child_schema["properties"].as_object().unwrap();
    assert_eq!(
        child_properties
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["chats", "childProjects", "nonTaskDocuments", "tasks"]
    );
}

#[test]
fn project_operational_status_uses_only_closed_lowercase_values() {
    for (value, expected) in [
        ("planned", ProjectOperationalStatus::Planned),
        ("active", ProjectOperationalStatus::Active),
        ("paused", ProjectOperationalStatus::Paused),
        ("completed", ProjectOperationalStatus::Completed),
        ("archived", ProjectOperationalStatus::Archived),
    ] {
        assert_eq!(ProjectOperationalStatus::from_str(value).unwrap(), expected);
        assert_eq!(expected.to_string(), value);
        assert_eq!(
            serde_json::to_string(&expected).unwrap(),
            format!("\"{value}\"")
        );
    }
    assert!(ProjectOperationalStatus::from_str("PLANNED").is_err());
}

#[test]
fn project_priority_uses_only_closed_lowercase_values() {
    for (value, expected) in [
        ("low", ProjectPriority::Low),
        ("normal", ProjectPriority::Normal),
        ("high", ProjectPriority::High),
        ("urgent", ProjectPriority::Urgent),
    ] {
        assert_eq!(ProjectPriority::from_str(value).unwrap(), expected);
        assert_eq!(expected.to_string(), value);
        assert_eq!(
            serde_json::to_string(&expected).unwrap(),
            format!("\"{value}\"")
        );
    }
    assert!(ProjectPriority::from_str("Normal").is_err());
}

#[test]
fn operational_transition_matrix_is_closed_and_allows_same_state_metadata_edits() {
    use ProjectOperationalStatus::*;
    for from in [Planned, Active, Paused, Completed, Archived] {
        for to in [Planned, Active, Paused, Completed, Archived] {
            let expected = matches!(
                (from, to),
                (Planned, Planned | Active | Archived)
                    | (Active, Active | Paused | Completed | Archived)
                    | (Paused, Paused | Active | Completed | Archived)
                    | (Completed, Completed | Active | Archived)
                    | (Archived, Archived | Planned)
            );
            assert_eq!(
                is_valid_operations_transition(from, to),
                expected,
                "{from}->{to}"
            );
        }
    }
}

#[test]
fn replacement_enforces_date_and_object_policy_bounds() {
    let mut request = replacement(ProjectOperationalStatus::Planned);
    request.start_date = Some(NaiveDate::from_ymd_opt(2026, 8, 29).unwrap());
    request.target_date = Some(NaiveDate::from_ymd_opt(2026, 8, 28).unwrap());
    assert_eq!(
        request.validate(),
        Err(ProjectOperationsValidationError::DateOrder)
    );
    request.start_date = None;
    request.target_date = None;
    request.policy = Some(json!(["not-object"]));
    assert_eq!(
        request.validate(),
        Err(ProjectOperationsValidationError::PolicyNotObject)
    );
    request.policy = Some(json!({"value": "x".repeat(4096)}));
    assert_eq!(
        request.validate(),
        Err(ProjectOperationsValidationError::PolicyTooLarge)
    );
}

#[test]
fn replacement_completion_and_noop_rules_are_deterministic() {
    use ProjectOperationalStatus::*;
    let now = timestamp("2026-08-28T02:00:00Z");
    let active = operations(Active, None);
    let completed = replacement(Completed).resolve(&active, now).unwrap();
    assert_eq!(completed.completed_at, Some(now));

    let completed_current = operations(Completed, Some(timestamp("2026-08-28T01:30:00Z")));
    let same = replacement(Completed)
        .resolve(&completed_current, now)
        .unwrap();
    assert_eq!(same.completed_at, completed_current.completed_at);
    assert!(same.changed_fields.is_empty());
    let archived = replacement(Archived)
        .resolve(&completed_current, now)
        .unwrap();
    assert_eq!(archived.completed_at, completed_current.completed_at);
    let reopened = replacement(Active)
        .resolve(&completed_current, now)
        .unwrap();
    assert_eq!(reopened.completed_at, None);
    let unarchived = replacement(Planned)
        .resolve(&operations(Archived, completed_current.completed_at), now)
        .unwrap();
    assert_eq!(unarchived.completed_at, None);
}
