//! Canonical formulas for project task reports.
//!
//! Reports use the same direct, live Task population as [`super::models::ProjectOverview`].
//! Adapters must fail a metric closed when a non-null property has a malformed or unknown
//! representation; these helpers operate only after that validation boundary.
//!
//! Formula contract:
//!
//! - completion is `Completed / all non-Canceled tasks`; a missing Status is Not Started;
//! - overdue is an open task whose due calendar date is strictly before `as_of_date`;
//! - blocked is an open task with authoritative blocked dependency readiness;
//! - WIP is In Progress or In Review, and may overlap blocked;
//! - throughput is the number of distinct tasks with a transition into Completed in a half-open
//!   window `[start, end)`; at most the latest transition per task is selected, regardless of the
//!   task's current status, so reopening cannot rewrite a past window;
//! - lead time is `task.created_at -> selected completion transition`; aggregates publish both
//!   summed seconds and sample count instead of persisting a rounded average;
//! - milestone risk is an open milestone that is overdue or authoritatively blocked;
//! - workload divides each current WIP task into exact `1/N` units across its `N` distinct
//!   assignees, while WIP with no assignee is counted separately.
//!
//! Throughput and lead time remain unavailable until a durable, permission-scoped completion
//! transition source exists. The eventually-consistent activity feed is not that source.

#[cfg(test)]
mod test;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

/// Maximum local calendar dates accepted by one report window.
pub const MAX_PROJECT_REPORT_WINDOW_DAYS: i64 = 400;

/// The only task scope supported by the project report contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectReportScope {
    /// Non-deleted Tasks whose canonical Document points directly at the project.
    DirectLiveTasks,
}

/// Canonical task statuses used by report formulas after property validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectReportTaskStatus {
    /// Work has not started.
    NotStarted,
    /// Work is actively underway.
    InProgress,
    /// Work is awaiting or undergoing review.
    InReview,
    /// Work is complete.
    Completed,
    /// Work was canceled and is excluded from completion.
    Canceled,
}

/// Why a report metric cannot be presented as authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectReportUnavailableReason {
    /// One or more relevant current task properties was malformed or unknown.
    MalformedCurrentTaskData,
    /// No durable, complete transition history is available for historical metrics.
    MissingDurableCompletionTransitions,
    /// One or more relevant assignee values could not be validated in the active team.
    UnavailableWorkloadData,
}

/// An explicit availability boundary; unavailable inputs are never rewritten as zero.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "availability", rename_all = "snake_case")]
pub enum ProjectReportMetric<T> {
    /// The metric was computed from authoritative inputs.
    Available {
        /// Exact metric result.
        value: T,
    },
    /// The metric cannot be computed authoritatively.
    Unavailable {
        /// Stable reason why no value is present.
        reason: ProjectReportUnavailableReason,
    },
}

/// Exact completion numerator and denominator. Presentation code may derive a percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReportCompletion {
    completed_tasks: u64,
    included_tasks: u64,
}

impl ProjectReportCompletion {
    /// Direct live tasks with exact Completed status.
    pub const fn completed_tasks(self) -> u64 {
        self.completed_tasks
    }

    /// Direct live tasks excluding exact Canceled status.
    pub const fn included_tasks(self) -> u64 {
        self.included_tasks
    }

    fn checked_record(
        &mut self,
        status: Option<ProjectReportTaskStatus>,
    ) -> Result<(), ProjectReportArithmeticError> {
        if status == Some(ProjectReportTaskStatus::Canceled) {
            return Ok(());
        }
        self.included_tasks = self
            .included_tasks
            .checked_add(1)
            .ok_or(ProjectReportArithmeticError::CountOverflow)?;
        if status == Some(ProjectReportTaskStatus::Completed) {
            self.completed_tasks = self
                .completed_tasks
                .checked_add(1)
                .ok_or(ProjectReportArithmeticError::CountOverflow)?;
        }
        Ok(())
    }
}

/// A validated half-open span of local dates and its exact UTC instant bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReportWindow {
    zone: Tz,
    start_date: NaiveDate,
    end_date_exclusive: NaiveDate,
    start_inclusive: DateTime<Utc>,
    end_exclusive: DateTime<Utc>,
}

/// Why a project report window is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectReportWindowError {
    /// The end local date is not later than the start local date.
    #[error("project report window must contain at least one local date")]
    Empty,
    /// The window exceeds the bounded local-date limit.
    #[error("project report window may contain at most 400 local dates")]
    TooWide,
    /// A date calculation exceeded Chrono's supported calendar range.
    #[error("project report window date is outside the supported range")]
    DateOverflow,
    /// An IANA zone does not map a boundary midnight to one exact instant.
    #[error("project report window boundary does not map to one exact instant")]
    NonUniqueBoundary,
}

