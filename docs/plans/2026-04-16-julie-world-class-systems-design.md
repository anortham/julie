# Julie World-Class Systems Design

**Status:** Draft for review
**Date:** 2026-04-16
**Area:** `src/daemon/`, `src/adapter/`, `src/workspace/`, `src/tools/workspace/indexing/`, `src/database/`, `src/search/`, `src/dashboard/`, `python/embeddings_sidecar/`

---

## Goal

Turn Julie's core runtime into a world-class foundation for AI agents by redesigning the systems that determine correctness, reliability, cross-platform behavior, and recovery, not by layering more fixes on top of fragile seams.

This program prioritizes:

1. Correctness and reliability over feature velocity
2. Structural improvement over local patching
3. Measurable health, repairability, and dogfooding quality over opaque "it probably works"

## Why This Exists

Recent tree-sitter hardening showed that foundational work pays off when we attack the right layer. Julie's next ceiling is no longer extractor coverage, it is the quality of the platform around extraction:

- daemon lifecycle and stale-binary replacement
- initial, catch-up, and incremental indexing behavior
- SQLite and Tantivy consistency
- embedding sidecar startup, capability detection, and degradation behavior
- cross-platform behavior, with Windows as a first-class target

The current system works and delivers value, but the hot paths carry too much responsibility and too many historical incident fixes inline. Comments such as "critical fix," "deadlock fix," and "corruption window" are useful archaeology, and they also signal that the architecture is making routine correctness too expensive.

Representative pressure points:

- [`src/handler.rs`](/Users/murphy/source/julie/src/handler.rs) centralizes too much workspace, lifecycle, and request orchestration.
- [`src/daemon/mod.rs`](/Users/murphy/source/julie/src/daemon/mod.rs), [`src/adapter/mod.rs`](/Users/murphy/source/julie/src/adapter/mod.rs), and [`src/daemon/ipc_session.rs`](/Users/murphy/source/julie/src/daemon/ipc_session.rs) split lifecycle authority across multiple flows.
- [`src/tools/workspace/indexing/processor.rs`](/Users/murphy/source/julie/src/tools/workspace/indexing/processor.rs) mixes parse, persist, project, resolve, analyze, and snapshot work in one pipeline.
- [`src/database/bulk_operations.rs`](/Users/murphy/source/julie/src/database/bulk_operations.rs) performs heroic consistency work that should be easier to express in the data model.
- [`python/embeddings_sidecar/sidecar/runtime.py`](/Users/murphy/source/julie/python/embeddings_sidecar/sidecar/runtime.py) owns probing, patching, fallback, and backend policy in one runtime boundary.

This is the right moment to do structural surgery. Julie is stable enough to support aggressive refactoring, there is no competing feature roadmap, and generated state can be discarded when the cleaner architecture earns it.

## Product Standard

This program is successful when Julie becomes:

- trustworthy, stale or wrong answers are rare and detectable
- deterministic, restarts, rebuilds, and degradation follow known rules
- cross-platform by design, not by post-hoc patching
- repairable, projections and sidecars can fall behind or fail without leaving the system mysterious
- observable, humans and agents can see system health without digging through logs
- efficient in daily dogfooding, fast enough, calm enough, and predictable enough that agents want Julie in the loop

## Program Shape

This program uses a shared contract reset followed by parallel subsystem tracks.

The design treats Julie as three first-class planes:

- `Control plane`: daemon, adapter, IPC, session lifecycle, workspace lifecycle, stale-binary replacement, platform-specific transport behavior
- `Data plane`: discovery, initial indexing, catch-up indexing, watcher-driven incremental indexing, canonical persistence, projections, repair flows
- `Runtime plane`: tool-time query behavior, health reporting, ranking, fallback modes, embedding availability, projection freshness

The key architectural move is:

`SQLite is the canonical source of truth for indexed workspace state. Tantivy, vectors, centrality outputs, and similar derived artifacts are rebuildable projections.`

This does not require event sourcing or other architecture cosplay. It does require revisioning, health metadata, repair triggers, and a hard separation between canonical writes and downstream projections.

