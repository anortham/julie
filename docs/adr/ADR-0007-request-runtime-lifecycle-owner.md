# ADR-0007: Request runtime lifecycle has one production owner

## Context

The machine service needs bounded checkout runtime retention without duplicate
watchers or writers. The old `WorkspaceRuntimeManager`/lease design was a
second, non-production lifecycle path.

## Decision

`RequestEngine` and `RuntimeFactory` are the sole production lifecycle owner.
Each bound key has one `RuntimeSlot`; its slot lock serializes initialization,
and an empty slot remains after eviction as lightweight key metadata.

The service periodically retires idle runtimes and drains them at shutdown.
Teardown quiesces watcher and mutation work, then cancels and joins embedding
tasks before clearing the runtime. Eviction retains durable facts, Tantivy,
vectors, and registry data for the next open.

Rejected: revive `WorkspaceRuntimeManager`, leases, a second lifecycle state
machine, or durable runtime lifecycle records. They duplicate the active path
without strengthening its ownership guarantees.

## Consequences

- Cold opens for one key cannot install duplicate writers; other keys remain independent.
- Reopening an evicted checkout reuses durable index data but recreates only live handles.
- Empty slots trade a small monotonic metadata map for stable per-key coordination.

## Applies To

`src/request_engine/runtime_factory.rs`, `src/request_engine/dispatch.rs`,
`src/service`, and handler teardown/embedding shutdown paths.

## Future Agents

Extend lifecycle behavior through `RuntimeFactory` and the service loop. Do not
add a manager, lease, or parallel owner; remove empty-slot metadata only with a
replacement that preserves per-key initialization and retirement coordination.