impl ProjectReportWindow {
    /// Builds a non-empty bounded local-date window and freezes its exact UTC bounds.
    pub fn new(
        zone: Tz,
        start_date: NaiveDate,
        end_date_exclusive: NaiveDate,
    ) -> Result<Self, ProjectReportWindowError> {
        let days = end_date_exclusive
            .signed_duration_since(start_date)
            .num_days();
        if days <= 0 {
            return Err(ProjectReportWindowError::Empty);
        }
        if days > MAX_PROJECT_REPORT_WINDOW_DAYS {
            return Err(ProjectReportWindowError::TooWide);
        }
        let start_inclusive = local_midnight(zone, start_date)?;
        let end_exclusive = local_midnight(zone, end_date_exclusive)?;
        Ok(Self {
            zone,
            start_date,
            end_date_exclusive,
            start_inclusive,
            end_exclusive,
        })
    }

    /// Builds the standard trailing 28-local-date window ending on `as_of_date`, inclusive.
    pub fn trailing_28_days(
        zone: Tz,
        as_of_date: NaiveDate,
    ) -> Result<Self, ProjectReportWindowError> {
        let start_date = as_of_date
            .checked_sub_days(Days::new(27))
            .ok_or(ProjectReportWindowError::DateOverflow)?;
        let end_date_exclusive = as_of_date
            .checked_add_days(Days::new(1))
            .ok_or(ProjectReportWindowError::DateOverflow)?;
        Self::new(zone, start_date, end_date_exclusive)
    }

    /// IANA time zone used to interpret both local-date boundaries.
    pub const fn zone(self) -> Tz {
        self.zone
    }

    /// Inclusive first local calendar date.
    pub const fn start_date(self) -> NaiveDate {
        self.start_date
    }

    /// Exclusive local calendar date after the final included day.
    pub const fn end_date_exclusive(self) -> NaiveDate {
        self.end_date_exclusive
    }

    /// Exact inclusive UTC instant derived from the first local midnight.
    pub const fn start_inclusive(self) -> DateTime<Utc> {
        self.start_inclusive
    }

    /// Exact exclusive UTC instant derived from the final local midnight.
    pub const fn end_exclusive(self) -> DateTime<Utc> {
        self.end_exclusive
    }

    /// Whether an instant belongs to this exact half-open window.
    pub fn contains(self, instant: DateTime<Utc>) -> bool {
        self.start_inclusive <= instant && instant < self.end_exclusive
    }
}

fn local_midnight(zone: Tz, date: NaiveDate) -> Result<DateTime<Utc>, ProjectReportWindowError> {
    let local = date
        .and_hms_opt(0, 0, 0)
        .ok_or(ProjectReportWindowError::DateOverflow)?;
    zone.from_local_datetime(&local)
        .single()
        .map(|instant| instant.with_timezone(&Utc))
        .ok_or(ProjectReportWindowError::NonUniqueBoundary)
}

/// Exact lead-time aggregate from which a UI can derive a rounded mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReportLeadTime {
    summed_seconds: u64,
    sample_count: u64,
}

impl ProjectReportLeadTime {
    /// Builds an aggregate while rejecting a non-empty sum with an empty cohort.
    pub fn new(
        summed_seconds: u64,
        sample_count: u64,
    ) -> Result<Self, ProjectReportArithmeticError> {
        if sample_count == 0 && summed_seconds != 0 {
            return Err(ProjectReportArithmeticError::InvalidLeadTime);
        }
        Ok(Self {
            summed_seconds,
            sample_count,
        })
    }

    /// Sum of exact non-negative lead-time seconds in the cohort.
    pub const fn summed_seconds(self) -> u64 {
        self.summed_seconds
    }

    /// Number of tasks contributing to the sum.
    pub const fn sample_count(self) -> u64 {
        self.sample_count
    }

    /// Returns a rounded-down mean only for a non-empty cohort.
    pub fn mean_seconds(self) -> Option<u64> {
        (self.sample_count > 0).then(|| self.summed_seconds / self.sample_count)
    }
}

/// An exact reduced non-negative fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReportRational {
    numerator: u128,
    denominator: u128,
}

