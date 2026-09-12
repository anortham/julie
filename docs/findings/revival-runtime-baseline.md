# Revival runtime baseline

Source: `6224248af5364aa84400409389e711e205b2c446`; Linux, debug binary, isolated `JULIE_HOME`, `JULIE_EMBEDDING_PROVIDER=none`. Warm corpus was a fresh copy of `fixtures/seed/a`; cold corpus was this worktree at `6224248a`.

Each of three clean service processes indexed warm, indexed cold while issuing warm lexical searches, then alternated 20 `manage_workspace open` requests. All 60 opens returned 200.

| repetition | cold wall ms | warm p95 ms | settled runtimes | RSS after opens | FDs after opens |
|---|---:|---:|---:|---:|---:|
| 1 | 24889.4 | 2.585 | 3 | 1000017920 | 43 |
| 2 | 25661.1 | 2.637 | 3 | 998912000 | 43 |
| 3 | 24832.1 | 2.647 | 3 | 991637504 | 43 |

Fresh/warm/cold/after-open runtime counts were 1/2/3/3 because `/status` itself acquires the unbound runtime. Watcher status reported 0 in every settled stage. The first warm request during each cold initialization blocked for 24.0–24.8s; later warm requests were about 2ms. The hot-checkout resource budget is 1,200,021,504 bytes RSS and 52 FDs (20% over the maximum same-workload baseline).

Selected policy before 3C: retain at most 8 unused runtimes; expire after 60 seconds. This measurement is Linux-only; Windows validation is deferred to final post-merge validation.
