use super::*;

fn value<'a>(
    attrs: &'a std::collections::HashMap<String, aws_sdk_sqs::types::MessageAttributeValue>,
    key: &str,
) -> &'a str {
    attrs[key].string_value().unwrap()
}

#[test]
fn legacy_ownerless_attributes_are_byte_compatible() {
    let attrs = construct_message_attributes(None, "document-id", None).unwrap();
    assert_eq!(attrs.len(), 1);
    assert_eq!(value(&attrs, "document_id"), "document-id");
    assert!(!attrs.contains_key("user_id"));
    assert!(!attrs.contains_key("deleted_at"));
}

#[test]
fn owner_attributes_have_no_purge_token() {
    let attrs =
        construct_message_attributes(Some("macro|owner@example.com"), "document-id", None).unwrap();
    assert_eq!(attrs.len(), 2);
    assert_eq!(value(&attrs, "document_id"), "document-id");
    assert_eq!(value(&attrs, "user_id"), "macro|owner@example.com");
    assert!(!attrs.contains_key("deleted_at"));
}

#[test]
fn purge_candidate_attributes_have_exact_token_and_no_owner() {
    let attrs =
        construct_message_attributes(None, "document-id", Some("2026-08-29T00:00:00+00:00"))
            .unwrap();
    assert_eq!(attrs.len(), 2);
    assert_eq!(value(&attrs, "document_id"), "document-id");
    assert_eq!(value(&attrs, "deleted_at"), "2026-08-29T00:00:00+00:00");
    assert!(!attrs.contains_key("user_id"));
}
