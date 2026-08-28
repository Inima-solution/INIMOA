//! Canonical task Status transition guard.
use crate::domain::model::{EntityPropertyMutationSnapshot, TaskStatusMutationOutcome};
use models_properties::service::{entity_property::EntityProperty, property_value::PropertyValue};
use models_properties::{EntityReference, EntityType};
use sqlx::{Pool, Postgres, Transaction};
use system_properties::{StatusOption, SystemPropertyKey};
use uuid::Uuid;

pub(crate) struct TaskGuardState {
    pub project_id: Option<String>,
    pub status: Option<StatusOption>,
    pub dependencies: Result<Vec<Uuid>, ()>,
}
pub(crate) fn requires_readiness(status: Option<StatusOption>) -> bool {
    matches!(
        status,
        Some(StatusOption::InProgress | StatusOption::InReview | StatusOption::Completed)
    )
}

pub(crate) async fn load_task_guard_state(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> anyhow::Result<Option<TaskGuardState>> {
    let row = sqlx::query!(r#"SELECT d."projectId" AS project_id, status_ep.values AS status_values, depends_ep.values AS depends_values FROM "Document" d JOIN document_sub_type dst ON dst.document_id=d.id AND dst.sub_type='task' LEFT JOIN entity_properties status_ep ON status_ep.entity_id=d.id AND status_ep.entity_type='TASK' AND status_ep.property_definition_id=$2 LEFT JOIN entity_properties depends_ep ON depends_ep.entity_id=d.id AND depends_ep.entity_type='TASK' AND depends_ep.property_definition_id=$3 WHERE d.id=$1 AND d."deletedAt" IS NULL FOR UPDATE OF d"#, task_id.to_string(), SystemPropertyKey::STATUS_UUID, SystemPropertyKey::DEPENDS_ON_UUID).fetch_optional(&mut **tx).await?;
    Ok(row.map(|row| TaskGuardState {
        project_id: row.project_id,
        status: row
            .status_values
            .and_then(|v| serde_json::from_value::<PropertyValue>(v).ok())
            .and_then(|v| match v {
                PropertyValue::SelectOption(ids) if ids.len() == 1 => {
                    StatusOption::from_uuid(ids[0])
                }
                _ => None,
            }),
        dependencies: parse_dependencies(row.depends_values, task_id),
    }))
}
fn parse_dependencies(value: Option<serde_json::Value>, source: Uuid) -> Result<Vec<Uuid>, ()> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let PropertyValue::EntityRef(refs) = serde_json::from_value(value).map_err(|_| ())? else {
        return Err(());
    };
    refs.into_iter()
        .map(|r| parse_reference(r, source))
        .collect()
}
fn parse_reference(r: EntityReference, source: Uuid) -> Result<Uuid, ()> {
    if r.entity_type != EntityType::Task || r.specific_message_id.is_some() {
        return Err(());
    };
    let id = Uuid::parse_str(&r.entity_id).map_err(|_| ())?;
    if id == source { Err(()) } else { Ok(id) }
}
pub(crate) async fn all_dependencies_completed(
    tx: &mut Transaction<'_, Postgres>,
    project_id: Option<&str>,
    ids: &[Uuid],
) -> anyhow::Result<bool> {
    if ids.is_empty() {
        return Ok(true);
    };
    let ids = ids.iter().map(Uuid::to_string).collect::<Vec<_>>();
    sqlx::query_scalar!(r#"SELECT COUNT(*)=$1::bigint AS "completed!" FROM UNNEST($2::text[]) candidate(id) JOIN "Document" d ON d.id=candidate.id AND d."deletedAt" IS NULL AND d."projectId" IS NOT DISTINCT FROM $3 JOIN document_sub_type dst ON dst.document_id=d.id AND dst.sub_type='task' JOIN entity_properties ep ON ep.entity_id=d.id AND ep.entity_type='TASK' AND ep.property_definition_id=$4 AND ep.values=jsonb_build_object('type','SelectOption','value',jsonb_build_array($5::text))"#, ids.len() as i64, &ids, project_id, SystemPropertyKey::STATUS_UUID, StatusOption::COMPLETED_UUID.to_string()).fetch_one(&mut **tx).await.map_err(Into::into)
}
pub async fn transition_task_status(
    pool: &Pool<Postgres>,
    task_id: Uuid,
    status: Option<StatusOption>,
) -> anyhow::Result<TaskStatusMutationOutcome> {
    let mut tx = pool.begin().await?;
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKDEPS")
    )
    .fetch_one(&mut *tx)
    .await?;
    let Some(state) = load_task_guard_state(&mut tx, task_id).await? else {
        return Ok(TaskStatusMutationOutcome::Blocked);
    };
    if requires_readiness(status)
        && (state.dependencies.is_err()
            || !all_dependencies_completed(
                &mut tx,
                state.project_id.as_deref(),
                state.dependencies.as_deref().unwrap_or_default(),
            )
            .await?)
    {
        return Ok(TaskStatusMutationOutcome::Blocked);
    };
    let value = status
        .map(|s| serde_json::to_value(PropertyValue::SelectOption(vec![s.uuid()])))
        .transpose()?;
    let row=sqlx::query!(r#"WITH previous AS (SELECT values FROM entity_properties WHERE entity_id=$2 AND entity_type='TASK' AND property_definition_id=$3) INSERT INTO entity_properties (id,entity_id,entity_type,property_definition_id,values) VALUES ($1,$2,'TASK',$3,$4) ON CONFLICT (entity_id,entity_type,property_definition_id) DO UPDATE SET values=EXCLUDED.values,updated_at=NOW() RETURNING id,entity_id,property_definition_id,values,(SELECT values FROM previous) AS previous,created_at,updated_at"#,macro_uuid::generate_uuid_v7(),task_id.to_string(),SystemPropertyKey::STATUS_UUID,value).fetch_one(&mut *tx).await?;
    let value = row
        .values
        .filter(|v| !v.is_null())
        .map(serde_json::from_value)
        .transpose()?;
    let previous_value = row
        .previous
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value(v).ok());
    let snapshot = EntityPropertyMutationSnapshot {
        property: EntityProperty {
            id: row.id,
            entity_id: row.entity_id,
            entity_type: EntityType::Task,
            property_definition_id: row.property_definition_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        },
        value,
        previous_value,
    };
    tx.commit().await?;
    Ok(TaskStatusMutationOutcome::Updated(snapshot))
}
#[cfg(test)]
mod test;