impl ProjectReportRational {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };

    /// Builds and reduces a non-negative fraction with a positive denominator.
    pub fn new(numerator: u128, denominator: u128) -> Option<Self> {
        if denominator == 0 {
            return None;
        }
        let reduction = greatest_common_divisor(numerator, denominator);
        Some(Self {
            numerator: numerator / reduction,
            denominator: denominator / reduction,
        })
    }

    /// Reduced numerator.
    pub const fn numerator(self) -> u128 {
        self.numerator
    }

    /// Reduced positive denominator.
    pub const fn denominator(self) -> u128 {
        self.denominator
    }

    fn checked_add_unit_fraction(self, denominator: u128) -> Option<Self> {
        let common_divisor = greatest_common_divisor(self.denominator, denominator);
        let left_multiplier = denominator / common_divisor;
        let right_multiplier = self.denominator / common_divisor;
        let numerator = self
            .numerator
            .checked_mul(left_multiplier)?
            .checked_add(right_multiplier)?;
        let denominator = self.denominator.checked_mul(left_multiplier)?;
        Self::new(numerator, denominator)
    }
}

const fn greatest_common_divisor(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// Current exact workload allocation without copying Task records into the report view.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReportWorkload {
    allocation_by_assignee: BTreeMap<String, ProjectReportRational>,
    unassigned_wip_tasks: u64,
}

impl ProjectReportWorkload {
    /// Exact WIP allocation keyed by validated active-team assignee id.
    pub const fn allocation_by_assignee(&self) -> &BTreeMap<String, ProjectReportRational> {
        &self.allocation_by_assignee
    }

    /// WIP tasks with no assignee.
    pub const fn unassigned_wip_tasks(&self) -> u64 {
        self.unassigned_wip_tasks
    }

    fn checked_add_unassigned(&mut self) -> Result<(), ProjectReportArithmeticError> {
        self.unassigned_wip_tasks = self
            .unassigned_wip_tasks
            .checked_add(1)
            .ok_or(ProjectReportArithmeticError::CountOverflow)?;
        Ok(())
    }
}

/// Arithmetic failures are fail-closed rather than returning an inexact report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectReportArithmeticError {
    /// An exact count exceeded its bounded integer representation.
    #[error("project report count overflowed")]
    CountOverflow,
    /// A rational addition exceeded its bounded integer representation.
    #[error("exact workload allocation overflowed")]
    WorkloadOverflow,
    /// A non-empty lead-time sum had no contributing tasks.
    #[error("lead-time sum requires a non-empty cohort")]
    InvalidLeadTime,
}

/// Structural validation failures in an aggregate report metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectReportValidationError {
    /// The at-risk milestone subset exceeded the open milestone population.
    #[error("at-risk milestones cannot exceed open milestones")]
    InvalidMilestoneTotals,
}

/// Current milestone population and its union risk count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReportMilestones {
    open_milestones: u64,
    at_risk_milestones: u64,
}

impl ProjectReportMilestones {
    /// Builds milestone totals while preserving the risk-subset invariant.
    pub fn new(
        open_milestones: u64,
        at_risk_milestones: u64,
    ) -> Result<Self, ProjectReportValidationError> {
        if at_risk_milestones > open_milestones {
            return Err(ProjectReportValidationError::InvalidMilestoneTotals);
        }
        Ok(Self {
            open_milestones,
            at_risk_milestones,
        })
    }

    /// Open direct live tasks with exact milestone marker true.
    pub const fn open_milestones(self) -> u64 {
        self.open_milestones
    }

    /// Open milestones that are overdue or blocked, counted once.
    pub const fn at_risk_milestones(self) -> u64 {
        self.at_risk_milestones
    }
}

/// An unavailable-only historical metric while durable transition facts do not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ProjectReportHistoricalMetric {
    availability: ProjectReportHistoricalAvailability,
    reason: ProjectReportUnavailableReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ProjectReportHistoricalAvailability {
    Unavailable,
}

impl ProjectReportHistoricalMetric {
    fn missing_transition_history() -> Self {
        Self {
            availability: ProjectReportHistoricalAvailability::Unavailable,
            reason: ProjectReportUnavailableReason::MissingDurableCompletionTransitions,
        }
    }
}

/// Bounded aggregate contract for the project Reports mode.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskReportSnapshot {
    as_of_date: NaiveDate,
    scope: ProjectReportScope,
    completion: ProjectReportMetric<ProjectReportCompletion>,
    wip_tasks: ProjectReportMetric<u64>,
    overdue_tasks: ProjectReportMetric<u64>,
    blocked_tasks: ProjectReportMetric<u64>,
    milestones: ProjectReportMetric<ProjectReportMilestones>,
    workload: ProjectReportMetric<ProjectReportWorkload>,
    throughput: ProjectReportHistoricalMetric,
    lead_time: ProjectReportHistoricalMetric,
}

