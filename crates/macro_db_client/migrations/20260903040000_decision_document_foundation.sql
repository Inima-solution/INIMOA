ALTER TYPE document_sub_type_value ADD VALUE IF NOT EXISTS 'decision';

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
VALUES
    (
        '00000001-0000-0000-0000-000000000015',
        NULL,
        NULL,
        'Decision State',
        'SELECT_STRING',
        false,
        NULL,
        true
    ),
    (
        '00000001-0000-0000-0000-000000000016',
        NULL,
        NULL,
        'Decided By',
        'ENTITY',
        false,
        'USER',
        true
    ),
    (
        '00000001-0000-0000-0000-000000000017',
        NULL,
        NULL,
        'Decided At',
        'DATE',
        false,
        NULL,
        true
    ),
    (
        '00000001-0000-0000-0000-000000000018',
        NULL,
        NULL,
        'Source Links',
        'LINK',
        true,
        NULL,
        true
    );

INSERT INTO property_options (
    id,
    property_definition_id,
    display_order,
    string_value
)
VALUES
    (
        '00000001-0000-0000-0015-000000000001',
        '00000001-0000-0000-0000-000000000015',
        0,
        'Proposed'
    ),
    (
        '00000001-0000-0000-0015-000000000002',
        '00000001-0000-0000-0000-000000000015',
        1,
        'Accepted'
    ),
    (
        '00000001-0000-0000-0015-000000000003',
        '00000001-0000-0000-0000-000000000015',
        2,
        'Rejected'
    ),
    (
        '00000001-0000-0000-0015-000000000004',
        '00000001-0000-0000-0000-000000000015',
        3,
        'Superseded'
    );
