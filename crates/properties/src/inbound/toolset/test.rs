use super::set_entity_property::map_set_entity_property_error;
#[allow(unused_imports)]
use super::*;
use ai_toolset::schema::generate_validated_input_schema;

#[test]
fn property_target_schemas_expose_document_and_exclude_task() {
    let get_schema = serde_json::to_value(
        &generate_validated_input_schema::<GetEntityProperties>()
            .unwrap()
            .schema,
    )
    .unwrap();
    let set_schema = serde_json::to_value(
        &generate_validated_input_schema::<SetEntityProperty>()
            .unwrap()
            .schema,
    )
    .unwrap();
    let bulk_schema = serde_json::to_value(
        &generate_validated_input_schema::<BulkSetEntityPropertyOptions>()
            .unwrap()
            .schema,
    )
    .unwrap();

    let expected_targets = serde_json::json!([
        "document", "project", "chat", "thread", "channel", "call", "user", "company"
    ]);
    for schema in [&get_schema, &set_schema] {
        assert_eq!(
            schema["properties"]["entity_type"]["enum"],
            expected_targets
        );
    }
    assert_eq!(
        bulk_schema["properties"]["entities"]["items"]["properties"]["entity_type"]["enum"],
        expected_targets
    );
}

#[test]
fn set_entity_property_reference_schema_keeps_task_for_parent_and_subtasks() {
    let schema = serde_json::to_value(
        &generate_validated_input_schema::<SetEntityProperty>()
            .unwrap()
            .schema,
    )
    .unwrap();
    assert_eq!(
        schema["properties"]["entity_ref"]["properties"]["entityType"]["enum"],
        serde_json::json!([
            "document", "task", "project", "chat", "thread", "channel", "call", "user", "company"
        ])
    );
    assert_eq!(
        schema["properties"]["entity_refs"]["items"]["properties"]["entityType"]["enum"],
        serde_json::json!([
            "document", "task", "project", "chat", "thread", "channel", "call", "user", "company"
        ])
    );
}

#[test]
fn test_get_entity_properties_schema_validation() {
    let result = generate_validated_input_schema::<GetEntityProperties>();
    assert!(result.is_ok(), "{:?}", result);

    let validated = result.unwrap();
    assert_eq!(validated.name, "GetEntityProperties");
    assert!(
        validated.description.contains("Get all properties"),
        "Description should contain expected text"
    );
}

#[test]
fn test_set_entity_property_schema_validation() {
    let result = generate_validated_input_schema::<SetEntityProperty>();
    assert!(result.is_ok(), "{:?}", result);

    let validated = result.unwrap();
    assert_eq!(validated.name, "SetEntityProperty");
    assert!(
        validated.description.contains("Set or update a property"),
        "Description should contain expected text"
    );
}

#[test]
fn test_set_entity_property_schema_documents_delta_options() {
    let validated = generate_validated_input_schema::<SetEntityProperty>().unwrap();
    let schema_json = serde_json::to_string(&validated.schema).unwrap();
    assert!(
        schema_json.contains("add_option_ids"),
        "schema should expose add_option_ids"
    );
    assert!(
        schema_json.contains("remove_option_ids"),
        "schema should expose remove_option_ids"
    );
    assert!(
        validated.description.contains("atomically"),
        "description should steer to atomic add/remove over full replace"
    );
}

#[test]
fn test_bulk_set_entity_property_options_schema_validation() {
    let result = generate_validated_input_schema::<BulkSetEntityPropertyOptions>();
    assert!(result.is_ok(), "{:?}", result);

    let validated = result.unwrap();
    assert_eq!(validated.name, "BulkSetEntityPropertyOptions");
    assert!(
        validated.description.contains("many entities"),
        "Description should explain the multi-entity apply"
    );

    let schema_json = serde_json::to_string(&validated.schema).unwrap();
    assert!(
        schema_json.contains("entities")
            && schema_json.contains("add_option_ids")
            && schema_json.contains("remove_option_ids"),
        "schema should expose entities and the add/remove option deltas"
    );
}

