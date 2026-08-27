CREATE TYPE business_role AS ENUM (
    'member',
    'manager',
    'approver',
    'hr_admin',
    'payroll_admin',
    'org_admin',
    'auditor',
    'agent'
);

CREATE TABLE team_business_role (
    team_id UUID NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    principal TEXT NOT NULL,
    business_role business_role NOT NULL,
    granted_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT team_business_role_pkey PRIMARY KEY (team_id, principal, business_role),
    CONSTRAINT team_business_role_member_is_derived CHECK (business_role <> 'member')
);

CREATE INDEX team_business_role_team_role_idx
    ON team_business_role (team_id, business_role);

CREATE INDEX team_business_role_principal_idx
    ON team_business_role (principal);
