//! Depends On validation at the shared task property write boundary.

use std::collections::HashSet;

use entity_access::domain::models::{EntityAccessAuth, EntityType as AccessEntityType};
use models_properties::EntityType;
use models_properties::api::requests::SetPropertyValue;
use uuid::Uuid;

use crate::domain::error::PropertiesErr;
use crate::domain::model::{EditReceipt, PropertyAccessReceiptExt, TaskDependencyMutationOutcome};
use crate::domain::ports::{NotificationService, PermissionService, PropertiesRepo};
use crate::domain::service_impl::PropertiesServiceImpl;
use macro_event_broker::MacroEventBroker;

impl<R, P, N, B> PropertiesServiceImpl<R, P, N, B>
where
    R: PropertiesRepo,
    P: PermissionService,
    N: NotificationService,
    B: MacroEventBroker,
    anyhow::Error: From<R::Err> + From<P::Err> + From<N::Err>,
{
    pub(crate) async fn handle_task_dependencies_property(
        &self,
        access: &EditReceipt,
        value: Option<SetPropertyValue>,
    ) -> Result<crate::domain::model::EntityPropertyMutationSnapshot, PropertiesErr> {
        let task_id = Uuid::parse_str(access.entity_id()).map_err(|_| malformed_dependencies())?;
        let dependency_ids = parse_dependencies(task_id, value)?;

        match access.auth() {
            EntityAccessAuth::Internal => {}
            EntityAccessAuth::Authenticated(user_id) => {
                let permission_service = self.permission_service()?;
                for dependency_id in &dependency_ids {
                    permission_service
                        .mint_view_receipt(
                            Some(user_id),
                            &dependency_id.to_string(),
                            AccessEntityType::Document,
                        )
                        .await
                        .map_err(|_| PropertiesErr::TaskDependenciesUnavailable)?;
                }
            }
            EntityAccessAuth::Bot(_) | EntityAccessAuth::Unauthenticated => {
                return Err(PropertiesErr::PermissionDenied);
            }
        }

        match self
            .repository
            .replace_task_dependencies(task_id, &dependency_ids)
            .await
            .map_err(anyhow::Error::from)?
        {
            TaskDependencyMutationOutcome::Updated(snapshot) => Ok(snapshot),
            TaskDependencyMutationOutcome::Unavailable => {
                Err(PropertiesErr::TaskDependenciesUnavailable)
            }
            TaskDependencyMutationOutcome::Cycle => Err(PropertiesErr::TaskDependencyCycle),
        }
    }
}

fn malformed_dependencies() -> PropertiesErr {
    PropertiesErr::Validation("Depends On requires task references".to_string())
}

fn parse_dependencies(
    task_id: Uuid,
    value: Option<SetPropertyValue>,
) -> Result<Vec<Uuid>, PropertiesErr> {
    let references = match value {
        None => return Ok(Vec::new()),
        Some(SetPropertyValue::MultiEntityReference { references }) => references,
        Some(_) => return Err(malformed_dependencies()),
    };

    let mut dependency_ids = Vec::with_capacity(references.len());
    let mut seen = HashSet::with_capacity(references.len());
    for reference in references {
        if reference.entity_type != EntityType::Task || reference.specific_message_id.is_some() {
            return Err(malformed_dependencies());
        }
        let dependency_id =
            Uuid::parse_str(&reference.entity_id).map_err(|_| malformed_dependencies())?;
        if dependency_id == task_id {
            return Err(PropertiesErr::Validation(
                "A task cannot depend on itself".to_string(),
            ));
        }
        if !seen.insert(dependency_id) {
            return Err(PropertiesErr::Validation(
                "A task dependency may only be listed once".to_string(),
            ));
        }
        dependency_ids.push(dependency_id);
    }
    Ok(dependency_ids)
}

#[cfg(test)]
mod test;
