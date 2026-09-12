# Revival content retrieval cases

`fixtures/search-quality/search-matrix-cases.toml` includes nine `revival_scoped_content` rows for Plan 5. They cover Markdown, TOML configuration, and a Rust source comment. Each query runs through the product tool route with the backend omitted, then with explicit lexical and semantic backends.

The checked-in corpus expands `~/source`, so a safe candidate run uses a temporary home with a `source/julie` symlink to the current worktree. Build the candidate binaries with the normal home first. Then start an isolated service, index the symlinked checkout through its JSON API, and run the already-built evaluator with the temporary `HOME` and `JULIE_HOME`. This leaves the user's service, registry, and indexes untouched.

```bash
cargo build -p julie -p xtask-eval
sidecar_program=/absolute/path/to/julie-semantic-sidecar
matrix_home=$(mktemp -d)
mkdir -p "$matrix_home/source"
ln -s "$PWD" "$matrix_home/source/julie"
env HOME="$matrix_home" JULIE_HOME="$matrix_home/julie-home" JULIE_EMBEDDING_PROVIDER=native JULIE_NATIVE_SIDECAR_PROGRAM="$sidecar_program" "$PWD/target/debug/julie-server" service >"$matrix_home/service.log" 2>&1 &
python3 - "$matrix_home/julie-home/service.json" "$matrix_home/source/julie" <<'PY'
import json, pathlib, sys, time, urllib.request

record_path = pathlib.Path(sys.argv[1])
for _ in range(200):
    if record_path.exists():
        break
    time.sleep(0.05)
record = json.loads(record_path.read_text())
request = urllib.request.Request(
    f"http://127.0.0.1:{record['port']}/api/manage_workspace",
    data=json.dumps({"arguments": {"operation": "index", "path": sys.argv[2], "force": True}}).encode(),
    headers={"Authorization": f"Bearer {record['token']}", "Content-Type": "application/json"},
    method="POST",
)
with urllib.request.urlopen(request, timeout=300) as response:
    response.read()
PY
env HOME="$matrix_home" JULIE_HOME="$matrix_home/julie-home" JULIE_EMBEDDING_PROVIDER=native JULIE_NATIVE_SIDECAR_PROGRAM="$sidecar_program" "$PWD/target/debug/xtask-eval" search-matrix baseline --profile full --out "$PWD/target/revival-content-baseline.json"
env HOME="$matrix_home" JULIE_HOME="$matrix_home/julie-home" "$PWD/target/debug/julie-server" service stop
```

The evaluator process runs each trio against one handler and unchanged index state. Compare the nine `revival_scoped_content` rows in the JSON. The omitted backend passes when `expected_value` appears on page one. Explicit lexical and semantic remain controls for content and symbol retrieval respectively. Missing full-profile repositories are reported as skipped and do not invalidate the Julie rows.