#[test]
fn test_list_tags_schema_validation() {
    let result = generate_validated_input_schema::<ListTags>();
    assert!(result.is_ok(), "{:?}", result);

    let validated = result.unwrap();
    assert_eq!(validated.name, "ListTags");
    assert!(
        validated.description.contains("personal tag set"),
        "Description should explain the personal/team tag sets"
    );
    assert!(
        validated.description.contains("SetEntityProperty"),
        "Description should point at SetEntityProperty for applying tags"
    );
}

#[test]
fn test_create_tag_schema_validation() {
    let result = generate_validated_input_schema::<CreateTag>();
    assert!(result.is_ok(), "{:?}", result);

    let validated = result.unwrap();
    assert_eq!(validated.name, "CreateTag");
    assert!(
        validated.description.contains("Create a new tag"),
        "Description should explain that it creates a new tag"
    );

    let schema_json = serde_json::to_string(&validated.schema).unwrap();
    assert!(
        schema_json.contains("label") && schema_json.contains("color"),
        "schema should expose label and color"
    );
    assert!(
        schema_json.contains("scope"),
        "schema should expose the personal/team scope"
    );
}

#[test]
fn test_edit_tag_schema_validation() {
    let result = generate_validated_input_schema::<EditTag>();
    assert!(result.is_ok(), "{:?}", result);

    let validated = result.unwrap();
    assert_eq!(validated.name, "EditTag");
    assert!(
        validated.description.contains("Rename or recolor"),
        "Description should explain rename/recolor"
    );

    let schema_json = serde_json::to_string(&validated.schema).unwrap();
    assert!(
        schema_json.contains("label") && schema_json.contains("color"),
        "schema should expose label and color"
    );
    assert!(
        schema_json.contains("property_definition_id"),
        "schema should require the tag set's property_definition_id"
    );
}

#[test]
fn test_delete_tag_schema_validation() {
    let result = generate_validated_input_schema::<DeleteTag>();
    assert!(result.is_ok(), "{:?}", result);

    let validated = result.unwrap();
    assert_eq!(validated.name, "DeleteTag");
    assert!(
        validated.description.contains("Permanently delete a tag"),
        "Description should explain the destructive delete"
    );

    let schema_json = serde_json::to_string(&validated.schema).unwrap();
    assert!(
        schema_json.contains("property_definition_id"),
        "schema should require the tag set's property_definition_id"
    );
}

#[test]
fn set_entity_property_response_omits_readiness_for_success_and_exposes_exact_blocker_shape() {
    let task_id = uuid::Uuid::from_u128(0xB01);
    let blocking_id = uuid::Uuid::from_u128(0xB02);
    assert_eq!(
        serde_json::to_value(SetEntityPropertyResponse {
            success: true,
            message: "Property updated successfully.".to_owned(),
            task_dependency_readiness: None,
            task_subtask_completion_readiness: None,
        })
        .unwrap(),
        serde_json::json!({
            "success": true,
            "message": "Property updated successfully.",
        })
    );
    assert_eq!(
        serde_json::to_value(SetEntityPropertyResponse {
            success: false,
            message: "Task transition is blocked by dependencies".to_owned(),
            task_dependency_readiness: Some(crate::domain::model::TaskDependencyReadiness {
                task_id,
                readiness: crate::domain::model::TaskReadiness::Blocked,
                depends_on_task_ids: vec![blocking_id],
                blocking_task_ids: vec![blocking_id],
                has_unavailable_dependencies: true,
            }),
            task_subtask_completion_readiness: None,
        })
        .unwrap(),
        serde_json::json!({
            "success": false,
            "message": "Task transition is blocked by dependencies",
            "taskDependencyReadiness": {
                "taskId": task_id,
                "readiness": "blocked",
                "dependsOnTaskIds": [blocking_id],
                "blockingTaskIds": [blocking_id],
                "hasUnavailableDependencies": true,
            },
        })
    );
}

#[test]
fn set_entity_property_response_schema_contains_optional_five_field_readiness() {
    let schema = serde_json::to_value(schemars::schema_for!(SetEntityPropertyResponse)).unwrap();
    assert!(schema["properties"]["taskDependencyReadiness"].is_object());
    let schema_text = schema.to_string();
    for key in [
        "taskDependencyReadiness",
        "taskId",
        "readiness",
        "dependsOnTaskIds",
        "blockingTaskIds",
        "hasUnavailableDependencies",
        "taskSubtaskCompletionReadiness",
        "subtaskIds",
        "blockingSubtaskIds",
        "hasUnavailableSubtasks",
    ] {
        assert!(
            schema_text.contains(key),
            "missing {key} from response schema"
        );
    }
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "taskDependencyReadiness")
    );
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "taskSubtaskCompletionReadiness")
    );
}

