use super::*;

#[test]
fn exists_query_serializes_exactly() {
    assert_eq!(
        ExistsQuery::new("properties.date_value").to_json(),
        serde_json::json!({"exists": {"field": "properties.date_value"}})
    );
}
