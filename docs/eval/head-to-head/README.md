# Head-to-head retrieval matrix (Julie vs Miller)

Retrieval only: search and symbol-inspect rows on the ten public repos in `../semantic-value/scorecard.toml`. Agent episodes and the Rust `xtask-eval revival` harness are out of scope.

## Run

From the Julie repo root:

```bash
python3 docs/eval/head-to-head/run_matrix.py --self-check
python3 docs/eval/head-to-head/run_matrix.py --validate
python3 docs/eval/head-to-head/run_matrix.py --require-semantics
python3 docs/eval/head-to-head/run_matrix.py --julie-backends auto,hybrid,semantic --require-semantics
```

Binaries default to `target/release/julie-server` and `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller`. Override with `--julie-bin` and `--miller-bin`. Filter with `--repos` and `--tasks`. Skip a product with `--skip-julie` or `--skip-miller`. `--require-semantics` aborts when any opened Julie repo has zero rows in `vectors`. `--julie-backends` is a comma list (`auto`, `hybrid`, `semantic`). `auto` omits `backend=`; the others add `backend=<name>` on Julie search rows and label them `<id>.<backend>`.

The runner writes `results/<UTC timestamp>.json` and `results/<UTC timestamp>.md`. Stdio MCP replies carry no `readiness` object (the JSON API does), so Julie semantic coverage stays sqlite-based: read-only `count(*)` on `vectors` and `symbols` in `$JULIE_HOME/indexes/<workspace_id>/facts.sqlite`. Run twice when indexes are cold. Keep the second pair.

## Scoring

- Search rows (`retrieval.concept`, `retrieval.implementation`): `path_top5`. The expected file must appear in the first five ranked paths.
- Inspect rows (`inspect.symbol`): `path_top`. The first ranked path must be the expected file.
- Julie search is `fast_search` with `limit=5` (product compact default). Miller search is `search` with `limit=5 format=json mode=auto`.
- Inspect targets are the first top-level symbol `julie-server symbols <expected path> --json` lists whose kind is not `import`, `package`, `namespace`, `module`, or `using`. Search output is never used to pick a target.

## Frozen contract this run honors

1. Cases were written from `scorecard.toml` and from `julie-server symbols` on each expected file, before any candidate search or inspect output was viewed.
2. The manifest and results live under `docs/eval/`. Neither product indexes that tree as a corpus. The corpus roots are the ten external repos pinned in `cases.json`.