#[test]
fn set_entity_property_completion_blocker_is_structured_and_exact() {
    let task_id = uuid::Uuid::from_u128(0xB21);
    let blocker = uuid::Uuid::from_u128(0xB22);
    let response = map_set_entity_property_error(
        crate::domain::error::PropertiesErr::TaskCompletionBlockedBySubtasks(
            crate::domain::model::TaskSubtaskCompletionBlockedDetails::new(
                crate::domain::model::TaskSubtaskCompletionReadiness {
                    task_id,
                    readiness: crate::domain::model::TaskReadiness::Blocked,
                    subtask_ids: vec![blocker],
                    blocking_subtask_ids: vec![blocker],
                    has_unavailable_subtasks: true,
                },
            ),
        ),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(response).unwrap(),
        serde_json::json!({
            "success": false,
            "message": "Task completion is blocked by subtasks",
            "taskSubtaskCompletionReadiness": {
                "taskId": task_id,
                "readiness": "blocked",
                "subtaskIds": [blocker],
                "blockingSubtaskIds": [blocker],
                "hasUnavailableSubtasks": true,
            }
        })
    );
}

#[test]
fn set_entity_property_error_mapping_executes_structured_and_ordinary_branches() {
    let task_id = uuid::Uuid::from_u128(0xB11);
    let blocker = uuid::Uuid::from_u128(0xB12);
    let structured = map_set_entity_property_error(
        crate::domain::error::PropertiesErr::TaskTransitionBlockedWithReadiness(
            crate::domain::model::TaskTransitionBlockedDetails::new(
                crate::domain::model::TaskDependencyReadiness {
                    task_id,
                    readiness: crate::domain::model::TaskReadiness::Blocked,
                    depends_on_task_ids: vec![blocker],
                    blocking_task_ids: vec![blocker],
                    has_unavailable_dependencies: false,
                },
            ),
        ),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(structured).unwrap(),
        serde_json::json!({
            "success": false,
            "message": "Task transition is blocked by dependencies",
            "taskDependencyReadiness": {
                "taskId": task_id,
                "readiness": "blocked",
                "dependsOnTaskIds": [blocker],
                "blockingTaskIds": [blocker],
                "hasUnavailableDependencies": false,
            },
        })
    );

    let ordinary =
        map_set_entity_property_error(crate::domain::error::PropertiesErr::TaskDependencyCycle)
            .unwrap_err();
    assert_eq!(
        ordinary.description,
        "Failed to set property: Task dependencies cannot contain a cycle"
    );
    assert_eq!(
        ordinary.internal_error.to_string(),
        "Task dependencies cannot contain a cycle"
    );
}

// run `cargo test -p properties inbound::toolset::test::print_get_input_schema -- --nocapture --include-ignored`
#[test]
#[ignore = "prints the input schema"]
fn print_get_input_schema() {
    let schema = generate_validated_input_schema::<GetEntityProperties>()
        .unwrap()
        .schema;
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}

// run `cargo test -p properties inbound::toolset::test::print_set_input_schema -- --nocapture --include-ignored`
#[test]
#[ignore = "prints the input schema"]
fn print_set_input_schema() {
    let schema = generate_validated_input_schema::<SetEntityProperty>()
        .unwrap()
        .schema;
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}

// run `cargo test -p properties inbound::toolset::test::print_get_output_schema -- --nocapture --include-ignored`
#[test]
#[ignore = "prints the output schema"]
fn print_get_output_schema() {
    let schema = schemars::schema_for!(GetEntityPropertiesResponse);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}

// run `cargo test -p properties inbound::toolset::test::print_set_output_schema -- --nocapture --include-ignored`
#[test]
#[ignore = "prints the output schema"]
fn print_set_output_schema() {
    let schema = schemars::schema_for!(SetEntityPropertyResponse);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