## Design Principles

### 1. One Canonical Truth

Structured workspace intelligence lives in SQLite. Derived indexes and analyses are products of that state, not peer authorities.

### 2. Explicit State Machines Beat Boolean Folklore

Daemon lifecycle and indexing lifecycle must be modeled with named states and legal transitions, not split across flags, retries, sleeps, and comments.

### 3. One Engine Per Core Workflow

Initial indexing, catch-up indexing, and watcher-driven indexing should share one engine and one routing model. Primary and reference workspaces should use the same core write path.

### 4. Health Is Part of the Product

If a subsystem can be degraded, stale, rebuilding, blocked, or waiting, Julie should know that and surface it clearly.

### 5. Cross-Platform Rules Are Designed Up Front

Windows should not remain the place where transport, path, and replacement assumptions go to die. Platform behavior belongs behind contracts, not smeared through hot paths.

### 6. No Sacred Cows

If a subsystem boundary is the problem, redesign it. Generated state can be blown away and rebuilt when that simplifies the architecture.

## Shared Contracts

The parallel tracks all answer to the same contracts.

### Workspace Identity Contract

Julie needs one canonical workspace identity model that is used consistently across:

- daemon mode
- stdio mode
- primary and reference workspaces
- rebinding and client roots changes
- Unix and Windows
- symlinked and canonicalized paths

This contract owns canonical root selection, path normalization, workspace ID derivation, and how workspace identity is attached to runtime requests.

### Session Lifecycle Contract

Daemon and adapter interactions must follow one explicit lifecycle:

- daemon startup
- readiness publication
- adapter connect and handshake
- version gate outcomes
- stale-binary replacement
- draining and shutdown
- disconnect and cleanup

Platform transports, Unix socket and Windows named pipe, should implement this lifecycle contract rather than distort it.

### Indexing Contract

File indexing should move through named states:

- `discovered`
- `unchanged`
- `dirty`
- `extracting`
- `persisted`
- `projected`
- `failed`
- `repair-needed`

Initial, catch-up, and watcher indexing all use this same state machine. Repair reasons should be recorded, not inferred later from weird symptoms.

### Projection Contract

Canonical writes commit first. Projections then advance toward the canonical revision:

- Tantivy projection revision
- vector projection revision
- analysis revision, for example centrality or test-quality outputs

If a projection fails or lags, Julie records the lag, surfaces it, and repairs it without requiring process restart.

### Health Contract

Every subsystem should report:

- readiness
- degraded state
- freshness or lag
- repairability
- last-known-good state
- blocking reason when progress is stalled

The dashboard and machine-readable endpoints should use this contract directly.

## Architecture Decisions

### Decision 1: SQLite Is Canonical, Projections Are Disposable

Julie should stop treating Tantivy and vectors as semi-independent kingdoms. Canonical structured state belongs in SQLite. Search and embedding projections are rebuildable.

Implications:

- projection corruption becomes a repair event
- rebuilds become routine and safe
- storage upgrades get easier because generated artifacts can be discarded
- search freshness becomes measurable rather than assumed

### Decision 2: Lifecycle and Indexing Use Explicit State Machines

The daemon and indexing pipelines need named states and legal transitions.

Control-plane states should include concepts like:

- daemon `starting`, `ready`, `draining`, `restart-required`, `stopped`
- session `connecting`, `bound`, `serving`, `closing`, `rejected`

Data-plane states should include concepts like:

- file `dirty`, `extracting`, `persisted`, `projected`, `failed`, `repair-needed`
- workspace revision advancement and projection lag

### Decision 3: Index by Snapshot Revision, Not Side-Effect Soup

Indexing should produce canonical workspace revisions. Downstream work then advances against those revisions:

- canonical write commits revision `N`
- Tantivy projects revision `N`
- vectors project revision `N`
- analysis jobs compute revision `N`

If a later stage fails, canonical state remains sound and the repair system knows what lags behind.

### Decision 4: Health Is a First-Class Runtime Surface

