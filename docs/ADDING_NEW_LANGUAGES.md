# Adding New Language Support

Language and parser work — new extractors, parser upgrades, golden fixtures, capability tests — happens in the external repo:

**[`anortham/julie-extractors`](https://github.com/anortham/julie-extractors)**

The extractors are consumed here as a pinned git dependency. To pick up a new language or parser release:

1. Release a new tag in `anortham/julie-extractors`.
2. Re-pin the `julie-extractors` git-dep in `Cargo.toml` to the new tag.
3. Update `SEMANTIC_INDEX_ENGINE_VERSION` in `src/tools/workspace/indexing/engine_version.rs` to match the tag's `EXTRACTION_CONTRACT_VERSION`. The changed contract string forces a one-time reindex on all workspaces, and the `engine_version` regression test enforces that the version string is kept in sync.
4. After re-pinning, add a row to `src/tests/integration/real_world_contract.rs` for any new language that has a fixture under `fixtures/real-world/`.

See `docs/TREE_SITTER_UPGRADES.md` for the extractor dependency integration gate (`cargo xtask test bucket extractor-dep-integration`) required for any parser version or git-rev change.

**Last Updated**: 2026-07-14
