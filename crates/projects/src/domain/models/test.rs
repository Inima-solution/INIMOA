use std::str::FromStr;

use super::{ProjectOperationalStatus, ProjectPriority};

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
