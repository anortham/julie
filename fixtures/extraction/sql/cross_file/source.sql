-- Phase 3.1 fixture: cross-schema FK references must produce a
-- StructuredPendingRelationship with target.namespace_path=["other_schema"]
-- and target.terminal_name="users". The local-only `audit_events` FK to
-- `orders.id` resolves concretely.
CREATE TABLE orders (
    id INT PRIMARY KEY,
    user_id INT REFERENCES other_schema.users(id)
);

CREATE TABLE audit_events (
    id INT PRIMARY KEY,
    order_id INT REFERENCES orders(id)
);
