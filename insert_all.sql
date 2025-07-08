-- Drop and create schema
DROP TABLE IF EXISTS node_tps, node_overhead, node_resource_utilization, experiment_summary CASCADE;

CREATE TABLE experiment_summary (
    id SERIAL PRIMARY KEY,
    protocol TEXT NOT NULL,
    filename TEXT NOT NULL,
    nodes INTEGER NOT NULL,
    txpb INTEGER NOT NULL,
    rounds INTEGER NOT NULL,
    avg_tps REAL,
    avg_overhead REAL,
    avg_cpu REAL,
    avg_mem REAL,
    latency_seconds REAL
);

CREATE TABLE node_tps (
    id SERIAL PRIMARY KEY,
    experiment_id INTEGER NOT NULL REFERENCES experiment_summary(id),
    node_id TEXT NOT NULL,
    tps REAL
);

CREATE TABLE node_overhead (
    id SERIAL PRIMARY KEY,
    experiment_id INTEGER NOT NULL REFERENCES experiment_summary(id),
    node_id TEXT NOT NULL,
    overhead_messages INTEGER
);

CREATE TABLE node_resource_utilization (
    id SERIAL PRIMARY KEY,
    experiment_id INTEGER NOT NULL REFERENCES experiment_summary(id),
    node_id TEXT NOT NULL,
    cpu_util REAL,
    mem_util REAL
);

-- Optional truncation for repeatable runs
-- TRUNCATE TABLE node_tps, node_overhead, node_resource_utilization, experiment_summary RESTART IDENTITY CASCADE;

