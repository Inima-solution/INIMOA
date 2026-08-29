//! Atomic canonical task hierarchy replacements.

use std::collections::HashSet;

use models_properties::service::{entity_property::EntityProperty, property_value::PropertyValue};
use models_properties::{EntityReference, EntityType};
use sqlx::{PgConnection, Pool, Postgres};
use system_properties::SystemPropertyKey;
use uuid::Uuid;

use crate::domain::model::TaskHierarchyMutationOutcome;

pub async fn link_parent_task(
    pool: &Pool<Postgres>,
    task_id: Uuid,
    parent_task_id: Option<Uuid>,
) -> anyhow::Result<TaskHierarchyMutationOutcome> {
    if parent_task_id == Some(task_id) {
        return Ok(TaskHierarchyMutationOutcome::Cycle);
    }
    let mut tx = pool.begin().await?;
    lock_hierarchy(&mut tx).await?;
    if !lock_live_same_project_tasks(
        &mut tx,
        task_id,
        &parent_task_id.into_iter().collect::<Vec<_>>(),
    )
    .await?
    {
        return Ok(TaskHierarchyMutationOutcome::Unavailable);
    }
    if let Some(parent_id) = parent_task_id
        && would_create_cycle(&mut tx, task_id, parent_id).await?
    {
        return Ok(TaskHierarchyMutationOutcome::Cycle);
    }
    let old_parent = read_parent(&mut tx, task_id).await?;
    if let Some(old_parent_id) = old_parent
        && Some(old_parent_id) != parent_task_id
    {
        remove_from_subtasks(&mut tx, old_parent_id, task_id).await?;
    }
    let Some(property) = write_parent(&mut tx, task_id, parent_task_id).await? else {
        return Ok(TaskHierarchyMutationOutcome::Unavailable);
    };
    if let Some(parent_id) = parent_task_id {
        append_to_subtasks(&mut tx, parent_id, task_id).await?;
    }
    tx.commit().await?;
    Ok(TaskHierarchyMutationOutcome::Updated(property))
}

pub async fn link_subtasks(
    pool: &Pool<Postgres>,
    task_id: Uuid,
    subtask_ids: Vec<Uuid>,
) -> anyhow::Result<TaskHierarchyMutationOutcome> {
    if subtask_ids.contains(&task_id) {
        return Ok(TaskHierarchyMutationOutcome::Cycle);
    }
    if subtask_ids.iter().collect::<HashSet<_>>().len() != subtask_ids.len() {
        anyhow::bail!("duplicate task hierarchy references");
    }
    let mut tx = pool.begin().await?;
    lock_hierarchy(&mut tx).await?;
    if !lock_live_same_project_tasks(&mut tx, task_id, &subtask_ids).await? {
        return Ok(TaskHierarchyMutationOutcome::Unavailable);
    }
    for child_id in &subtask_ids {
        if would_create_cycle(&mut tx, *child_id, task_id).await? {
            return Ok(TaskHierarchyMutationOutcome::Cycle);
        }
    }
    let current = read_subtasks(&mut tx, task_id).await?;
    let requested = subtask_ids.iter().copied().collect::<HashSet<_>>();
    let removed = current
        .iter()
        .copied()
        .filter(|id| !requested.contains(id))
        .collect::<Vec<_>>();
    let Some(property) = write_subtasks(&mut tx, task_id, &subtask_ids).await? else {
        return Ok(TaskHierarchyMutationOutcome::Unavailable);
    };
    for child_id in &subtask_ids {
        if let Some(old_parent_id) = read_parent(&mut tx, *child_id).await?
            && old_parent_id != task_id
        {
            remove_from_subtasks(&mut tx, old_parent_id, *child_id).await?;
        }
        if write_parent(&mut tx, *child_id, Some(task_id))
            .await?
            .is_none()
        {
            return Ok(TaskHierarchyMutationOutcome::Unavailable);
        }
    }
    for child_id in removed {
        if read_parent(&mut tx, child_id).await? == Some(task_id)
            && write_parent(&mut tx, child_id, None).await?.is_none()
        {
            return Ok(TaskHierarchyMutationOutcome::Unavailable);
        }
    }
    tx.commit().await?;
    Ok(TaskHierarchyMutationOutcome::Updated(property))
}

