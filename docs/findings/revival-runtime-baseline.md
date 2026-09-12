# Revival runtime baseline and after-measurement

## Fixed replay

The baseline binary was `target/release/julie-server` from `86a4101f83105429382862c42dc6d60340b00f8b` (SHA-256 `e3f70c1a3c1d12d044a78190b959ee01024446b043e7f2883ee4db8750561ccc`). The after binary was built from `c3096cf3591f1c694bb8eec8ba5ce9fd13ee262d`, reporting `julie-server 8.0.0` (SHA-256 `022a21e5884d40e436d7dc67eb68b442dafd575490599cd3610f3e6488817841`). The cold checkout was `/home/murphy/source/julie/.worktrees/revival-runtime`; each repetition copied `fixtures/seed/a` into a new `mktemp -d` warm checkout and created its `.git` directory to make an isolated workspace root.

Each Linux release repetition used a private `mktemp -d` `JULIE_HOME`, `JULIE_EMBEDDING_PROVIDER=none`, and `julie-server service --semantics off`. The harness authenticated through that repetition's `service.json`; no semantic sidecar ran. It captured fresh status, opened warm and captured status, opened cold while issuing five concurrent lexical warm `RuntimeFactory` searches, captured status, then alternated warm/cold opens ten times each and captured final status. It recorded service-PID RSS (`ps -o rss=` × 1024), FDs (`/proc/<pid>/fd`), HTTP result bodies, and nanosecond request timings immediately. It stopped and awaited every foreground service before proceeding.

## Baseline

| repetition | peak after 20 opens RSS bytes | FDs | cold wall ms | warm-query p95 ms |
|---|---:|---:|---:|---:|
| 1 | 778272768 | 48 | 12011.865 | 12017.719 |
| 2 | 754442240 | 47 | 12129.876 | 12132.604 |
| 3 | 777187328 | 47 | 12203.795 | 12205.497 |

The baseline budget is 120% of the largest after-20-open observation: 933927322 RSS bytes and 58 FDs. The selected retention policy is at most eight unused runtimes with a 60-second idle expiry.

## After-measurement

All 60 alternating opens, three initial warm opens, three cold opens, 15 concurrent warm searches, and 12 authenticated status requests returned HTTP 200. Each of the three owned service PIDs stopped and was reaped before the next repetition.

| repetition | stage | loaded runtimes | loaded watchers | RSS bytes | FDs |
|---|---|---:|---:|---:|---:|
| 1 | fresh | 0 | 0 | 23588864 | 18 |
| 1 | after warm | 2 | 1 | 46862336 | 30 |
| 1 | after cold | 3 | 2 | 797126656 | 47 |
| 1 | after 20 opens | 3 | 2 | 797179904 | 47 |
| 2 | fresh | 0 | 0 | 24035328 | 18 |
| 2 | after warm | 2 | 1 | 47239168 | 30 |
| 2 | after cold | 3 | 2 | 780722176 | 48 |
| 2 | after 20 opens | 3 | 2 | 780918784 | 48 |
| 3 | fresh | 0 | 0 | 23719936 | 18 |
| 3 | after warm | 2 | 1 | 47267840 | 30 |
| 3 | after cold | 3 | 2 | 756584448 | 49 |
| 3 | after 20 opens | 3 | 2 | 756609024 | 49 |

| repetition | cold wall ms | five concurrent warm-query latencies ms | nearest-rank p95 ms |
|---|---:|---|---:|
| 1 | 12340.159 | `[9.097, 13.182, 15.527, 14.543, 11.798]` | 15.527 |
| 2 | 12130.904 | `[15.727, 18.104, 14.862, 15.424, 14.916]` | 18.104 |
| 3 | 13189.663 | `[14.875, 15.583, 11.767, 15.751, 14.293]` | 15.751 |

For five samples, nearest-rank p95 is the maximum after numeric sort. Peak after-20-open RSS was 797179904 bytes (14.6% below the 933927322-byte budget); peak FDs were 49 (15.5% below the 58-FD budget). Warm-query p95 improved from the 12-second baseline interference to at most 18.104 ms; no hot-checkout regression exceeded the recorded budget.

The status API exposes loaded runtime/watcher counts rather than internal empty-slot counts. This fixed replay stays below the 60-second expiry, so it retained no eligible idle runtime: the final count stayed at three loaded runtimes and two watchers after 20 opens, with no per-open growth. The three runtime entries are the only slots represented by this workload; the eight-unused-slot cap is exercised by deterministic lifecycle tests, not by adding wall-clock delay to the matched replay. Process RSS includes their internal slot metadata, but public status has no per-slot byte exporter.

This is Linux-only release evidence. Windows verification remains deferred to final post-merge validation. Counts cover the service PID only; semantics were off, so no child process was present.
