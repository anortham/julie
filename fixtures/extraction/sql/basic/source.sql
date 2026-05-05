CREATE TABLE workers (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE jobs (
    id INTEGER PRIMARY KEY,
    worker_id INTEGER NOT NULL,
    FOREIGN KEY (worker_id) REFERENCES workers(id)
);

CREATE VIEW active_workers AS
SELECT id, name
FROM workers
WHERE id > 0;
