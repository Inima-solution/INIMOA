//! Caller-scoped projection of canonical direct task dependency relations.

use entity_access::domain::models::EntityType as AccessEntityType;
use uuid::Uuid;

use crate::domain::{
    error::PropertiesErr,
    model::{
        PropertyAccessReceiptExt, TaskDependencyRelations, TaskDependencyRelationsSnapshot,
        TaskReadiness, ViewReceipt,
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
    pub(crate) const TASK_DEPENDENCY_RELATIONS_BATCH_MAX: usize = 200;

    pub(crate) async fn get_task_dependency_relations_scoped(
        &self,
        sources: &[ViewReceipt],
        task_ids: &[Uuid],
    ) -> Result<Vec<TaskDependencyRelations>, PropertiesErr> {
        if task_ids.len() > Self::TASK_DEPENDENCY_RELATIONS_BATCH_MAX
            || sources.len() != task_ids.len()
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
            .get_task_dependency_relations(task_ids)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or(PropertiesErr::NotFound)?;
        if snapshots.len() != task_ids.len()
            || snapshots
                .iter()
                .zip(task_ids)
                .any(|(snapshot, task_id)| snapshot.readiness.task_id != *task_id)
        {
            return Err(PropertiesErr::NotFound);
        }
        let permission_service = self.permission_service()?;
        let mut result = Vec::with_capacity(snapshots.len());
        for snapshot in snapshots {
            result.push(project_snapshot(permission_service, &actor, snapshot).await);
        }
        Ok(result)
    }
}

async fn project_snapshot<P: PermissionService>(
    permission_service: &P,
    actor: &macro_user_id::user_id::MacroUserIdStr<'_>,
    snapshot: TaskDependencyRelationsSnapshot,
) -> TaskDependencyRelations {
    let mut readiness = snapshot.readiness;
    let forward_ids = readiness.depends_on_task_ids.clone();
    if readiness.has_unavailable_dependencies
        || !all_visible(permission_service, actor, &forward_ids).await
    {
        readiness.readiness = TaskReadiness::Blocked;
        readiness.depends_on_task_ids.clear();
        readiness.blocking_task_ids.clear();
        readiness.has_unavailable_dependencies = true;
    }
    let successor_unavailable = snapshot.has_unavailable_successors
        || !all_visible(permission_service, actor, &snapshot.successor_task_ids).await;
    let successor_ids = if successor_unavailable {
        Vec::new()
    } else {
        snapshot.successor_task_ids
    };
    let has_unavailable_successors = successor_unavailable;
    TaskDependencyRelations {
        task_id: readiness.task_id,
        readiness: readiness.readiness,
        depends_on_task_ids: readiness.depends_on_task_ids,
        blocking_task_ids: readiness.blocking_task_ids,
        has_unavailable_dependencies: readiness.has_unavailable_dependencies,
        successor_task_ids: successor_ids,
        has_unavailable_successors,
    }
}

async fn all_visible<P: PermissionService>(
    permission_service: &P,
    actor: &macro_user_id::user_id::MacroUserIdStr<'_>,
    ids: &[Uuid],
) -> bool {
    for id in ids {
        if permission_service
            .mint_view_receipt(Some(actor), &id.to_string(), AccessEntityType::Document)
            .await
            .is_err()
        {
            return false;
        }
    }
    true
}