Silent degradation is not acceptable for a tool selling speed and trust. Julie must expose subsystem health both to humans and to tools.

### Decision 5: Cross-Platform Behavior Is Designed, Not Patched

Transport readiness, replacement semantics, file locking, path rules, and rebuild expectations should be part of the design. Windows is not "Unix with emotional issues," and the architecture should stop pretending otherwise.

## Workstreams

### Track 0: Contract Reset and Instrumentation

This track lands first and defines the rules for every other track.

Deliverables:

- canonical workspace identity spec
- daemon and session lifecycle state machine
- indexing state machine
- projection revision and health model
- reliability harness for daemon, restart, indexing, and repair scenarios
- benchmark harness for indexing throughput, catch-up latency, search latency, and embedding startup
- cross-platform test matrix with Windows treated as a first-class target
- dashboard-backed health surface for the contracts above

Representative areas:

- [`src/handler.rs`](/Users/murphy/source/julie/src/handler.rs)
- [`src/workspace/mod.rs`](/Users/murphy/source/julie/src/workspace/mod.rs)
- [`src/dashboard/state.rs`](/Users/murphy/source/julie/src/dashboard/state.rs)
- [`src/dashboard/routes/status.rs`](/Users/murphy/source/julie/src/dashboard/routes/status.rs)

### Track 1: Control Plane, Daemon / Adapter / IPC / Windows

This track owns lifecycle determinism and stale-binary replacement.

Current pressure points:

- restart semantics are spread across adapter retry logic, daemon accept-loop logic, version-gate logic, and disconnect-time checks
- Windows named-pipe behavior is coupled into lifecycle flow instead of isolated behind a transport contract
- lifecycle authority is fragmented across multiple files

Goals:

- one explicit lifecycle model
- platform-specific transport behind a narrow interface
- deterministic stale-binary replacement
- boring, testable restart behavior on Windows and Unix

Representative areas:

- [`src/adapter/mod.rs`](/Users/murphy/source/julie/src/adapter/mod.rs)
- [`src/adapter/launcher.rs`](/Users/murphy/source/julie/src/adapter/launcher.rs)
- [`src/daemon/mod.rs`](/Users/murphy/source/julie/src/daemon/mod.rs)
- [`src/daemon/ipc.rs`](/Users/murphy/source/julie/src/daemon/ipc.rs)
- [`src/daemon/ipc_session.rs`](/Users/murphy/source/julie/src/daemon/ipc_session.rs)
- [`src/daemon/workspace_pool.rs`](/Users/murphy/source/julie/src/daemon/workspace_pool.rs)

### Track 2: Data Plane A, Indexing Pipeline Unification

This track owns one indexing engine for full index, catch-up, and watcher-driven updates.

Current pressure points:

- initial, catch-up, and watcher indexing feel related but not unified
- reference and primary workspace routing leaks into hot paths
- parsing, persistence, projection, relationship resolution, analysis, and snapshots are fused into giant methods
- correctness depends on path normalization discipline staying perfect

Goals:

- one indexing engine with explicit stages
- one source of truth for workspace routing
- one repair story for stale, missing, or partial index state
- no silent broad fallbacks without recorded reasons

Representative areas:

- [`src/tools/workspace/indexing/index.rs`](/Users/murphy/source/julie/src/tools/workspace/indexing/index.rs)
- [`src/tools/workspace/indexing/processor.rs`](/Users/murphy/source/julie/src/tools/workspace/indexing/processor.rs)
- [`src/tools/workspace/indexing/incremental.rs`](/Users/murphy/source/julie/src/tools/workspace/indexing/incremental.rs)
- [`src/watcher/`](/Users/murphy/source/julie/src/watcher)
- [`src/startup.rs`](/Users/murphy/source/julie/src/startup.rs)

### Track 3: Data Plane B, Canonical Storage and Rebuildable Projections

This track owns schema, revisioning, Tantivy projection flow, and repairability.

Current pressure points:

- core bulk write paths perform manual constraint triage that should be easier to express
- foreign keys are disabled during major write paths
- Tantivy projection work is synchronous and coarse-grained
- projection failure can degrade into restart-coupled repair

