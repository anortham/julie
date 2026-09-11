# Head-to-head retrieval matrix (Julie vs Miller)

Retrieval only: search and symbol-inspect rows on the ten public repos in `../semantic-value/scorecard.toml`. Agent episodes and the Rust `xtask-eval revival` harness are out of scope.

## Run

From the Julie repo root:

```bash
python3 docs/eval/head-to-head/run_matrix.py --self-check
python3 docs/eval/head-to-head/run_matrix.py --validate
python3 docs/eval/head-to-head/run_matrix.py --require-semantics
```

Binaries default to `target/release/julie-server` and `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller`. Override with `--julie-bin` and `--miller-bin`. Filter with `--repos` and `--tasks`. Skip a product with `--skip-julie` or `--skip-miller`. `--require-semantics` aborts when any opened Julie repo has zero rows in `vectors`.

The runner writes `results/<UTC timestamp>.json` and `results/<UTC timestamp>.md`. Before the kept run it records Julie vector coverage from `$JULIE_HOME/indexes/<workspace_id>/facts.sqlite` (read-only `count(*)` on `vectors` and `symbols`) and each Julie row's `readiness` field. Run twice. Keep the second pair; the first run warms both indexes.

## Scoring

- Search rows (`retrieval.concept`, `retrieval.implementation`): `path_top5`. The expected file must appear in the first five ranked paths.
- Inspect rows (`inspect.symbol`): `path_top`. The first ranked path must be the expected file.
- Julie search is `fast_search` with `limit=5` (product compact default). Miller search is `search` with `limit=5 format=json mode=auto`.
- Inspect targets are the first top-level symbol `julie-server symbols <expected path> --json` lists whose kind is not `import`, `package`, `namespace`, `module`, or `using`. Search output is never used to pick a target.

## Frozen contract this run honors

1. Cases were written from `scorecard.toml` and from `julie-server symbols` on each expected file, before any candidate search or inspect output was viewed.
2. The manifest and results live under `docs/eval/`. Neither product indexes that tree as a corpus. The corpus roots are the ten external repos pinned in `cases.json`.
