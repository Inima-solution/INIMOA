DELETE FROM reauthentication_receipts
WHERE proof_method = 'password_mfa';

ALTER TABLE reauthentication_receipts
    DROP CONSTRAINT reauthentication_receipts_proof_method_check,
    ADD CONSTRAINT reauthentication_receipts_proof_method_check
        CHECK (proof_method = 'password');
