use super::*;
use chrono::NaiveDateTime;

#[sqlx::test(fixtures(path = "../../../fixtures", scripts("recently_deleted")))]
async fn returns_old_deleted_projects_with_exact_tokens(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let cutoff = NaiveDateTime::parse_from_str("2020-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")?;
    let mut projects = get_projects_to_delete(&pool, &cutoff).await?;
    projects.sort_unstable_by(|left, right| left.project_id.cmp(&right.project_id));

    assert_eq!(
        projects,
        vec![ProjectToDelete {
            project_id: "p1".to_owned(),
            deleted_at: chrono::DateTime::parse_from_rfc3339("2019-10-16T00:00:00+00:00")?
                .with_timezone(&chrono::Utc),
        },]
    );

    Ok(())
}
