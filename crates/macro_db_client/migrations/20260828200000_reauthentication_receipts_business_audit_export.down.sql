DELETE FROM reauthentication_receipts
WHERE purpose = 'business_audit_export';

ALTER TABLE reauthentication_receipts
    DROP CONSTRAINT reauthentication_receipts_purpose_check,
    ADD CONSTRAINT reauthentication_receipts_purpose_check
        CHECK (purpose = 'company_role_change');
