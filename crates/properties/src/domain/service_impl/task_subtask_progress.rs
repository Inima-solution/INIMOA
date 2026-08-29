//! Caller-scoped projection of canonical direct-subtask progress.

use std::collections::HashSet;

use entity_access::domain::models::EntityType as AccessEntityType;
use uuid::Uuid;

use crate::domain::{
    error::PropertiesErr,
    model::{
        PropertyAccessReceiptExt, TaskSubtaskProgress, TaskSubtaskProgressSnapshot, ViewReceipt,
    },
    ports::{NotificationService, PermissionService, PropertiesRepo},
    service_impl::PropertiesServiceImpl,
};
use macro_event_broker::MacroEventBroker;

impl<R, P, N, B> PropertiesServiceImpl<R, P, N, B>
where
    R: PropertiesRepo,
    P: PermissionService,
    N: NotificationService,
    B: MacroEventBroker,
    anyhow::Error: From<R::Err> + From<P::Err> + From<N::Err>,
{
    pub(crate) const TASK_SUBTASK_PROGRESS_BATCH_MAX: usize = 200;

    pub(crate) async fn get_task_subtask_progress_scoped(
        &self,
        sources: &[ViewReceipt],
        task_ids: &[Uuid],
    ) -> Result<Vec<TaskSubtaskProgress>, PropertiesErr> {
        if task_ids.len() > Self::TASK_SUBTASK_PROGRESS_BATCH_MAX || sources.len() != task_ids.len()
        {
            return Err(PropertiesErr::Validation(
                "At most 200 task IDs may be requested".to_owned(),
            ));
        }
        let Some(first) = sources.first() else {
            return Ok(Vec::new());
        };
        let actor = first
            .get_authenticated_user()
            .map_err(|_| PropertiesErr::PermissionDenied)?;
        for (source, task_id) in sources.iter().zip(task_ids) {
            if source.entity().entity_type != AccessEntityType::Document
                || source.entity_id() != task_id.to_string()
                || source
                    .get_authenticated_user()
                    .map_err(|_| PropertiesErr::PermissionDenied)?
                    != actor
            {
                return Err(PropertiesErr::PermissionDenied);
            }
        }
        let snapshots = self
            .repository
            .get_task_subtask_progress(task_ids)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or(PropertiesErr::NotFound)?;
        if snapshots.len() != task_ids.len()
            || snapshots
                .iter()
                .zip(task_ids)
                .any(|(snapshot, task_id)| snapshot.task_id != *task_id)
        {
            return Err(PropertiesErr::NotFound);
        }
        let permission_service = self.permission_service()?;
        let mut progress = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            progress.push(project_snapshot(permission_service, &actor, snapshot).await);
        }
        Ok(progress)
    }
}

async fn project_snapshot<P: PermissionService>(
    permission_service: &P,
    actor: &macro_user_id::user_id::MacroUserIdStr<'_>,
    snapshot: TaskSubtaskProgressSnapshot,
) -> TaskSubtaskProgress {
    let unavailable = || TaskSubtaskProgress {
        task_id: snapshot.task_id,
        completed_subtasks: 0,
        total_subtasks: 0,
        has_unavailable_subtasks: true,
    };
    if snapshot.has_unavailable_subtasks {
        return unavailable();
    }
    for child_id in &snapshot.subtask_ids {
        if permission_service
            .mint_view_receipt(
                Some(actor),
                &child_id.to_string(),
                AccessEntityType::Document,
            )
            .await
            .is_err()
        {
            return unavailable();
        }
    }
    let canceled = HashSet::<Uuid>::from_iter(snapshot.canceled_subtask_ids);
    let completed = HashSet::<Uuid>::from_iter(snapshot.completed_subtask_ids);
    let live = snapshot
        .subtask_ids
        .iter()
        .filter(|id| !canceled.contains(id))
        .count();
    let complete = snapshot
        .subtask_ids
        .iter()
        .filter(|id| !canceled.contains(id) && completed.contains(id))
        .count();
    TaskSubtaskProgress {
        task_id: snapshot.task_id,
        completed_subtasks: complete as u32,
        total_subtasks: live as u32,
        has_unavailable_subtasks: false,
    }
}
