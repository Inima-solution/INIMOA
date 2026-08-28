ALTER TABLE reauthentication_receipts
    DROP CONSTRAINT reauthentication_receipts_purpose_check,
    ADD CONSTRAINT reauthentication_receipts_purpose_check
        CHECK (purpose IN ('company_role_change', 'business_audit_export'));
