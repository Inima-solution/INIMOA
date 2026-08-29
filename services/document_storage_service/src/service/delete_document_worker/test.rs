use entity_access::domain::models::{EntityAccessAuth, EntityType};

use super::handle::{
    MessageRoute, PurgeRoute, classify_message, count_occurrences, document_cleanup_receipt,
    parse_purge_token, route_purge_outcome,
};

#[test]
fn document_cleanup_receipt_is_internal_for_the_document() {
    let document_id = "410ee0f3-80df-4ae7-b9a6-5c87fe5408af";

    let receipt = document_cleanup_receipt(document_id);

    assert_eq!(receipt.entity().entity_id, document_id);
    assert_eq!(receipt.entity().entity_type, EntityType::Document);
    assert!(matches!(receipt.auth(), EntityAccessAuth::Internal));
}

#[test]
fn test_count_occurrences() {
    let shas = vec![
        "a1b2c3".to_string(),
        "d4e5f6".to_string(),
        "a1b2c3".to_string(),
        "g7h8i9".to_string(),
        "a1b2c3".to_string(),
        "d4e5f6".to_string(),
    ];

    let mut result = count_occurrences(shas);
    result.sort();
    assert_eq!(
        result,
        vec![
            ("a1b2c3".to_string(), 3),
            ("d4e5f6".to_string(), 2),
            ("g7h8i9".to_string(), 1),
        ]
    );
}

#[test]
fn token_route_acknowledges_malformed_or_stale_without_metadata() {
    assert!(parse_purge_token("not-a-timestamp").is_none());
    assert!(matches!(
        route_purge_outcome(macro_db_client::document::DocumentPurgeOutcome::StaleOrUnavailable),
        PurgeRoute::AckOnly
    ));
}

#[test]
fn token_route_exposes_metadata_only_after_purge() {
    let metadata = macro_db_client::document::DocumentPurgeMetadata {
        document_id: "id".into(),
        owner: "owner".into(),
        project_id: None,
        file_type: None,
        bom_shas: vec![],
    };
    match route_purge_outcome(macro_db_client::document::DocumentPurgeOutcome::Purged(
        metadata.clone(),
    )) {
        PurgeRoute::PostCommitCleanup(actual) => assert_eq!(actual, metadata),
        PurgeRoute::AckOnly => panic!("purged metadata must reach cleanup"),
    }
}

#[test]
fn no_token_remains_legacy_retention() {
    assert!(matches!(
        classify_message(false, None),
        MessageRoute::LegacyRetention
    ));
}

#[test]
fn owner_message_stays_owner_cleanup_and_valid_token_is_exact() {
    assert!(matches!(
        classify_message(true, Some("2026-08-29T00:00:00+00:00")),
        MessageRoute::OwnerCleanup
    ));
    assert_eq!(
        parse_purge_token("2026-08-29T00:00:00+00:00")
            .unwrap()
            .to_rfc3339(),
        "2026-08-29T00:00:00+00:00"
    );
}
