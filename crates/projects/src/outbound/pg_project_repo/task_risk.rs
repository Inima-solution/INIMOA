use chrono::NaiveDate;
use sqlx::{PgPool, Row};
use system_properties::{StatusOption, SystemPropertyKey};

use crate::domain::models::ProjectTaskRisk;

pub(super) async fn get_project_task_risk_scoped(
    pool: &PgPool,
    project_id: &str,
    team_id: uuid::Uuid,
    as_of_date: NaiveDate,
) -> Result<Option<ProjectTaskRisk>, sqlx::Error> {
    let row = sqlx::query(r#"
      WITH scoped_project AS (
        SELECT p.id, operations.status AS operations_status, operations.target_date
        FROM "Project" p
        JOIN team_user m ON m.user_id = p."userId" AND m.team_id = $2
        LEFT JOIN project_operations operations ON operations.project_id = p.id
        WHERE p.id = $1 AND p."deletedAt" IS NULL
      ), tasks AS (
        SELECT d.id, s.values status_values, due.values due_values, a.values assignee_values,
          dep.values dependency_values, milestone.values milestone_values
        FROM scoped_project p JOIN "Document" d ON d."projectId" = p.id AND d."deletedAt" IS NULL
        JOIN document_sub_type st ON st.document_id = d.id AND st.sub_type = 'task'::document_sub_type_value
        LEFT JOIN entity_properties s ON s.entity_id=d.id AND s.entity_type='TASK' AND s.property_definition_id=$3
        LEFT JOIN entity_properties due ON due.entity_id=d.id AND due.entity_type='TASK' AND due.property_definition_id=$4
        LEFT JOIN entity_properties a ON a.entity_id=d.id AND a.entity_type='TASK' AND a.property_definition_id=$5
        LEFT JOIN entity_properties dep ON dep.entity_id=d.id AND dep.entity_type='TASK' AND dep.property_definition_id=$6
        LEFT JOIN entity_properties milestone ON milestone.entity_id=d.id AND milestone.entity_type='TASK' AND milestone.property_definition_id=$13
      ), normalized_shape AS (
        SELECT *,
          CASE WHEN jsonb_typeof(status_values)='object' AND status_values->>'type'='SelectOption'
             AND jsonb_typeof(status_values->'value')='array' THEN jsonb_array_length(status_values->'value') ELSE -1 END status_length,
          CASE WHEN jsonb_typeof(assignee_values)='object' AND assignee_values->>'type'='EntityReference'
             AND jsonb_typeof(assignee_values->'value')='array' THEN true ELSE false END assignee_canonical,
          CASE WHEN jsonb_typeof(assignee_values)='object' AND assignee_values->>'type'='EntityReference'
             AND jsonb_typeof(assignee_values->'value')='array' THEN jsonb_array_length(assignee_values->'value') ELSE -1 END assignee_length,
          CASE WHEN dependency_values IS NULL OR dependency_values='null'::jsonb THEN true
             WHEN jsonb_typeof(dependency_values)='object' AND dependency_values->>'type'='EntityReference'
              AND jsonb_typeof(dependency_values->'value')='array' THEN true ELSE false END dependency_canonical,
          CASE WHEN jsonb_typeof(due_values) = 'object' AND due_values->>'type' = 'Date'
             AND jsonb_typeof(due_values->'value') = 'string'
             AND due_values->>'value' ~ '^[0-9]{4}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](\.[0-9]{1,9})?Z$'
            THEN true ELSE false END due_shape,
          CASE WHEN milestone_values IS NULL OR milestone_values='null'::jsonb THEN true
            WHEN jsonb_typeof(milestone_values)='object' AND milestone_values->>'type'='Boolean'
              AND jsonb_typeof(milestone_values->'value')='boolean' THEN true ELSE false END milestone_canonical,
          CASE WHEN jsonb_typeof(milestone_values)='object' AND milestone_values->>'type'='Boolean'
              AND milestone_values->'value'='true'::jsonb THEN true ELSE false END milestone
        FROM tasks
      ), normalized AS (
        SELECT *, CASE WHEN due_shape THEN CASE
          WHEN substring(due_values->>'value' from 1 for 4)::int = 0 THEN false
          WHEN substring(due_values->>'value' from 6 for 2)::int IN (1, 3, 5, 7, 8, 10, 12) THEN true
          WHEN substring(due_values->>'value' from 6 for 2)::int IN (4, 6, 9, 11)
            THEN substring(due_values->>'value' from 9 for 2)::int <= 30
          ELSE substring(due_values->>'value' from 9 for 2)::int <= CASE
            WHEN substring(due_values->>'value' from 1 for 4)::int % 400 = 0
              OR (substring(due_values->>'value' from 1 for 4)::int % 4 = 0
                AND substring(due_values->>'value' from 1 for 4)::int % 100 <> 0)
            THEN 29 ELSE 28 END
          END ELSE false END due_valid
        FROM normalized_shape
      ), source AS (
        SELECT *, CASE WHEN status_values IS NULL OR status_values='null'::jsonb THEN 'open'
          WHEN status_length=1 AND status_values->'value'->>0 IN ($7,$8) THEN 'excluded'
          WHEN status_length=1 AND status_values->'value'->>0 IN ($9,$10,$11) THEN 'open'
          ELSE 'unavailable' END state
        FROM normalized
      ), dependencies AS (
        SELECT source.id,
          NOT source.dependency_canonical OR COALESCE(bool_or(
            raw.reference IS NOT NULL AND (ref.invalid OR NOT COALESCE(pred.live_task,false) OR NOT COALESCE(pred.completed,false))
          ),false) blocked,
          NOT source.dependency_canonical OR COALESCE(bool_or(
            raw.reference IS NOT NULL AND (ref.invalid OR NOT COALESCE(pred.live_task,false))
          ),false) unavailable
        FROM source
        LEFT JOIN LATERAL jsonb_array_elements(CASE WHEN source.dependency_canonical THEN COALESCE(source.dependency_values->'value','[]'::jsonb) ELSE '[]'::jsonb END) raw(reference) ON true
        LEFT JOIN LATERAL (
          SELECT CASE WHEN raw.reference IS NULL THEN false WHEN jsonb_typeof(raw.reference)<>'object' THEN true
            WHEN raw.reference->>'entity_type' IS DISTINCT FROM 'TASK' OR raw.reference->>'specific_message_id' IS NOT NULL THEN true
            WHEN COALESCE(raw.reference->>'entity_id','') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN true
            WHEN raw.reference->>'entity_id'=source.id THEN true ELSE false END invalid,
          CASE WHEN jsonb_typeof(raw.reference)='object' AND raw.reference->>'entity_type' IS NOT DISTINCT FROM 'TASK'
            AND raw.reference->>'specific_message_id' IS NULL AND COALESCE(raw.reference->>'entity_id','') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
            AND raw.reference->>'entity_id'<>source.id THEN raw.reference->>'entity_id' END dependency_id
        ) ref ON raw.reference IS NOT NULL
        LEFT JOIN LATERAL (
          SELECT d.id IS NOT NULL AND st.document_id IS NOT NULL live_task,
            s.values=jsonb_build_object('type','SelectOption','value',jsonb_build_array($7::uuid)) completed
          FROM "Document" d LEFT JOIN document_sub_type st ON st.document_id=d.id AND st.sub_type='task'::document_sub_type_value
          LEFT JOIN entity_properties s ON s.entity_id=d.id AND s.entity_type='TASK' AND s.property_definition_id=$3
          WHERE d.id=ref.dependency_id AND d."projectId"=$1 AND d."deletedAt" IS NULL
        ) pred ON ref.dependency_id IS NOT NULL
        GROUP BY source.id, source.dependency_canonical
      ), classified AS (
        SELECT source.*, dep.blocked, dep.unavailable dependency_unavailable,
          CASE WHEN due_valid THEN substring(due_values->>'value' from 1 for 10) < to_char($12, 'YYYY-MM-DD') ELSE false END overdue,
          due_values IS NOT NULL AND due_values<>'null'::jsonb AND NOT due_valid due_unavailable,
          assignee_values IS NULL OR assignee_values='null'::jsonb OR assignee_length=0 unassigned,
          NOT (assignee_values IS NULL OR assignee_values='null'::jsonb OR assignee_length=0
            OR (assignee_canonical AND assignee_length>0 AND NOT EXISTS (
              SELECT 1 FROM jsonb_array_elements(assignee_values->'value') ref
              WHERE jsonb_typeof(ref)<>'object' OR ref->>'entity_type' IS DISTINCT FROM 'USER' OR ref->>'specific_message_id' IS NOT NULL OR COALESCE(ref->>'entity_id','')=''
            ))) assignee_unavailable
        FROM source JOIN dependencies dep ON dep.id=source.id
      )
      SELECT risk.overdue_tasks, risk.blocked_tasks, risk.unassigned_tasks,
        risk.open_milestones, risk.at_risk_milestones,
        COALESCE(project.operations_status IN ('planned', 'active')
          AND project.target_date BETWEEN $12 AND ($12 + 7), false) AS approaching_target,
        risk.has_unavailable_risk_data
          OR project.operations_status IS NULL
          OR project.target_date IS NULL AS has_unavailable_risk_data
      FROM scoped_project project
      CROSS JOIN LATERAL (
        SELECT COUNT(*) FILTER (WHERE state='open' AND overdue) overdue_tasks,
          COUNT(*) FILTER (WHERE state='open' AND blocked) blocked_tasks,
          COUNT(*) FILTER (WHERE state='open' AND unassigned) unassigned_tasks,
          COUNT(*) FILTER (WHERE state='open' AND milestone) open_milestones,
          COUNT(*) FILTER (WHERE state='open' AND milestone AND (overdue OR blocked)) at_risk_milestones,
          COALESCE(bool_or(state='unavailable' OR (state='open' AND (due_unavailable OR assignee_unavailable OR dependency_unavailable OR NOT milestone_canonical))),false) has_unavailable_risk_data
        FROM classified
      ) risk
      "#)
    .bind(project_id).bind(team_id).bind(SystemPropertyKey::STATUS_UUID).bind(SystemPropertyKey::DUE_DATE_UUID)
    .bind(SystemPropertyKey::ASSIGNEES_UUID).bind(SystemPropertyKey::DEPENDS_ON_UUID)
    .bind(StatusOption::COMPLETED_UUID.to_string()).bind(StatusOption::CANCELED_UUID.to_string())
    .bind(StatusOption::NOT_STARTED_UUID.to_string()).bind(StatusOption::IN_PROGRESS_UUID.to_string())
    .bind(StatusOption::IN_REVIEW_UUID.to_string()).bind(as_of_date)
    .bind(SystemPropertyKey::MILESTONE_UUID).fetch_optional(pool).await?;
    row.map(|r| {
        ProjectTaskRisk::new(
            r.try_get("overdue_tasks")?,
            r.try_get("blocked_tasks")?,
            r.try_get("unassigned_tasks")?,
            r.try_get("open_milestones")?,
            r.try_get("at_risk_milestones")?,
            r.try_get("approaching_target")?,
            r.try_get("has_unavailable_risk_data")?,
        )
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))
    })
    .transpose()
}
