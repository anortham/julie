# Best-in-Class Tree-Sitter — Live Dogfood Handoff

The autonomous run completed the offline tree-sitter quality work through the
release-gate sweep at worktree HEAD `235bd37c`:

- `fixtures/extraction/capabilities.json` has 0 open gaps.
- `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` is release-profile evidence with 22
  verified repos, including VB.NET `samples`, 0 skipped repos, and 0 hard
  failures.
- `fixtures/extraction/tree-sitter-real-world-corpus.toml` has 110
  representative specs, 5 per release-profile repo.
- `cargo doc -p julie-extractors --no-deps` is warning-free.
- `docs/LANGUAGE_CERTIFICATION_REPORT.md` is current.

The branch-level offline gates passed at `235bd37c` and are recorded in
`docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md` plus
`docs/TREE_SITTER_QUALITY_BAR.md`:

- `cargo fmt --check`
- `git diff --check`
- `cargo xtask certify tree-sitter --check`
- `cargo xtask test bucket extractors`
- `cargo xtask test bucket parser-upgrade`
- `cargo xtask test changed`
- `cargo xtask test dev`
- `cargo xtask test system`
- `cargo xtask test dogfood`
- `cargo xtask test full`
- `cargo build --release`
- `cargo build --examples -p julie-extractors`
- `cargo test -p julie-extractors --doc`
- `cargo doc -p julie-extractors --no-deps`
- `cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate`

## Pending Live MCP Dogfood

Live MCP dogfood is still pending. The Codex session could not complete it
because the `mcp__julie__` transport returned `Transport closed`. The release
binary CLI did prove two equivalent data-plane checks:

- `./target/release/julie-server --workspace . --json call-path extract_symbols_static extract_canonical --max-hops 6`
  found a one-hop call edge from `extract_symbols_static` to `extract_canonical`.
- `./target/release/julie-server --workspace . --json refs extract_canonical -n 20`
  found the definition plus 20 visible references.

That CLI evidence is not a substitute for the live MCP rows below.

## User Sign-Off Steps

1. `cargo build --release`
2. Restart Claude Code or the MCP client so it respawns the new server.
3. In the Julie workspace, run through the MCP client:
   - `manage_workspace health` — expect READY status with nonzero symbol and
     relationship counts.
   - `call_path extract_symbols_static extract_canonical` — expect a one-hop
     edge through the canonical pipeline.
   - `fast_refs extract_canonical` — expect definition + references including
     public API projection and real-world contract callers.
   - SQLite check: inspect the on-disk index metadata for the engine-version
     column actually written by the indexer. Verify the column name against
     `src/database/schema.rs`; value must contain
     `2026-05-10.tree-sitter-best-in-class-v1` via
     `julie_extractors::EXTRACTION_CONTRACT_VERSION`, composed into
     `SEMANTIC_INDEX_ENGINE_VERSION` at
     `src/tools/workspace/indexing/engine_version.rs`.
   - `manage_workspace refresh workspace_id=julie_<id>` — expect
     already-up-to-date behavior without a full reindex.
4. Append rows to both verification ledgers:
   - `docs/TREE_SITTER_QUALITY_BAR.md`
   - `docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md`
5. Merge `.worktrees/best-in-class-treesitter/` back to `main` with a merge
   commit. Do not rebase; the verification ledger cites commit SHAs.
