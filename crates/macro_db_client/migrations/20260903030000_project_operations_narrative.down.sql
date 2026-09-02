ALTER TABLE project_operations
    DROP CONSTRAINT IF EXISTS project_operations_next_action_bound,
    DROP CONSTRAINT IF EXISTS project_operations_objective_bound,
    DROP COLUMN IF EXISTS next_action,
    DROP COLUMN IF EXISTS objective;