async fn lock_hierarchy(tx: &mut PgConnection) -> anyhow::Result<()> {
    // ponytail: this global lock is only for hierarchy replacements; shard per project only after measured throughput requires it.
    sqlx::query_scalar!(
        r#"SELECT 1 AS "locked!" FROM pg_advisory_xact_lock($1)"#,
        i64::from_be_bytes(*b"TASKHIER")
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(())
}

async fn lock_live_same_project_tasks(
    tx: &mut PgConnection,
    source_id: Uuid,
    candidates: &[Uuid],
) -> anyhow::Result<bool> {
    let mut ids = candidates.to_vec();
    ids.push(source_id);
    ids.sort_unstable();
    ids.dedup();
    let ids = ids.iter().map(Uuid::to_string).collect::<Vec<_>>();
    let rows = sqlx::query!(
        r#"
        SELECT d.id
        FROM "Document" d
        JOIN document_sub_type dst ON dst.document_id = d.id AND dst.sub_type = 'task'
        WHERE d.id = ANY($1)
          AND d."deletedAt" IS NULL
          AND d."projectId" IS NOT DISTINCT FROM (
              SELECT "projectId" FROM "Document" WHERE id = $2
          )
        ORDER BY d.id
        FOR UPDATE
        "#,
        &ids,
        source_id.to_string(),
    )
    .fetch_all(&mut *tx)
    .await?;
    Ok(rows.len() == ids.len())
}

/// The single canonical parent-chain check used by both write directions.
async fn would_create_cycle(
    tx: &mut PgConnection,
    child_id: Uuid,
    proposed_parent_id: Uuid,
) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar!(
        r#"
        WITH RECURSIVE parent_chain(id, path) AS (
            SELECT $1::TEXT, ARRAY[$1::TEXT]
            UNION ALL
            SELECT edge->>'entity_id', parent_chain.path || (edge->>'entity_id')
            FROM parent_chain
            JOIN entity_properties ep
              ON ep.entity_id = parent_chain.id
             AND ep.entity_type = 'TASK'
             AND ep.property_definition_id = $2
            CROSS JOIN LATERAL jsonb_array_elements(
                CASE WHEN jsonb_typeof(ep.values->'value') = 'array'
                    THEN ep.values->'value' ELSE '[]'::jsonb END
            ) edge
            WHERE edge->>'entity_type' = 'TASK'
              AND edge->>'entity_id' IS NOT NULL
              AND NOT edge->>'entity_id' = ANY(parent_chain.path)
        )
        SELECT EXISTS(
            SELECT 1 FROM parent_chain WHERE id = $3
        ) AS "has_cycle!"
        "#,
        proposed_parent_id.to_string(),
        SystemPropertyKey::PARENT_TASK_UUID,
        child_id.to_string(),
    )
    .fetch_one(&mut *tx)
    .await?)
}

async fn read_parent(tx: &mut PgConnection, task_id: Uuid) -> anyhow::Result<Option<Uuid>> {
    let parent: Option<Option<String>> = sqlx::query_scalar!(
        r#"
        SELECT values->'value'->0->>'entity_id' as "parent_id"
        FROM entity_properties
        WHERE entity_id = $1
          AND entity_type = 'TASK'
          AND property_definition_id = $2
          AND values IS NOT NULL
        "#,
        task_id.to_string(),
        SystemPropertyKey::PARENT_TASK_UUID
    )
    .fetch_optional(&mut *tx)
    .await?;
    parent
        .flatten()
        .map(|id| Uuid::parse_str(&id))
        .transpose()
        .map_err(Into::into)
}

async fn read_subtasks(tx: &mut PgConnection, task_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
    let ids: Vec<String> = sqlx::query_scalar!(
        r#"
        SELECT elem->>'entity_id' as "subtask_id!"
        FROM entity_properties,
             jsonb_array_elements(values->'value') elem
        WHERE entity_id = $1
          AND entity_type = 'TASK'
          AND property_definition_id = $2
          AND values IS NOT NULL
        "#,
        task_id.to_string(),
        SystemPropertyKey::SUBTASKS_UUID
    )
    .fetch_all(&mut *tx)
    .await?;
    ids.into_iter()
        .map(|id| Uuid::parse_str(&id).map_err(Into::into))
        .collect()
}

async fn remove_from_subtasks(
    tx: &mut PgConnection,
    parent_id: Uuid,
    child_id: Uuid,
) -> anyhow::Result<()> {
    let updated = read_subtasks(tx, parent_id)
        .await?
        .into_iter()
        .filter(|id| *id != child_id)
        .collect::<Vec<_>>();
    if write_subtasks(tx, parent_id, &updated).await?.is_none() {
        anyhow::bail!("required task hierarchy property missing");
    }
    Ok(())
}

async fn append_to_subtasks(
    tx: &mut PgConnection,
    parent_id: Uuid,
    child_id: Uuid,
) -> anyhow::Result<()> {
    let mut current = read_subtasks(tx, parent_id).await?;
    if !current.contains(&child_id) {
        current.push(child_id);
    }
    if write_subtasks(tx, parent_id, &current).await?.is_none() {
        anyhow::bail!("required task hierarchy property missing");
    }
    Ok(())
}

async fn write_parent(
    tx: &mut PgConnection,
    task_id: Uuid,
    parent_id: Option<Uuid>,
) -> anyhow::Result<Option<EntityProperty>> {
    write_property(
        tx,
        task_id,
        SystemPropertyKey::PARENT_TASK_UUID,
        parent_id.map(|id| {
            PropertyValue::EntityRef(vec![EntityReference::new(id.to_string(), EntityType::Task)])
        }),
    )
    .await
}

async fn write_subtasks(
    tx: &mut PgConnection,
    task_id: Uuid,
    ids: &[Uuid],
) -> anyhow::Result<Option<EntityProperty>> {
    write_property(
        tx,
        task_id,
        SystemPropertyKey::SUBTASKS_UUID,
        (!ids.is_empty()).then(|| {
            PropertyValue::EntityRef(
                ids.iter()
                    .map(|id| EntityReference::new(id.to_string(), EntityType::Task))
                    .collect(),
            )
        }),
    )
    .await
}

async fn write_property(
    tx: &mut PgConnection,
    task_id: Uuid,
    property_id: Uuid,
    value: Option<PropertyValue>,
) -> anyhow::Result<Option<EntityProperty>> {
    let value = value.map(serde_json::to_value).transpose()?;
    Ok(sqlx::query_as!(
        EntityProperty,
        r#"
        UPDATE entity_properties
        SET values = $4, updated_at = NOW()
        WHERE entity_id = $1
          AND entity_type = $2
          AND property_definition_id = $3
        RETURNING
            id,
            entity_id,
            entity_type as "entity_type: EntityType",
            property_definition_id,
            created_at,
            updated_at
        "#,
        task_id.to_string(),
        EntityType::Task as EntityType,
        property_id,
        value,
    )
    .fetch_optional(&mut *tx)
    .await?)
}
