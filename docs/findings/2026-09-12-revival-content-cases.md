# Revival content retrieval cases

`fixtures/search-quality/search-matrix-cases.toml` includes nine `revival_scoped_content` rows for Plan 5. They cover Markdown, TOML configuration, and a Rust source comment. Each query runs through the product tool route with the backend omitted, then with explicit lexical and semantic backends.

The checked-in corpus expands `~/source`. A safe run uses a temporary home and a clean detached clone at `~/source/julie`, so concurrent edits cannot change the corpus. Build candidate binaries in the task worktree first. Copy the already prepared model into a temporary writable cache, start an isolated service, index the clone through its JSON API, and run the evaluator with the temporary `HOME` and `JULIE_HOME`. This leaves the user's service, registry, indexes, and model cache untouched.

```bash
task_root=$PWD
shared_target=/home/murphy/source/julie/target
sidecar_program=/home/murphy/source/julie-semantic-sidecar/target/release/julie-semantic-sidecar
prepared_model=/home/murphy/.cache/julie-semantic/bge-small-en-v1.5-f32.gguf
candidate_sha=$(git rev-parse HEAD)
matrix_home=$(mktemp -d)
mkdir -p "$matrix_home/source" "$matrix_home/cache"
git clone --shared --no-checkout "$task_root" "$matrix_home/source/julie"
git -C "$matrix_home/source/julie" checkout --detach "$candidate_sha"
cp --reflink=auto "$prepared_model" "$matrix_home/cache/bge-small-en-v1.5-f32.gguf"
env HOME="$matrix_home" JULIE_HOME="$matrix_home/julie-home" JULIE_EMBEDDING_PROVIDER=native JULIE_EMBEDDING_CACHE_DIR="$matrix_home/cache" JULIE_NATIVE_SIDECAR_PROGRAM="$sidecar_program" "$shared_target/debug/julie-server" service >"$matrix_home/service.log" 2>&1 &
service_pid=$!
trap 'env HOME="$matrix_home" JULIE_HOME="$matrix_home/julie-home" "$shared_target/debug/julie-server" service stop >/dev/null 2>&1 || kill "$service_pid" 2>/dev/null || true' EXIT
python3 - "$matrix_home/julie-home/service.json" "$matrix_home/source/julie" <<'PY'
import json, pathlib, sys, time, urllib.request

record_path = pathlib.Path(sys.argv[1])
for _ in range(200):
    if record_path.exists():
        break
    time.sleep(0.05)
else:
    raise SystemExit("isolated Julie service did not publish service.json")
record = json.loads(record_path.read_text())
request = urllib.request.Request(
    f"http://127.0.0.1:{record['port']}/api/manage_workspace",
    data=json.dumps({"operation": "index", "path": sys.argv[2], "force": True}).encode(),
    headers={"Authorization": f"Bearer {record['token']}", "Content-Type": "application/json"},
    method="POST",
)
with urllib.request.urlopen(request, timeout=300) as response:
    response.read()
PY
env HOME="$matrix_home" JULIE_HOME="$matrix_home/julie-home" JULIE_EMBEDDING_PROVIDER=native JULIE_EMBEDDING_CACHE_DIR="$matrix_home/cache" JULIE_NATIVE_SIDECAR_PROGRAM="$sidecar_program" "$shared_target/debug/xtask-eval" search-matrix baseline --profile full --out "$shared_target/revival-content-baseline.json"
env HOME="$matrix_home" JULIE_HOME="$matrix_home/julie-home" "$shared_target/debug/julie-server" service stop
```

The evaluator process runs each trio against one handler and unchanged index state. Compare the nine `revival_scoped_content` rows in the JSON. The omitted backend passes when `expected_value` appears on page one. Explicit lexical and semantic remain controls for content and symbol retrieval respectively. Missing full-profile repositories are reported as skipped and do not invalidate the Julie rows.

For before-and-after evidence, build a second evaluator from pre-change commit `b60f7d8e3ca6dbdf921dcac87de3b74c8e06bc2d` with only the matrix adapter, fixture, and current `facts.sqlite` readiness check overlaid. Run it before the candidate evaluator against the same stopped-service index, temporary model cache, and detached corpus. This measures the old and new product `FastSearchTool` paths under identical readiness; the explicit lexical and semantic rows remain controls. The overlay is measurement-only and is never committed to the product branch.
