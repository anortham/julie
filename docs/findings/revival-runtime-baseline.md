# Revival runtime baseline

## Identity and fixed replay

The release executable is `target/release/julie-server`, built from `86a4101f83105429382862c42dc6d60340b00f8b` and reporting `julie-server 8.0.0`; SHA-256: `e3f70c1a3c1d12d044a78190b959ee01024446b043e7f2883ee4db8750561ccc`. The cold checkout is `/home/murphy/source/julie/.worktrees/revival-runtime` at `b499ee33c21159c073fd396f787eef224451bb35`. Its diff from the binary source contains only the finding, plan, and memory documents, so the measured Rust source is `86a4101f`.

Every Linux repetition used a new `mktemp -d` `JULIE_HOME`, `JULIE_EMBEDDING_PROVIDER=none`, and `julie-server service --semantics off` in one foreground PTY. No semantic sidecar was started. The warm checkout was a new `mktemp -d` directory made for each repetition with:

```sh
cp -a /home/murphy/source/julie/.worktrees/revival-runtime/fixtures/seed/a/. "$warm/"
mkdir "$warm/.git"
```

The `.git` directory makes the copied seed an isolated workspace root. The cold checkout remained the larger worktree named above. The service token and port came from that repetition's private `$JULIE_HOME/service.json` and were not retained.

```sh
base="http://127.0.0.1:$port"
auth=(-H "Authorization: Bearer $token" -H 'Content-Type: application/json')
curl --silent --show-error --output response.json --write-out '%{http_code}' "${auth[@]}" "$base/status"
curl --silent --show-error --output response.json --write-out '%{http_code}' -X POST "${auth[@]}" \
  --data "{\"operation\":\"open\",\"path\":\"$path\"}" "$base/api/manage_workspace"
curl --silent --show-error --output response.json --write-out '%{http_code}' -X POST "${auth[@]}" \
  --data "{\"query\":\"RuntimeFactory\",\"workspace\":\"$warm\",\"backend\":\"lexical\"}" "$base/api/fast_search"
```

For each of three repetitions: capture authenticated status at fresh; open warm and capture status; start cold open, immediately issue exactly five concurrent lexical warm searches, await all six, and capture status; alternate warm/cold opens ten times each, then capture status after the twentieth alternating open. Each status sample records the returned `loaded_runtime_count` and `loaded_watcher_count`, `ps -o rss=` multiplied by 1024, and `find /proc/$pid/fd -mindepth 1 -maxdepth 1 | wc -l`. Cold and warm-request wall times use `date +%s%N`. Each status, request body, HTTP result, and latency was written immediately to the repetition directory. Finally `JULIE_HOME="$home" julie-server service stop` was issued and the foreground PID was awaited until `kill -0` failed.

## Results

All 60 alternating opens, all three initial warm opens, all three cold opens, all fifteen warm searches, and all twelve authenticated status calls returned HTTP 200. Every owned service exited after its `service stop` request.

| repetition | stage | runtimes | watchers | RSS bytes | FDs |
|---|---|---:|---:|---:|---:|
| 1 | fresh | 0 | 0 | 23642112 | 18 |
| 1 | after warm | 2 | 1 | 47001600 | 30 |
| 1 | after cold | 3 | 2 | 778084352 | 48 |
| 1 | after 20 opens | 3 | 2 | 778272768 | 48 |
| 2 | fresh | 0 | 0 | 23629824 | 18 |
| 2 | after warm | 2 | 1 | 46825472 | 30 |
| 2 | after cold | 3 | 2 | 754307072 | 47 |
| 2 | after 20 opens | 3 | 2 | 754442240 | 47 |
| 3 | fresh | 0 | 0 | 23695360 | 18 |
| 3 | after warm | 2 | 1 | 46948352 | 30 |
| 3 | after cold | 3 | 2 | 777142272 | 47 |
| 3 | after 20 opens | 3 | 2 | 777187328 | 47 |

| repetition | cold wall ms | five concurrent warm-query latencies ms | nearest-rank p95 ms |
|---|---:|---|---:|
| 1 | 12011.865 | `[12016.416, 12017.719, 12017.599, 12016.546, 12016.783]` | 12017.719 |
| 2 | 12129.876 | `[12132.604, 12132.054, 12132.072, 12.234, 11.516]` | 12132.604 |
| 3 | 12203.795 | `[12203.223, 12205.497, 12205.371, 10.782, 11.106]` | 12205.497 |

For five samples, nearest-rank p95 is rank `ceil(0.95 * 5) = 5`: the maximum after numeric sort. The hot-checkout resource budget is 120% of the largest after-20-opens measurement: `ceil(778272768 * 1.20) = 933927322` RSS bytes and `ceil(48 * 1.20) = 58` FDs. The selected 3C retention policy is at most eight unused runtimes, expiring after 60 seconds.

The concurrent-search samples show the current global initialization interference and are a baseline constraint, not a latency target. This is Linux-only release evidence; Windows verification remains deferred to final post-merge validation. Counts cover the service PID's RSS and FDs, not child-process-tree accounting; semantics were off, so no semantic child existed.
