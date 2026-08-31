INSERT INTO property_definitions (
    id,
    team_id,
    user_id,
    display_name,
    data_type,
    is_multi_select,
    specific_entity_type,
    is_system
)
VALUES (
    '00000001-0000-0000-0000-000000000014',
    NULL,
    NULL,
    'Start Date',
    'DATE',
    false,
    NULL,
    true
);

INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values)
SELECT
    gen_random_uuid(),
    d.id,
    'TASK',
    '00000001-0000-0000-0000-000000000014',
    NULL
FROM "Document" d
JOIN document_sub_type dst ON dst.document_id = d.id
WHERE dst.sub_type = 'task'
ON CONFLICT (entity_id, entity_type, property_definition_id) DO NOTHING;