Goals:

- canonical persistence that is easy to keep consistent
- projection lag detection and recovery
- Tantivy and vector rebuilds without daemon restart
- cleaner separation between canonical storage, projections, and maintenance analysis

Representative areas:

- [`src/database/mod.rs`](/Users/murphy/source/julie/src/database/mod.rs)
- [`src/database/bulk_operations.rs`](/Users/murphy/source/julie/src/database/bulk_operations.rs)
- [`src/database/files.rs`](/Users/murphy/source/julie/src/database/files.rs)
- [`src/database/relationships.rs`](/Users/murphy/source/julie/src/database/relationships.rs)
- [`src/search/index.rs`](/Users/murphy/source/julie/src/search/index.rs)
- [`src/search/scoring.rs`](/Users/murphy/source/julie/src/search/scoring.rs)

### Track 4: Runtime Plane, Embeddings and Query-Time Degradation

This track owns the sidecar contract, backend capability reporting, and graceful degraded mode.

Current pressure points:

- startup, device selection, probing, patching, fallback, and batch policy are tightly coupled
- backend-specific quirks are handled, but not yet presented through a clean runtime health contract
- embedding availability can still feel like folklore instead of a surfaced runtime state

Goals:

- explicit embedding runtime contract
- structured capability and degradation reporting
- deterministic query-time behavior when embeddings are unavailable or unhealthy
- platform backends treated as supported modes with known behavior

Representative areas:

- [`src/daemon/embedding_service.rs`](/Users/murphy/source/julie/src/daemon/embedding_service.rs)
- [`src/embeddings/`](/Users/murphy/source/julie/src/embeddings)
- [`src/tools/workspace/indexing/embeddings.rs`](/Users/murphy/source/julie/src/tools/workspace/indexing/embeddings.rs)
- [`python/embeddings_sidecar/sidecar/main.py`](/Users/murphy/source/julie/python/embeddings_sidecar/sidecar/main.py)
- [`python/embeddings_sidecar/sidecar/runtime.py`](/Users/murphy/source/julie/python/embeddings_sidecar/sidecar/runtime.py)

## Dashboard and Observability

The dashboard is part of the architecture, not decoration.

Julie already exposes useful daemon status via:

- [`src/dashboard/state.rs`](/Users/murphy/source/julie/src/dashboard/state.rs)
- [`src/dashboard/routes/status.rs`](/Users/murphy/source/julie/src/dashboard/routes/status.rs)

This program expands the dashboard into the visible face of the health contract.

The dashboard should surface:

- `Control plane`: daemon state, active sessions, restart-required state, adapter/daemon mismatch events, transport state, stale-binary replacement progress
- `Data plane`: current workspace revision, indexing stage, dirty file count, watcher backlog, failed files, repair queue, Tantivy lag, vector lag
- `Runtime plane`: canonical store health, projection freshness, embedding runtime status, degradation reason, query fallback mode

The dashboard should also answer repairability questions without log spelunking:

- Is Tantivy behind SQLite?
- Are vectors rebuilding?
- Did embeddings fall back from DirectML, CUDA, or MPS to CPU?
- Is watcher processing paused or blocked?
- Is Windows replacement waiting on a drain or transport condition?

First version scope should stay operational and blunt:

- summary cards for global status
- per-workspace health rows
- lag and repair indicators
- recent lifecycle and indexing events
- machine-readable live endpoint that reflects the same state model

## Sequencing

### Phase 1: Contract Reset and Harnesses

This phase lands first and fast.

Goals:

- define shared contracts and state machines
- add reliability harnesses and regression scenarios
- add benchmark harnesses and baseline measurements
- wire the first dashboard-backed health surfaces

Exit criteria:

- named failure scenarios can be run on demand
- every major track has measurable before-state baselines
- health surfaces exist for canonical store, projections, daemon lifecycle, watcher state, and embeddings

### Phase 2: Reliability Core, Track 1 and Track 2

These tracks go first because correctness and reliability win every tie.

Order:

