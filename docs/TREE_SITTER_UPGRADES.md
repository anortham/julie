# Tree-Sitter Upgrade Ledger

Updated: 2026-07-14

Julie treats parser output as a data contract. A tree-sitter dependency upgrade is accepted only when every registry language still passes the production-path golden gate and the real-world parser contract.

## Sources

- `https://crates.io/api/v1/crates/<crate>` for current crates.io max versions.
- `cargo metadata --format-version 1` for locked versions and git revisions.
- `cargo tree -p julie-extractors -e normal` for the resolved parser graph (run from the `anortham/julie-extractors` checkout).
- docs.rs for `tree-sitter 0.26.11` ABI constants: `LANGUAGE_VERSION = 15`, `MIN_COMPATIBLE_LANGUAGE_VERSION = 13`.

## Upgrade Rules

1. Parser crates and golden fixtures are managed in the external [`anortham/julie-extractors`](https://github.com/anortham/julie-extractors) repo. Make upgrades there; release a new tag; then re-pin julie's git-dep.
2. After re-pinning here, run `cargo xtask test bucket extractor-dep-integration` for any tree-sitter core, parser crate, parser git revision, or expected-output change caused by grammar drift.
3. If a parser crate cannot be updated, record the blocker here with the failing command or dependency resolver error.
4. Git parser dependencies must be pinned by `rev` in the external `julie-extractors` repo. Floating branch dependencies are not acceptable for parser infrastructure.
5. Expected-output changes are acceptable only when they match reviewed parser behavior. Do not erase symbols, relationships, identifiers, types, or diagnostics just to make the gate pass.

## Core ABI Decision

`tree-sitter` moved from `0.25.10` to `0.26.8`, then to `0.26.11` with `julie-extractors v2.14.0`. Julie and the extractor crate use the same `0.26.11` runtime because Cargo permits only one package with `links = "tree-sitter"` in the dependency graph.

The core crate supports parser ABI versions `13..=15`, based on docs.rs constants for `MIN_COMPATIBLE_LANGUAGE_VERSION` and `LANGUAGE_VERSION`. The upgrade still required code changes because the Rust binding API changed some child-index methods to `u32`. Julie now casts child indices at call sites that walk children by index.

The initial `0.26.8` trial failed because `harper-tree-sitter-dart 0.0.5` required `tree-sitter ^0.25.6`, and Cargo forbids two packages with `links = "tree-sitter"` in one graph. Dart was moved to `tree-sitter-dart 0.2.0`, which uses the `tree-sitter-language` bridge and works with the upgraded core.

The upgraded Go grammar changed malformed-tree recovery around a missing struct brace. Julie kept the recovered `VariadicFunction` symbol by adding a de-duplicated Go source fallback for visible `func Name(...)` signatures that the AST walk misses in error recovery. The recovered symbol is marked with confidence `0.8` and no parent because the malformed parse tree cannot provide a reliable parent.

The Dart parser replacement changed class, superclass, call-expression, field, mixin, and extension node shapes. Julie now accepts `class_declaration`, reads wrapped `type` nodes for Dart type text, and extracts calls from `call_expression` targets. The real-world parser contract caught the missing `runApp` identifier in the Flutter isolate fixture.

## `julie-extractors v2.14.0`

Julie moved from `julie-extractors v2.5.3` to `v2.14.0` at release commit `dc270e2f590c2b41413a9fd6e642bd8671022979`.

- T-SQL now parses bracketed multipart names, batch separators, identity declarations, and parser-backed `MERGE` statements without diagnostics while preserving malformed-input diagnostics.
- Razor uses the public expression node kinds for nested conditionals and template syntax; clean attribute parsing no longer exposes `@rendermode` values as synthetic property symbols from error recovery.
- C# uses the owned grammar fork at `688cf95ae4c984638557dab73253bd66719bdd5c`, including C# 14 file-based app and compound-assignment syntax.
- Swift moved to `0.7.3`, R moved to `1.3.0`, and the shared Tree-sitter runtime moved to `0.26.11`.
- Julie's consumer contract test covers current C#, R, Razor, T-SQL, and Swift syntax through `extract_canonical` and requires zero parse diagnostics.

## Parser Inventory

| Parser | Previous locked | Current locked | Latest checked | Decision | Evidence |
| --- | --- | --- | --- | --- | --- |
| `tree-sitter-bash` | `0.23.3` | `0.25.1` | `0.25.1` | upgraded | Golden gate passes after upgrade. |
| `tree-sitter-c` | `0.24.2` | `0.24.2` | `0.24.2` | current | No newer crates.io release. |
| `tree-sitter-cpp` | `0.23.4` | `0.23.4` | `0.23.4` | current | No newer crates.io release. |
| `tree-sitter-c-sharp` | crates.io `0.23.5` | git `688cf95ae4c984638557dab73253bd66719bdd5c` (`0.23.5`) | owned fork | upgraded and pinned | C# 14 file-based app and compound-assignment evidence. |
| `tree-sitter-css` | `0.23.2` | `0.25.0` | `0.25.0` | upgraded | Golden gate passes after upgrade. |
| `tree-sitter-dart` | `harper-tree-sitter-dart 0.0.5` | `0.2.0` | `0.2.0` | replaced and upgraded | Harper crate blocked core `0.26.8`; replacement passes golden gate. |
| `tree-sitter-elixir` | `0.3.5` | `0.3.5` | `0.3.5` | current | No newer crates.io release. |
| `tree-sitter-gdscript` | `5.0.1` | `6.1.0` | `6.1.0` | upgraded | Golden gate passes after upgrade. |
| `tree-sitter-go` | `0.23.4` | `0.25.0` | `0.25.0` | upgraded | Required malformed recovery fix, then golden gate passes. |
| `tree-sitter-html` | `0.23.2` | `0.23.2` | `0.23.2` | current | No newer crates.io release. |
| `tree-sitter-java` | `0.23.5` | `0.23.5` | `0.23.5` | current | No newer crates.io release. |
| `tree-sitter-javascript` | `0.23.1` | `0.25.0` | `0.25.0` | upgraded | Covers `javascript` and `jsx`; golden gate passes. |
| `tree-sitter-json` | `0.24.8` | `0.24.8` | `0.24.8` | current | No newer crates.io release. |
| `tree-sitter-kotlin-ng` | `1.1.0` | `1.1.0` | `1.1.0` | current | No newer crates.io release. |
| `tree-sitter-lua` | `0.2.0` | `0.5.0` | `0.5.0` | upgraded | Golden gate passes after upgrade. |
| `tree-sitter-md` | `0.5.3` | `0.5.3` | `0.5.3` | current | Manifest now records the locked current release. |
| `tree-sitter-php` | `0.24.2` | `0.24.2` | `0.24.2` | current | No newer crates.io release. |
| `tree-sitter-python` | `0.23.6` | `0.25.0` | `0.25.0` | upgraded | Golden gate passes after upgrade. |
| `tree-sitter-r` | `1.2.0` | `1.3.0` | `1.3.0` | upgraded | Current-syntax fixture parses cleanly. |
| `tree-sitter-regex` | `0.23.0` | `0.25.0` | `0.25.0` | upgraded | Golden gate passes after upgrade. |
| `tree-sitter-ruby` | `0.23.1` | `0.23.1` | `0.23.1` | current | No newer crates.io release. |
| `tree-sitter-rust` | `0.24.2` | `0.24.2` | `0.24.2` | current | No newer crates.io release. |
| `tree-sitter-scala` | `0.25.1` | `0.26.0` | `0.26.0` | upgraded | Golden gate passes after upgrade. |
| `tree-sitter-sequel-tsql` | `tree-sitter-sequel 0.3.11` | git `63ea933e464813d01cab5d7febcb0f77409c247b` (`0.4.2`) | owned fork | replaced and pinned | Terraform T-SQL fixtures parse cleanly while malformed controls remain diagnostic. |
| `tree-sitter-swift` | `0.7.2` | `0.7.3` | `0.7.3` | upgraded | Current-syntax fixture parses cleanly. |
| `tree-sitter-toml-ng` | `0.7.0` | `0.7.0` | `0.7.0` | current | No newer crates.io release. |
| `tree-sitter-typescript` | `0.23.2` | `0.23.2` | `0.23.2` | current | Covers `typescript` and `tsx`; no newer crates.io release. |
| `tree-sitter-yaml` | `0.7.2` | `0.7.2` | `0.7.2` | current | Manifest now records the locked current release. |
| `tree-sitter-zig` | `1.1.2` | `1.1.2` | `1.1.2` | current | No newer crates.io release. |
| `tree-sitter-powershell` | `0.26.3`, git `73800ecc8bddeee8f1079a5a2e0c13c3d00269bb` | `0.26.4`, git `d398441825243b00e317e87e1829b9d6a3e54ce0` | git source | upgraded and pinned | Updated to current remote commit, then pinned by `rev`. |
| `tree-sitter-qmljs` | git `606a66b96a13ef30ed5c7ec7e5adc20a9a40157a` | git `606a66b96a13ef30ed5c7ec7e5adc20a9a40157a` | git source | pinned | No remote movement during audit; manifest now pins `rev`. |
| `tree-sitter-razor` | git `a3399c26610817c6d32c7643793caf3729cfb6d2` | git `fba8571f06c06aa5acca01e3d762f5a5e78dc50f` | owned fork | upgraded and pinned | Public expression nodes and current Razor template syntax are certified. |
| `tree-sitter-vb-dotnet` | git `25dca4ac1456c691e2381a2d76151ef432aefc9e` | git `25dca4ac1456c691e2381a2d76151ef432aefc9e` | git source | pinned | Already pinned by `rev`; kept unchanged. |

## Commands

Parser tests and golden gates run in the external [`anortham/julie-extractors`](https://github.com/anortham/julie-extractors) repo. After re-pinning the git-dep here, run the Julie-side extractor dependency integration gate:

```bash
cargo xtask test bucket extractor-dep-integration
```

For branch handoff after parser work, also run the plan-required broader gates:

```bash
cargo xtask test changed
cargo xtask test dev
```

Add `cargo xtask test system` when watcher, startup, indexing, or workspace lifecycle code changed. Add `cargo xtask test dogfood` when graph or search behavior changed.
