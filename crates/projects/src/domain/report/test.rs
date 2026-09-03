use chrono::{Duration, NaiveDate};

use super::*;

fn date(value: &str) -> NaiveDate {
    value.parse().unwrap()
}

fn instant(value: &str) -> chrono::DateTime<Utc> {
    value.parse().unwrap()
}

#[test]
fn completion_excludes_canceled_treats_missing_as_incomplete_and_keeps_zero_denominator() {
    let result = completion([
        Some(ProjectReportTaskStatus::Completed),
        Some(ProjectReportTaskStatus::Canceled),
        Some(ProjectReportTaskStatus::InReview),
        None,
    ])
    .unwrap();
    assert_eq!(result.completed_tasks(), 1);
    assert_eq!(result.included_tasks(), 3);

    assert_eq!(
        completion([Some(ProjectReportTaskStatus::Canceled)]).unwrap(),
        ProjectReportCompletion::default()
    );
}

#[test]
fn completion_and_workload_counts_fail_closed_on_overflow() {
    let mut completion = ProjectReportCompletion {
        completed_tasks: 0,
        included_tasks: u64::MAX,
    };
    assert_eq!(
        completion.checked_record(None),
        Err(ProjectReportArithmeticError::CountOverflow)
    );

    let mut workload = ProjectReportWorkload {
        allocation_by_assignee: BTreeMap::new(),
        unassigned_wip_tasks: u64::MAX,
    };
    assert_eq!(
        workload.checked_add_unassigned(),
        Err(ProjectReportArithmeticError::CountOverflow)
    );
    assert_eq!(
        ProjectReportRational {
            numerator: u128::MAX,
            denominator: 1,
        }
        .checked_add_unit_fraction(1),
        None
    );
}

#[test]
fn current_state_formulas_share_open_boundaries_and_allow_blocked_wip_overlap() {
    let as_of = date("2026-09-03");

    assert!(is_overdue(
        Some(ProjectReportTaskStatus::InProgress),
        Some(date("2026-09-02")),
        as_of
    ));
    assert!(!is_overdue(
        Some(ProjectReportTaskStatus::InProgress),
        Some(as_of),
        as_of
    ));
    assert!(!is_overdue(
        Some(ProjectReportTaskStatus::Completed),
        Some(date("2026-09-02")),
        as_of
    ));
    assert!(is_wip(Some(ProjectReportTaskStatus::InReview)));
    assert!(is_blocked(Some(ProjectReportTaskStatus::InReview), true));
}

#[test]
fn milestone_risk_is_open_milestone_and_overdue_or_blocked_union() {
    let as_of = date("2026-09-03");

    assert!(is_milestone_at_risk(
        Some(ProjectReportTaskStatus::NotStarted),
        true,
        Some(date("2026-09-02")),
        false,
        as_of,
    ));
    assert!(is_milestone_at_risk(
        Some(ProjectReportTaskStatus::InProgress),
        true,
        Some(date("2026-09-02")),
        true,
        as_of,
    ));
    assert!(!is_milestone_at_risk(
        Some(ProjectReportTaskStatus::Canceled),
        true,
        Some(date("2026-09-02")),
        true,
        as_of,
    ));
    assert!(!is_milestone_at_risk(
        Some(ProjectReportTaskStatus::InProgress),
        false,
        Some(date("2026-09-02")),
        true,
        as_of,
    ));
}

#[test]
fn report_window_validates_bounds_and_freezes_non_utc_dst_instants() {
    let zone = chrono_tz::America::New_York;
    let window = ProjectReportWindow::new(zone, date("2026-03-08"), date("2026-03-09")).unwrap();
    assert_eq!(window.zone(), zone);
    assert_eq!(window.start_inclusive(), instant("2026-03-08T05:00:00Z"));
    assert_eq!(window.end_exclusive(), instant("2026-03-09T04:00:00Z"));
    assert_eq!(
        window.end_exclusive() - window.start_inclusive(),
        Duration::hours(23)
    );
    assert!(window.contains(instant("2026-03-08T05:00:00Z")));
    assert!(!window.contains(instant("2026-03-09T04:00:00Z")));
    assert_eq!(
        ProjectReportWindow::new(zone, date("2026-03-08"), date("2026-03-08")),
        Err(ProjectReportWindowError::Empty)
    );
    assert_eq!(
        ProjectReportWindow::new(zone, date("2026-03-09"), date("2026-03-08")),
        Err(ProjectReportWindowError::Empty)
    );
    assert_eq!(
        ProjectReportWindow::new(zone, date("2025-01-01"), date("2026-02-06")),
        Err(ProjectReportWindowError::TooWide)
    );

    // Pacific/Apia skipped the entire 2011-12-30 local date.
    assert_eq!(
        ProjectReportWindow::new(
            chrono_tz::Pacific::Apia,
            date("2011-12-30"),
            date("2011-12-31")
        ),
        Err(ProjectReportWindowError::NonUniqueBoundary)
    );
    // America/Havana repeated local midnight at the 2026 fall-back transition.
    assert_eq!(
        ProjectReportWindow::new(
            chrono_tz::America::Havana,
            date("2026-11-01"),
            date("2026-11-02")
        ),
        Err(ProjectReportWindowError::NonUniqueBoundary)
    );
}

#[test]
fn trailing_window_contains_28_viewer_local_dates() {
    let window =
        ProjectReportWindow::trailing_28_days(chrono_tz::Asia::Seoul, date("2026-09-03")).unwrap();
    assert_eq!(window.start_date(), date("2026-08-07"));
    assert_eq!(window.end_date_exclusive(), date("2026-09-04"));
    assert_eq!(window.start_inclusive(), instant("2026-08-06T15:00:00Z"));
    assert_eq!(window.end_exclusive(), instant("2026-09-03T15:00:00Z"));
}

