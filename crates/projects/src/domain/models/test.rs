use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::json;

use super::{
    is_valid_operations_transition, ProjectOperationalStatus, ProjectOperations,
    ProjectOperationsValidationError, ProjectPriority, ReplaceProjectOperationsArgs,
};

fn timestamp(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
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
