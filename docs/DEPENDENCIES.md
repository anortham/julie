# Dependencies Management

**Last Updated:** 2026-05-05

## Tree-Sitter Dependencies

Tree-sitter parser output is a data contract for Julie. Parser dependencies are no longer frozen, but they are not casual upgrades either. Any change to `tree-sitter`, a parser crate version, or a parser git revision must go through the parser-upgrade gate.

Current baseline:

- `tree-sitter = "0.26.8"`
- Parser crate inventory and decisions live in `docs/TREE_SITTER_UPGRADES.md`
- Parser metadata for each registry language lives in `crates/julie-extractors/src/language_spec.rs`
- Capability and fixture coverage live in `fixtures/extraction/capabilities.json`

Required parser-change workflow:

1. Verify the current upstream crate or git revision before editing manifests.
2. Update `docs/TREE_SITTER_UPGRADES.md` with the decision and evidence.
3. Update `LanguageSpec` and `fixtures/extraction/capabilities.json` when parser crate names or statuses change.
4. Run narrow failing tests first for any grammar drift.
5. Run `cargo xtask test bucket parser-upgrade`.

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
