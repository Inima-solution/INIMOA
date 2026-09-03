use sqlx::{Pool, Postgres};
use uuid::Uuid;

const MIGRATION: &str =
    include_str!("../migrations/20260903040000_decision_document_foundation.sql");

const DEFINITIONS: [(&str, &str, &str, bool, Option<&str>); 4] = [
    (
        "00000001-0000-0000-0000-000000000015",
        "Decision State",
        "SELECT_STRING",
        false,
        None,
    ),
    (
        "00000001-0000-0000-0000-000000000016",
        "Decided By",
        "ENTITY",
        false,
        Some("USER"),
    ),
    (
        "00000001-0000-0000-0000-000000000017",
        "Decided At",
        "DATE",
        false,
        None,
    ),
    (
        "00000001-0000-0000-0000-000000000018",
        "Source Links",
        "LINK",
        true,
        None,
    ),
];

#[sqlx::test]
async fn decision_foundation_migration_has_stable_subtype_definitions_and_options(
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    let definition_ids = DEFINITIONS
        .iter()
        .map(|(id, ..)| id.parse::<Uuid>())
        .collect::<Result<Vec<_>, _>>()?;

    sqlx::query("DELETE FROM property_definitions WHERE id = ANY($1)")
        .bind(&definition_ids)
        .execute(&pool)
        .await?;
    sqlx::raw_sql(MIGRATION).execute(&pool).await?;

    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT 'decision'::document_sub_type_value::text",)
            .fetch_one(&pool)
            .await?,
        "decision"
    );

    for (id, display_name, data_type, is_multi_select, specific_entity_type) in DEFINITIONS {
        let id = id.parse::<Uuid>()?;
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                bool,
                Option<String>,
                bool,
                Option<Uuid>,
                Option<String>,
            ),
        >(
            "SELECT display_name, data_type::text, is_multi_select, specific_entity_type::text, is_system, team_id, user_id FROM property_definitions WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            row,
            (
                display_name.to_owned(),
                data_type.to_owned(),
                is_multi_select,
                specific_entity_type.map(str::to_owned),
                true,
                None,
                None,
            )
        );
    }

    assert_eq!(
        sqlx::query_as::<_, (Uuid, i32, String)>(
            "SELECT id, display_order, string_value FROM property_options WHERE property_definition_id = $1 ORDER BY display_order",
        )
        .bind(definition_ids[0])
        .fetch_all(&pool)
        .await?,
        vec![
            (
                "00000001-0000-0000-0015-000000000001".parse()?,
                0,
                "Proposed".to_owned(),
            ),
            (
                "00000001-0000-0000-0015-000000000002".parse()?,
                1,
                "Accepted".to_owned(),
            ),
            (
                "00000001-0000-0000-0015-000000000003".parse()?,
                2,
                "Rejected".to_owned(),
            ),
            (
                "00000001-0000-0000-0015-000000000004".parse()?,
                3,
                "Superseded".to_owned(),
            ),
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM property_options WHERE property_definition_id = ANY($1) AND property_definition_id <> $2",
        )
        .bind(&definition_ids)
        .bind(definition_ids[0])
        .fetch_one(&pool)
        .await?,
        0
    );

    Ok(())
}
