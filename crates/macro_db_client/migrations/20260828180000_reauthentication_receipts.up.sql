CREATE TABLE reauthentication_receipts (
    id UUID PRIMARY KEY,
    team_id UUID NOT NULL,
    principal TEXT NOT NULL CHECK (
        octet_length(principal) BETWEEN 1 AND 256
        AND principal LIKE 'macro|%'
    ),
    purpose TEXT NOT NULL CHECK (purpose = 'company_role_change'),
    proof_method TEXT NOT NULL CHECK (proof_method = 'password'),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    request_id TEXT NOT NULL CHECK (
        octet_length(request_id) BETWEEN 1 AND 256
        AND btrim(request_id) <> ''
    ),
    consumed_at TIMESTAMPTZ,
    CHECK (expires_at = issued_at + INTERVAL '5 minutes'),
    CHECK (
        consumed_at IS NULL
        OR consumed_at BETWEEN issued_at AND expires_at
    )
);