impl ProjectTaskReportSnapshot {
    /// Builds a current report and forcibly marks unsupported historical metrics unavailable.
    #[allow(clippy::too_many_arguments)]
    pub fn current(
        as_of_date: NaiveDate,
        completion: ProjectReportMetric<ProjectReportCompletion>,
        wip_tasks: ProjectReportMetric<u64>,
        overdue_tasks: ProjectReportMetric<u64>,
        blocked_tasks: ProjectReportMetric<u64>,
        milestones: ProjectReportMetric<ProjectReportMilestones>,
        workload: ProjectReportMetric<ProjectReportWorkload>,
    ) -> Self {
        Self {
            as_of_date,
            scope: ProjectReportScope::DirectLiveTasks,
            completion,
            wip_tasks,
            overdue_tasks,
            blocked_tasks,
            milestones,
            workload,
            throughput: ProjectReportHistoricalMetric::missing_transition_history(),
            lead_time: ProjectReportHistoricalMetric::missing_transition_history(),
        }
    }
}

/// Computes the exact completion numerator and denominator.
pub fn completion(
    tasks: impl IntoIterator<Item = Option<ProjectReportTaskStatus>>,
) -> Result<ProjectReportCompletion, ProjectReportArithmeticError> {
    tasks
        .into_iter()
        .try_fold(ProjectReportCompletion::default(), |mut total, status| {
            total.checked_record(status)?;
            Ok(total)
        })
}

/// Whether a current task is open for risk formulas.
pub const fn is_open(status: Option<ProjectReportTaskStatus>) -> bool {
    !matches!(
        status,
        Some(ProjectReportTaskStatus::Completed | ProjectReportTaskStatus::Canceled)
    )
}

/// Whether a current task contributes to work in progress.
pub const fn is_wip(status: Option<ProjectReportTaskStatus>) -> bool {
    matches!(
        status,
        Some(ProjectReportTaskStatus::InProgress | ProjectReportTaskStatus::InReview)
    )
}

/// Whether a task is overdue at the caller's calendar-date boundary.
pub fn is_overdue(
    status: Option<ProjectReportTaskStatus>,
    due_date: Option<NaiveDate>,
    as_of_date: NaiveDate,
) -> bool {
    is_open(status) && due_date.is_some_and(|due_date| due_date < as_of_date)
}

/// Whether a current task contributes to the blocked count.
pub const fn is_blocked(status: Option<ProjectReportTaskStatus>, blocked: bool) -> bool {
    is_open(status) && blocked
}

/// Whether a current milestone contributes to milestone risk.
pub fn is_milestone_at_risk(
    status: Option<ProjectReportTaskStatus>,
    milestone: bool,
    due_date: Option<NaiveDate>,
    blocked: bool,
    as_of_date: NaiveDate,
) -> bool {
    milestone && is_open(status) && (is_overdue(status, due_date, as_of_date) || blocked)
}

/// Selects at most one throughput transition for one task in the window.
pub fn latest_completion_in_window(
    transitions: impl IntoIterator<Item = DateTime<Utc>>,
    window: ProjectReportWindow,
) -> Option<DateTime<Utc>> {
    transitions
        .into_iter()
        .filter(|transition| window.contains(*transition))
        .max()
}

/// Returns exact whole seconds only when completion is not before creation.
pub fn lead_time_seconds(created_at: DateTime<Utc>, completion_at: DateTime<Utc>) -> Option<u64> {
    if completion_at < created_at {
        return None;
    }
    completion_at
        .signed_duration_since(created_at)
        .num_seconds()
        .try_into()
        .ok()
}

/// Aggregates current WIP workload. Assignees are deduplicated within each task.
pub fn workload<'a>(
    tasks: impl IntoIterator<Item = (Option<ProjectReportTaskStatus>, &'a [String])>,
) -> Result<ProjectReportWorkload, ProjectReportArithmeticError> {
    tasks.into_iter().try_fold(
        ProjectReportWorkload::default(),
        |mut total, (status, assignees)| {
            if !is_wip(status) {
                return Ok(total);
            }
            let distinct = assignees.iter().collect::<BTreeSet<_>>();
            if distinct.is_empty() {
                total.checked_add_unassigned()?;
            } else {
                let denominator = distinct.len() as u128;
                for assignee in distinct {
                    let previous = total
                        .allocation_by_assignee
                        .get(assignee)
                        .copied()
                        .unwrap_or(ProjectReportRational::ZERO);
                    let next = previous
                        .checked_add_unit_fraction(denominator)
                        .ok_or(ProjectReportArithmeticError::WorkloadOverflow)?;
                    total.allocation_by_assignee.insert(assignee.clone(), next);
                }
            }
            Ok(total)
        },
    )
}
