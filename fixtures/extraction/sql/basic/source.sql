CREATE TABLE workers (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE VIEW active_workers AS
SELECT id, name
FROM workers
WHERE id > 0;
