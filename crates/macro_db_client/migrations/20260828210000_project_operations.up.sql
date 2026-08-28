CREATE TABLE project_operations (
    project_id TEXT PRIMARY KEY REFERENCES "Project"(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'planned'
        CHECK (status IN ('planned', 'active', 'paused', 'completed', 'archived')),
    priority TEXT NOT NULL DEFAULT 'normal'
        CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    lead_user_id TEXT REFERENCES "User"(id) ON DELETE SET NULL,
    start_date DATE,
    target_date DATE,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    policy JSONB,
    CONSTRAINT project_operations_date_order CHECK (
        start_date IS NULL OR target_date IS NULL OR start_date <= target_date
    ),
    CONSTRAINT project_operations_policy_object CHECK (
        policy IS NULL OR (jsonb_typeof(policy) = 'object' AND octet_length(policy::text) <= 4096)
    )
);

INSERT INTO project_operations (project_id)
SELECT id FROM "Project"
ON CONFLICT (project_id) DO NOTHING;

CREATE FUNCTION project_operations_create_for_project()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    INSERT INTO project_operations (project_id) VALUES (NEW.id);
    RETURN NEW;
END;
$$;

CREATE TRIGGER project_operations_create_for_project
AFTER INSERT ON "Project"
FOR EACH ROW EXECUTE FUNCTION project_operations_create_for_project();
