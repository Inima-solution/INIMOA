//! Helper functions for property service implementation.

use document_sub_type::DocumentSubType;
use entity_access::domain::models::EntityAccessAuth;
use macro_user_id::user_id::MacroUserIdStr;
use models_properties::api::requests::SetPropertyValue;
use models_properties::service::entity_property_with_definition::EntityPropertyWithDefinition;
use models_properties::service::property_value::PropertyValue;
use models_properties::{DataType, EntityType, PropertyOwner};
use system_properties::{DecisionStateOption, SystemPropertyKey};
use uuid::Uuid;

/// Whether a tag-typed property with the given owner is visible to the caller.
/// A user-owned tag set (personal labels) is visible only to its owner, so
/// personal tags stay private even on a shared entity. Team- and system-owned
/// tags are the shared vocabulary and are always visible. Internal (machine)
/// callers see everything.
fn tag_visible_to(owner: &PropertyOwner, auth: &EntityAccessAuth) -> bool {
    match owner {
        PropertyOwner::User { user_id } => match auth {
            EntityAccessAuth::Authenticated(caller) => user_id == caller.as_ref(),
            EntityAccessAuth::Bot(_) => false,
            EntityAccessAuth::Unauthenticated => false,
            EntityAccessAuth::Internal => true,
        },
        PropertyOwner::Team { .. } | PropertyOwner::System => true,
    }
}

/// Drops tag-typed properties the caller may not see. Non-tag properties are
/// unaffected.
pub fn retain_caller_visible_tags(
    properties: &mut Vec<EntityPropertyWithDefinition>,
    auth: &EntityAccessAuth,
) {
    properties.retain(|property| {
        property.definition.data_type != DataType::Tag
            || tag_visible_to(&property.definition.owner, auth)
    });
}

/// Extract option IDs from a PropertyValue.
pub fn extract_option_ids_from_property_value(value: &Option<PropertyValue>) -> Vec<Uuid> {
    match value {
        Some(PropertyValue::SelectOption(ids)) => ids.clone(),
        _ => Vec::new(),
    }
}

/// Check if a property can be attached to the given entity type.
pub fn is_property_applicable_to(property_id: Uuid, entity_type: EntityType) -> bool {
    // Decision properties require the canonical Document subtype, which this
    // entity-type-only helper cannot prove. Subtype-aware mutation paths use
    // `is_property_applicable_to_subject` below.
    if is_decision_property(property_id) {
        return false;
    }

    // Task-only properties: Parent Task, Subtasks, and Depends On.
    if property_id == SystemPropertyKey::PARENT_TASK_UUID
        || property_id == SystemPropertyKey::SUBTASKS_UUID
        || property_id == SystemPropertyKey::DEPENDS_ON_UUID
    {
        return entity_type == EntityType::Task;
    }

    // CRM-company-only properties: Stage, Owner, Revenue
    if property_id == SystemPropertyKey::STAGE_UUID
        || property_id == SystemPropertyKey::COMPANY_OWNER_UUID
        || property_id == SystemPropertyKey::REVENUE_UUID
    {
        return entity_type == EntityType::Company;
    }

    true
}

/// Check applicability after canonical Document subtype resolution.
pub fn is_property_applicable_to_subject(
    property_id: Uuid,
    entity_type: EntityType,
    document_sub_type: Option<DocumentSubType>,
) -> bool {
    if is_decision_property(property_id) {
        return entity_type == EntityType::Document
            && document_sub_type == Some(DocumentSubType::Decision);
    }

    is_property_applicable_to(property_id, entity_type)
}

/// Check whether a property row is an undeletable built-in for this subject.
pub fn is_property_required_for_subject(
    property_id: Uuid,
    entity_type: EntityType,
    document_sub_type: Option<DocumentSubType>,
) -> bool {
    if is_decision_property(property_id) {
        return entity_type == EntityType::Document
            && document_sub_type == Some(DocumentSubType::Decision);
    }

    SystemPropertyKey::is_required_for_entity(property_id, entity_type)
}

fn is_decision_property(property_id: Uuid) -> bool {
    matches!(
        property_id,
        SystemPropertyKey::DECISION_STATE_UUID
            | SystemPropertyKey::DECIDED_BY_UUID
            | SystemPropertyKey::DECIDED_AT_UUID
            | SystemPropertyKey::DECISION_SOURCES_UUID
    )
}

const MAX_DECISION_SOURCE_LINKS: usize = 20;
const MAX_DECISION_SOURCE_URL_BYTES: usize = 2_048;

/// Validate the externally navigable URLs stored in Decision Source Links.
pub fn validate_decision_source_links(
    property_id: Uuid,
    value: &Option<SetPropertyValue>,
) -> Result<(), &'static str> {
    if property_id != SystemPropertyKey::DECISION_SOURCES_UUID {
        return Ok(());
    }

    let Some(SetPropertyValue::MultiLink { urls }) = value else {
        return Ok(());
    };
    if urls.len() > MAX_DECISION_SOURCE_LINKS {
        return Err("Decision Source Links supports at most 20 URLs");
    }

    for raw in urls {
        if raw.is_empty() || raw.len() > MAX_DECISION_SOURCE_URL_BYTES {
            return Err("Decision Source Links URLs must be between 1 and 2048 bytes");
        }
        let parsed = url::Url::parse(raw)
            .map_err(|_| "Decision Source Links must contain valid HTTP(S) URLs")?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err("Decision Source Links must contain valid HTTP(S) URLs");
        }
    }

    Ok(())
}

