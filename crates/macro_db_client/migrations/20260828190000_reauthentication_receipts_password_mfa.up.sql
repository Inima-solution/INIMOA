ALTER TABLE reauthentication_receipts
    DROP CONSTRAINT reauthentication_receipts_proof_method_check,
    ADD CONSTRAINT reauthentication_receipts_proof_method_check
        CHECK (proof_method IN ('password', 'password_mfa'));
