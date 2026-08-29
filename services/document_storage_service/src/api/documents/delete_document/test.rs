use super::*;

#[test]
fn stale_outcome_maps_before_rest_effects_and_purged_keeps_captured_data() {
    assert!(
        require_purged(macro_db_client::document::DocumentPurgeOutcome::StaleOrUnavailable)
            .is_none()
    );
    let metadata = macro_db_client::document::DocumentPurgeMetadata {
        document_id: "id".into(),
        owner: "owner".into(),
        project_id: None,
        file_type: Some("docx".into()),
        bom_shas: vec!["sha".into()],
    };
    let actual = require_purged(macro_db_client::document::DocumentPurgeOutcome::Purged(
        metadata.clone(),
    ))
    .unwrap();
    assert_eq!(actual, metadata);
}