/// Enforce the Decision v1 single-USER contract for Decided By.
pub fn validate_decision_decided_by(
    property_id: Uuid,
    value: &Option<SetPropertyValue>,
) -> Result<(), &'static str> {
    if property_id != SystemPropertyKey::DECIDED_BY_UUID {
        return Ok(());
    }

    if let Some(SetPropertyValue::EntityReference { reference }) = value {
        if reference.entity_type != EntityType::User {
            return Err("Decision Decided By must reference a user");
        }
        if reference.specific_message_id.is_some()
            || MacroUserIdStr::parse_from_str(&reference.entity_id).is_err()
        {
            return Err("Decision Decided By must contain a valid Macro user ID");
        }
    }

    Ok(())
}

/// Keep the required Decision State non-null and inside its fixed four-state vocabulary.
pub fn validate_decision_state(
    property_id: Uuid,
    value: &Option<SetPropertyValue>,
) -> Result<(), &'static str> {
    if property_id != SystemPropertyKey::DECISION_STATE_UUID {
        return Ok(());
    }

    match value {
        Some(SetPropertyValue::SelectOption { option_id })
            if DecisionStateOption::from_uuid(*option_id).is_some() =>
        {
            Ok(())
        }
        _ => Err("Decision State must be one of Proposed, Accepted, Rejected, or Superseded"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_properties_require_the_decision_subtype() {
        for property_id in [
            SystemPropertyKey::DECISION_STATE_UUID,
            SystemPropertyKey::DECIDED_BY_UUID,
            SystemPropertyKey::DECIDED_AT_UUID,
            SystemPropertyKey::DECISION_SOURCES_UUID,
        ] {
            assert!(!is_property_applicable_to_subject(
                property_id,
                EntityType::Document,
                None,
            ));
            assert!(!is_property_applicable_to_subject(
                property_id,
                EntityType::Task,
                Some(DocumentSubType::Task),
            ));
            assert!(is_property_applicable_to_subject(
                property_id,
                EntityType::Document,
                Some(DocumentSubType::Decision),
            ));
            assert!(is_property_required_for_subject(
                property_id,
                EntityType::Document,
                Some(DocumentSubType::Decision),
            ));
        }
    }

    #[test]
    fn decision_source_links_reject_unsafe_and_unbounded_values() {
        for url in ["javascript:alert(1)", "file:///etc/passwd", "https://"] {
            assert!(
                validate_decision_source_links(
                    SystemPropertyKey::DECISION_SOURCES_UUID,
                    &Some(SetPropertyValue::MultiLink {
                        urls: vec![url.to_string()],
                    }),
                )
                .is_err()
            );
        }
        assert!(
            validate_decision_source_links(
                SystemPropertyKey::DECISION_SOURCES_UUID,
                &Some(SetPropertyValue::MultiLink {
                    urls: vec!["https://example.test/source".to_string()],
                }),
            )
            .is_ok()
        );
        assert!(
            validate_decision_source_links(
                SystemPropertyKey::DECISION_SOURCES_UUID,
                &Some(SetPropertyValue::MultiLink {
                    urls: (0..=MAX_DECISION_SOURCE_LINKS)
                        .map(|i| format!("https://example.test/{i}"))
                        .collect(),
                }),
            )
            .is_err()
        );
    }

    #[test]
    fn decided_by_rejects_non_user_references() {
        assert!(
            validate_decision_decided_by(
                SystemPropertyKey::DECIDED_BY_UUID,
                &Some(SetPropertyValue::EntityReference {
                    reference: models_properties::shared::EntityReference {
                        entity_type: EntityType::Project,
                        entity_id: "project-1".to_string(),
                        specific_message_id: None,
                    },
                }),
            )
            .is_err()
        );

        for entity_id in ["", "not-a-macro-user"] {
            assert!(
                validate_decision_decided_by(
                    SystemPropertyKey::DECIDED_BY_UUID,
                    &Some(SetPropertyValue::EntityReference {
                        reference: models_properties::shared::EntityReference {
                            entity_type: EntityType::User,
                            entity_id: entity_id.to_string(),
                            specific_message_id: None,
                        },
                    }),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn decision_state_is_non_null_and_uses_the_fixed_vocabulary() {
        assert!(validate_decision_state(SystemPropertyKey::DECISION_STATE_UUID, &None).is_err());
        assert!(
            validate_decision_state(
                SystemPropertyKey::DECISION_STATE_UUID,
                &Some(SetPropertyValue::SelectOption {
                    option_id: Uuid::new_v4(),
                }),
            )
            .is_err()
        );
        assert!(
            validate_decision_state(
                SystemPropertyKey::DECISION_STATE_UUID,
                &Some(SetPropertyValue::SelectOption {
                    option_id: DecisionStateOption::Accepted.uuid(),
                }),
            )
            .is_ok()
        );
    }
}
