CREATE TABLE business_audit_events (
    id UUID PRIMARY KEY,
    team_id UUID NOT NULL,
    actor TEXT NOT NULL CHECK (octet_length(actor) BETWEEN 1 AND 256),
    delegated_actor TEXT CHECK (
        delegated_actor IS NULL OR octet_length(delegated_actor) BETWEEN 1 AND 256
    ),
    action TEXT NOT NULL CHECK (octet_length(action) BETWEEN 1 AND 64),
    target_type TEXT NOT NULL CHECK (octet_length(target_type) BETWEEN 1 AND 64),
    target_id TEXT NOT NULL CHECK (octet_length(target_id) BETWEEN 1 AND 256),
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'denied', 'failed')),
    occurred_at TIMESTAMPTZ NOT NULL,
    request_id TEXT NOT NULL CHECK (octet_length(request_id) BETWEEN 1 AND 256),
    reason TEXT CHECK (reason IS NULL OR octet_length(reason) BETWEEN 1 AND 1000),
    metadata JSONB NOT NULL CHECK (
        jsonb_typeof(metadata) = 'object'
        AND octet_length(metadata::text) <= 4096
    ),
    retention_class TEXT NOT NULL CHECK (
        retention_class IN ('standard', 'confidential', 'restricted')
    )
);

CREATE INDEX business_audit_events_team_time_idx
    ON business_audit_events (team_id, occurred_at DESC, id DESC);

CREATE FUNCTION reject_business_audit_event_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'business audit events are immutable' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER business_audit_events_immutable
BEFORE UPDATE OR DELETE ON business_audit_events
FOR EACH ROW EXECUTE FUNCTION reject_business_audit_event_mutation();
