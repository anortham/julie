# Revival runtime baseline

Source: `86a4101f`; Linux release binary (`target/release/julie-server --version`: `julie-server 8.0.0`), isolated `JULIE_HOME`, `JULIE_EMBEDDING_PROVIDER=none`.

Each of three clean service processes indexed warm, indexed cold while issuing warm lexical searches, then alternated 20 `manage_workspace open` requests. All 60 opens returned 200.

| repetition | cold wall ms | warm p95 ms | settled runtimes | RSS after opens | FDs after opens |
|---|---:|---:|---:|---:|---:|
| 1 | 13991.637 | 13040.917 | 3 | 1049350144 | 49 |
| 2 | 13811.840 | 12913.482 | 3 | 1007173632 | 49 |
| 3 | 13810.011 | 12877.101 | 3 | 1023127552 | 49 |

Raw warm batches (ms): `[13040.711,13040.917,13040.763,13040.048,13039.316]`, `[12913.482,12913.171,12912.740,12911.487,12912.246]`, `[3.495,12877.101,12876.580,12875.678,12876.224]`; nearest-rank p95 is the maximum per batch. Direct settled watcher count was 2; all 60 opens returned 200 and services were stopped before temp-root deletion. The hot-checkout budget is 1,259,220,173 bytes RSS and 59 FDs (120% maxima).

Selected policy before 3C: retain at most 8 unused runtimes; expire after 60 seconds. This measurement is Linux-only; Windows validation is deferred to final post-merge validation.