#[test]
fn throughput_selects_one_latest_completion_and_does_not_read_current_status() {
    let window =
        ProjectReportWindow::trailing_28_days(chrono_tz::Asia::Seoul, date("2026-09-03")).unwrap();
    let selected = latest_completion_in_window(
        [
            instant("2026-08-01T12:00:00Z"),
            instant("2026-08-10T12:00:00Z"),
            instant("2026-08-20T12:00:00Z"),
        ],
        window,
    );

    // A reopen after this transition does not remove it from the historical cohort;
    // current Status is deliberately not an input to the selection function.
    assert_eq!(selected, Some(instant("2026-08-20T12:00:00Z")));
}

#[test]
fn lead_time_rejects_negative_subseconds_and_accepts_non_negative_subseconds() {
    let created = instant("2026-08-10T10:00:00Z");
    assert_eq!(
        lead_time_seconds(created, created - Duration::milliseconds(1)),
        None
    );
    assert_eq!(lead_time_seconds(created, created), Some(0));
    assert_eq!(
        lead_time_seconds(created, created + Duration::milliseconds(1)),
        Some(0)
    );
    assert_eq!(
        lead_time_seconds(created, instant("2026-08-12T10:00:01Z")),
        Some(172_801)
    );

    assert_eq!(
        ProjectReportLeadTime::new(10, 3).unwrap().mean_seconds(),
        Some(3)
    );
    assert_eq!(
        ProjectReportLeadTime::new(0, 0).unwrap().mean_seconds(),
        None
    );
    assert_eq!(
        ProjectReportLeadTime::new(1, 0),
        Err(ProjectReportArithmeticError::InvalidLeadTime)
    );
}

#[test]
fn workload_splits_each_wip_task_into_exact_units_and_tracks_unassigned() {
    let alice = "macro|alice@example.com".to_string();
    let bob = "macro|bob@example.com".to_string();
    let duplicated = vec![alice.clone(), alice.clone(), bob.clone()];
    let solo_alice = vec![alice.clone()];
    let unassigned = Vec::new();
    let completed = vec![alice.clone()];

    let result = workload([
        (
            Some(ProjectReportTaskStatus::InProgress),
            duplicated.as_slice(),
        ),
        (
            Some(ProjectReportTaskStatus::InProgress),
            solo_alice.as_slice(),
        ),
        (
            Some(ProjectReportTaskStatus::InReview),
            unassigned.as_slice(),
        ),
        (
            Some(ProjectReportTaskStatus::Completed),
            completed.as_slice(),
        ),
    ])
    .unwrap();

    assert_eq!(
        result.allocation_by_assignee().get(&alice),
        ProjectReportRational::new(3, 2).as_ref()
    );
    assert_eq!(
        result.allocation_by_assignee().get(&bob),
        ProjectReportRational::new(1, 2).as_ref()
    );
    assert_eq!(result.unassigned_wip_tasks(), 1);
    assert_eq!(ProjectReportRational::new(1, 0), None);
    assert_eq!(ProjectReportRational::new(6, 4).unwrap().numerator(), 3);
    assert_eq!(ProjectReportRational::new(6, 4).unwrap().denominator(), 2);
}

#[test]
fn whole_snapshot_serialization_forces_both_history_metrics_unavailable() {
    let snapshot = ProjectTaskReportSnapshot::current(
        date("2026-09-03"),
        ProjectReportMetric::Available {
            value: completion([
                Some(ProjectReportTaskStatus::Completed),
                Some(ProjectReportTaskStatus::InProgress),
            ])
            .unwrap(),
        },
        ProjectReportMetric::Available { value: 1 },
        ProjectReportMetric::Available { value: 1 },
        ProjectReportMetric::Unavailable {
            reason: ProjectReportUnavailableReason::MalformedCurrentTaskData,
        },
        ProjectReportMetric::Available {
            value: ProjectReportMilestones::new(2, 1).unwrap(),
        },
        ProjectReportMetric::Unavailable {
            reason: ProjectReportUnavailableReason::UnavailableWorkloadData,
        },
    );

    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        serde_json::json!({
            "asOfDate": "2026-09-03",
            "scope": "direct_live_tasks",
            "completion": {
                "availability": "available",
                "value": {"completedTasks": 1, "includedTasks": 2}
            },
            "wipTasks": {"availability": "available", "value": 1},
            "overdueTasks": {"availability": "available", "value": 1},
            "blockedTasks": {
                "availability": "unavailable",
                "reason": "malformed_current_task_data"
            },
            "milestones": {
                "availability": "available",
                "value": {"openMilestones": 2, "atRiskMilestones": 1}
            },
            "workload": {
                "availability": "unavailable",
                "reason": "unavailable_workload_data"
            },
            "throughput": {
                "availability": "unavailable",
                "reason": "missing_durable_completion_transitions"
            },
            "leadTime": {
                "availability": "unavailable",
                "reason": "missing_durable_completion_transitions"
            }
        })
    );
}

#[test]
fn milestone_totals_enforce_the_at_risk_subset() {
    let milestones = ProjectReportMilestones::new(2, 1).unwrap();
    assert_eq!(milestones.open_milestones(), 2);
    assert_eq!(milestones.at_risk_milestones(), 1);
    assert_eq!(
        ProjectReportMilestones::new(0, 1),
        Err(ProjectReportValidationError::InvalidMilestoneTotals)
    );
}
