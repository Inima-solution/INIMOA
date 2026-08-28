INSERT INTO macro_user (id, username, email, stripe_customer_id)
VALUES (
    'd3000000-0000-0000-0000-000000000001',
    'task-dependencies@example.test',
    'task-dependencies@example.test',
    'cus_task_dependencies'
);
INSERT INTO "User" (id, email, macro_user_id)
VALUES (
    'task-dependencies-owner',
    'task-dependencies@example.test',
    'd3000000-0000-0000-0000-000000000001'
);
INSERT INTO "Project" (id, name, "userId") VALUES
('task-dependencies-project-a', 'A', 'task-dependencies-owner'),
('task-dependencies-project-b', 'B', 'task-dependencies-owner');
