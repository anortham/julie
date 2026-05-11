# Best-in-Class Tree-Sitter — Live Dogfood Handoff

The tree-sitter quality run completed offline release gates and daemon-mode
live dogfood through evidence commit `88998e69`:

- `fixtures/extraction/capabilities.json` has 0 open gaps.
- `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` is release-profile evidence with 22
  verified repos, including VB.NET `samples`, 0 skipped repos, and 0 hard
  failures.
- `fixtures/extraction/tree-sitter-real-world-corpus.toml` has 110
  representative specs, 5 per release-profile repo.
- `cargo doc -p julie-extractors --no-deps` is warning-free.
- `docs/LANGUAGE_CERTIFICATION_REPORT.md` is current.

The branch-level offline gates passed and are recorded in
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

## Daemon-Mode Live Dogfood

The live dogfood rows are now recorded in both verification ledgers. Evidence
was collected against the running daemon through `julie-server tool ...` because
the Codex-hosted `mcp__julie__` connector still returned `Transport closed`:

- `manage_workspace health` returned READY/FULLY READY with SQLite healthy,
  projection current at 409/409, 46,843 symbols, 56,538 relationships, and the
  sidecar embeddings provider initialized on MPS.
- `call_path extract_symbols_static extract_canonical` found the one-hop edge
  `extract_symbols_static --call--> extract_canonical`.
- `fast_refs extract_canonical` found the definition plus visible callers,
  reporting 21 total references.
- SQLite `index_engine_state` recorded
  `extractors=2026-05-10.tree-sitter-best-in-class-v1+schema=2026-05-05.reference-identifier-v3`
  for `semantic_index_engine`.
- `manage_workspace refresh workspace_id=best-in-class-treesitter_2ad7e041`
  returned already up-to-date at canonical revision 409.

The direct Codex MCP connector issue is still unresolved. Current evidence
proves the daemon HTTP data plane and index state, not Codex's in-process MCP
transport.

## Remaining Steps

1. Decide whether the direct Codex `mcp__julie__` connector must be fixed before
   integration, or track it as a separate harness/adapter issue.
2. Merge `.worktrees/best-in-class-treesitter/` back to `main` with a merge
   commit, or open a PR. Do not rebase; the verification ledger cites commit
   SHAs.
