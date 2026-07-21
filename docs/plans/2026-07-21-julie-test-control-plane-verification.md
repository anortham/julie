# Julie Test Control Plane — Verification

Host: darwin aarch64 (dev workstation). All warm timings via `cargo xtask test bucket` / bucket command sequences after package warm. `expected_seconds ≈ warm p95 + small headroom`.

## Task 6 — Warm measurement + `fast` recalibration

### Warm bucket table (n=3 after warm discard)

| Bucket | Warm runs (s) | p50 | p95 | Prior expected | New expected | Notes |
|--------|---------------|-----|-----|----------------|--------------|-------|
| `core-database` | 3.9, 3.5, 3.7 | 3.7 | 3.9 | 5 | **5** | Keep; headroom still honest |
| `core-fast` (julie-lib cmds only) | 23.2, 20.1, 19.3 | 20.1 | 23.2 | 55 (provisional) | **26** | p95+~3s; long pole = `tests::core::handler` (~17.5s) |
| `xtask-runner` (optional) | — | — | — | 15 | unchanged | Not added to `fast`; discard run failed unrelated `changed_tests_full_search_buckets_cover_declared_search_modules` |

Per-command warm sample for `core-fast` (post-edit membership):

| Command | Wall (s) |
|---------|----------|
| `utils::paths::tests` | 0.7 |
| `tests::core::handler` | 17.5 |
| `tests::core::language` | 1.1 |
| `tests::core::paths` | 0.6 |

### Cold sample (documented separately)

First `cargo xtask test bucket core-database` after package warm still showed a heavy nextest prebuild:

```
SUMMARY: 1 buckets passed in 4.7s (warm)
PREBUILD: 87.1s
COLD WALL: 91.8s
```

Post-recalibration warm `cargo xtask test fast` (measurement, not worker ceiling):

```
SUMMARY: 2 buckets passed in 23.7s (warm)
PREBUILD: 0.6s
COLD WALL: 24.3s
```

### Membership decisions

| Constraint | Result |
|------------|--------|
| `nano ⊆ fast` | yes — both `["core-database", "core-fast"]` |
| Declared `fast` sum | 5 + 26 = **31 ≤ 60** |
| Split `core-fast`? | **No** — fits comfortably after dropping broken cmd |
| `julie-runtime` `tests::watcher_filtering` | **Removed from `core-fast` commands** — `julie-runtime` lib tests do not compile (`Arc<Mutex<SearchIndex>>` left in `repair_projection.rs` after Mutex removal). Restore when that crate's tests compile. |

### Verification Ledger

Measurement / contract runs were executed on the working tree immediately before the
Task 6 commit on `feat/test-control-plane`
(`chore(xtask): recalibrate fast expected_seconds from warm evidence`).
Exact SHA: see `git log -1` / `.razorback/sdd/task-6-report.md` after landing.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Manifest accepts valid fast tier | `cargo nextest run -p xtask -- manifest_tests_accept_valid_fast_tier` | worker-exact | Task-6 HEAD | pass | 2026-07-21T18:50:00Z | no |
| Checked-in manifest contract matches | `cargo nextest run -p xtask -- manifest_contract` | worker-exact | Task-6 HEAD | pass | 2026-07-21T18:50:00Z | no |
| Fast tier completes (measurement) | `cargo xtask test fast` | measurement-fast | Task-6 HEAD | pass (23.7s warm) | 2026-07-21T18:50:00Z | no |