1. control-plane lifecycle surgery
2. indexing-engine unification

Exit criteria for Track 1:

- one lifecycle model backed by tests
- restart behavior follows contract-level rules on Unix and Windows
- adapter retries are driven by lifecycle states, not blind sleeps
- lifecycle authority is no longer split across multiple competing flows

Exit criteria for Track 2:

- initial, catch-up, and watcher-driven indexing share one engine
- primary and reference workspaces use one routing model and one core write path
- file-state transitions are observable and testable
- repair reasons are recorded when broad reindex occurs

### Phase 3: Canonical Storage and Projection Rebuild

This phase begins after indexing state transitions are trustworthy.

Goals:

- redesign canonical write path around revisioned snapshots
- separate canonical persistence from projection and maintenance work
- add projection lag detection and repair scheduling
- remove restart-coupled repair paths

Exit criteria:

- canonical revision state lives in SQLite
- Tantivy and vector projections can rebuild to revision `N` without daemon restart
- projection drift is visible and covered by tests
- bulk write complexity shrinks because the data model carries more of the consistency load

### Phase 4: Embedding Runtime Contract

This phase can overlap with late Phase 3 once the health model exists.

Goals:

- define backend capability, startup, degradation, and fallback rules
- surface structured embedding runtime status through Julie
- make query behavior deterministic when embeddings are unavailable
- isolate backend-specific quirks behind capability checks

Exit criteria:

- sidecar startup and degradation are visible in Julie health
- query behavior is deterministic when embeddings are down
- backend-specific patches and workarounds are isolated and tested

### Phase 5: Convergence and Dogfood Week

This phase cashes the checks.

Goals:

- discard and rebuild generated state as needed
- run the full reliability and benchmark matrix
- dogfood hard across daily agent workflows
- remove compatibility scaffolding and dead repair code made obsolete by the redesign

Exit criteria:

- Julie feels calmer, faster, and less haunted in daily use
- reliability gates are green
- benchmark regressions are understood and acceptable, or fixed

## Success Gates

### Reliability Gates

- deterministic daemon lifecycle
- no known stale-answer regression paths
- no restart-coupled repair dependency for projections
- strong Windows parity for lifecycle and replacement behavior

### Benchmark Gates

- improved or flat search latency
- improved indexing and catch-up throughput
- bounded embedding startup and degradation costs
- no pathological rebuild regressions

### Dogfooding Gates

- lower setup and runtime friction
- fewer edge-case failures in daily agent use
- lower "what state is Julie in?" tax because health is visible

## Non-Goals

- broad new feature work unrelated to system quality
- preserving on-disk generated state for compatibility's sake
- cosmetic refactors that do not improve correctness, reliability, or repairability
- a clean-slate rewrite with no staged delivery and no measurable dogfooding benefit

## Risks

- Contract work can drift into abstract architecture if not tied to harnesses and exit criteria.
- Parallel tracks can re-diverge if Track 0 does not land first and hold the line.
- Storage redesign can sprawl if schema cleanup is not anchored to revisioning and repair goals.
- Windows parity will continue to lag if transport semantics are not pulled behind explicit interfaces.

## Acceptance Criteria Checklist

- [ ] Shared contracts are written and adopted before subsystem surgery begins
- [ ] The dashboard and live health endpoint reflect the same health model used by the runtime
- [ ] Daemon lifecycle is explicit, deterministic, and cross-platform at the contract level
- [ ] Indexing is unified across initial, catch-up, and watcher-driven flows
- [ ] SQLite is the canonical source of truth and projections are rebuildable
- [ ] Tantivy and vectors can lag, fail, and recover without requiring process restart
- [ ] Embedding runtime health and degradation are explicit and query-visible
- [ ] Reliability, benchmark, and dogfooding gates exist for every major phase
- [ ] Generated state can be discarded and rebuilt when that improves architecture clarity

## Recommendation

Proceed with a multi-track world-class systems hardening program built on a short contract reset, then execute control-plane, indexing, storage/projection, and embedding tracks in parallel under shared health and revision contracts.
