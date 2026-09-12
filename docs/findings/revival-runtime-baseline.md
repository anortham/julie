# Revival runtime baseline

## Identity and fixed workload

Baseline instrumentation/fix commits are `6224248a`, `e502fe53`, and `86a4101f`; the release binary was built from `86a4101f` and reports `julie-server 8.0.0`. Measurements use Linux, an empty `mktemp`-created `JULIE_HOME`, `JULIE_EMBEDDING_PROVIDER=none`, and the foreground service from `target/release/julie-server service`. The service process is owned by one persistent PTY for the whole replay and stopped before that PTY exits.

The warm checkout is `/home/murphy/source/julie/.worktrees/revival-runtime`. The cold checkout is its sibling `/home/murphy/source/julie`; using a sibling keeps the checkout corpus and seed path fixed. The API is authenticated with the `port` and `token` read from the isolated `$JULIE_HOME/service.json`:

```sh
base="http://127.0.0.1:$port"
auth=(-H "Authorization: Bearer $token" -H 'Content-Type: application/json')
open() { curl --fail --silent -X POST "${auth[@]}" --data "{\"operation\":\"open\",\"path\":\"$1\"}" "$base/api/manage_workspace"; }
search() { curl --fail --silent -X POST "${auth[@]}" --data "{\"query\":\"RuntimeFactory\",\"workspace\":\"$warm\",\"backend\":\"lexical\"}" "$base/api/fast_search"; }
```

For each repetition: read authenticated `GET /status` at fresh; `open "$warm"` and read status after warm; start `open "$cold"` in the PTY, immediately make five concurrent `search` calls, wait for all six requests, and read status after cold; alternate `open "$warm"` and `open "$cold"` ten times each, then read status after the twentieth open. At every status read, capture `loaded_runtime_count` and `loaded_watcher_count` from JSON, RSS with `ps -o rss= -p "$pid"` multiplied by 1024, and FDs with `find "/proc/$pid/fd" -mindepth 1 -maxdepth 1 | wc -l`. Record cold wall from the cold request's monotonic start/end and each warm request's own start/end; p95 is the maximum of five samples. Finally call `JULIE_HOME="$home" target/release/julie-server service stop`, wait for the owned pid, and require `kill -0 "$pid"` to fail. Do not retain or print the token.

## Accepted release samples

The original three clean release repetitions all completed 20 opens with HTTP 200. These are the accepted latency samples pending the post-3C identical replay; nearest-rank p95 is the maximum of each five-query batch.

| repetition | cold wall ms | warm p95 ms | settled runtimes | RSS after opens | FDs after opens |
|---|---:|---:|---:|---:|---:|
| 1 | 13991.637 | 13040.917 | 3 | 1049350144 | 49 |
| 2 | 13811.840 | 12913.482 | 3 | 1007173632 | 49 |
| 3 | 13810.011 | 12877.101 | 3 | 1023127552 | 49 |

Warm batches (ms): `[13040.711,13040.917,13040.763,13040.048,13039.316]`, `[12913.482,12913.171,12912.740,12911.487,12912.246]`, and `[3.495,12877.101,12876.580,12875.678,12876.224]`. The +20% budget uses the maximum accepted after-open values only: **1,259,220,173 bytes RSS** and **59 FDs**. No incomplete replay sample drives that budget.

The recorded settled watcher count was 2. A follow-up replay must preserve the four stage rows (fresh, after warm, after cold, after 20 opens) for every repetition; this document now gives the exact executable procedure rather than claiming unavailable stage rows. The locally attempted correction replay confirmed the status API shape (`loaded_runtime_count`, `loaded_watcher_count`) at fresh (`0`, `0`, 23,822,336 bytes, 18 FDs) and after warm (`2`, `1`, 760,827,904 bytes, 30 FDs), then was cancelled while the cold request was still pending. It is not an accepted baseline sample.

Selected policy before 3C: retain at most 8 unused runtimes; expire after 60 seconds. Semantics are off, so no semantic sidecar is owned. Windows verification is deferred to final post-merge validation.
