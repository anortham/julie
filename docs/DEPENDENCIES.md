# Dependencies Management

**Last Updated:** 2026-05-05

## Tree-Sitter Dependencies

Tree-sitter parser output is a data contract for Julie. Parser dependencies are no longer frozen, but they are not casual upgrades either. Any change to `tree-sitter`, a parser crate version, or a parser git revision must go through the parser-upgrade gate.

Current baseline:

- `tree-sitter = "0.26.8"` (core ABI)
- `julie-extractors` is consumed as a pinned git dependency from [`anortham/julie-extractors`](https://github.com/anortham/julie-extractors)
- Parser crate inventory and decisions live in `docs/TREE_SITTER_UPGRADES.md` (for context) and in the external repo's `Cargo.toml`
- Parser metadata (`language_spec.rs`), capability coverage (`capabilities.json`), and golden fixtures all live in the external `anortham/julie-extractors` repo

Required parser-change workflow:

1. Make the upgrade in the external `anortham/julie-extractors` repo; release a new tag there.
2. Re-pin the `julie-extractors` git-dep in julie's `Cargo.toml` to the new tag.
3. Sync `SEMANTIC_INDEX_ENGINE_VERSION` in `src/tools/workspace/indexing/engine_version.rs` to match the tag's `EXTRACTION_CONTRACT_VERSION`.
4. Update `docs/TREE_SITTER_UPGRADES.md` with the decision and evidence (summary of what changed upstream).
5. Run `cargo xtask test bucket parser-upgrade` in this repo.

Git parser dependencies must be pinned with `rev`. Floating branch dependencies are not acceptable for parser infrastructure.

## Adding New Dependencies

Before adding any dependency:

1. Verify the version on crates.io or the upstream source.
2. Check current documentation for API or platform requirements.
3. Confirm it does not break single-binary deployment.
4. Confirm it does not require unavailable external libraries.
5. Check cross-platform compatibility.
6. Consider startup time and binary size impact.

Examples:

- Before changing `tokio`, verify the current version and migration notes.
- Before changing `tree-sitter-*`, update the parser ledger and run the parser-upgrade bucket.
