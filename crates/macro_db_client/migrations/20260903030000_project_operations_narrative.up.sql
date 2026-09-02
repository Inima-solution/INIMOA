ALTER TABLE project_operations
    ADD COLUMN objective TEXT,
    ADD COLUMN next_action TEXT,
    ADD CONSTRAINT project_operations_objective_bound CHECK (
        objective IS NULL OR (
            octet_length(objective) BETWEEN 1 AND 2048
            AND btrim(
                objective,
                E' \t\n\r\f\v' || U&'\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000'
            ) <> ''
        )
    ),
    ADD CONSTRAINT project_operations_next_action_bound CHECK (
        next_action IS NULL OR (
            octet_length(next_action) BETWEEN 1 AND 1024
            AND btrim(
                next_action,
                E' \t\n\r\f\v' || U&'\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000'
            ) <> ''
        )
    );
