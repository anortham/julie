# ADR-0005: Source-aware language detection precedence

## Context

Language detection in Julie answers two different questions:

1. **Indexing gate**: "Does this file extension have a registered extractor at all?" — used to decide whether the watcher should even enqueue the path. Pure extension lookup; no content needed.
2. **Language attribution**: "What language is THIS file?" — used at index time, by live AST rewrites, by relationship resolution, and by resolver scoring. The answer must agree across all consumers or the same symbol gets indexed as one language and looked up as another.

This ADR governs question 2. Question 1 is correctly answered by `detect_language_from_extension` (used at e.g. `src/watcher/runtime.rs::path_has_registered_extractor`) and is out of scope.

Julie indexes 34 languages. Attribution by extension alone breaks for one stubborn case: C/C++ headers. The `.h` extension is shared by C and C++. Pre-cleanup, four places made independent attribution decisions about a `.h` file:

- Extractor pipeline (`crates/julie-extractors/src/pipeline.rs`) — defaulted to C via the language spec.
- `LanguageSpec` registry (`crates/julie-extractors/src/language_spec/specs.rs`) — `.h` → C.
- `rewrite_symbol` live AST parsing (`src/tools/editing/rewrite_symbol.rs`) — extension-only.
- Indexing file policy (`src/tools/workspace/indexing/file_policy.rs`) — plus Dockerfile, Makefile, TOML, JSON, shell name rules layered on top.

Worse, the workspace indexing resolver used extension-derived language for the *caller* file when scoring same-language symbol candidates, even though the database already had the indexer's source-aware decision stored. So a function defined in a C++ header could be deboosted when called from another C++ file because the resolver classified the caller as "C."

These were not independent bugs — they were the same missing policy duplicated four times.

## Decision

There is a single source-aware language detection policy: `detect_language_for_source(file_path: &str, content: &str) -> Option<&'static str>` at `crates/julie-extractors/src/language_spec/mod.rs:281`.

The rule:

1. Start with the extension-based language from `LanguageSpec`.
2. For `.h` files specifically: call `header_contains_cpp_syntax(content)` (line 294). This function compares the C and C++ parsers' error counts. If C++ parses with strictly fewer errors than C, the file is C++; otherwise it stays C.
3. Empty / whitespace-only content short-circuits to the extension default — no parser comparison, no warning.

This decision is the **single point of truth** for language at indexing time, live-rewrite time, watcher time, and the resolver's same-language scoring:

- **Indexing**: `crates/julie-extractors/src/pipeline.rs:29` calls `detect_language_for_source` per file.
- **Live AST rewrites**: `src/tools/editing/rewrite_symbol.rs::parse_live_tree` calls `detect_language_for_source`; `src/tools/refactoring/mod.rs` does the same for `rename_symbol`.
- **Watcher**: `src/watcher/handlers.rs` indexes files through the same source-aware path.
- **External extraction CLI**: `julie-server` external extract path uses `detect_language_for_source`.
- **Resolver same-language scoring**: `src/tools/workspace/indexing/resolver.rs::caller_language_for_pending` (line 196) returns the **stored** language from `get_file_languages_by_paths` — what the indexer decided — and falls back to extension detection only when the row is missing.

The decision rule for the resolver is critical: the database row is the indexer's source-aware verdict. Falling back to extension is a stale-data path, not a "second opinion."

## Consequences

**Easier**

- A C++ header is C++ everywhere: index, rewrite, rename, watcher, resolver. No tool sees a different decision than any other.
- Adding a new tool that needs language for a file path: call `detect_language_for_source` (if you have content) or fetch the stored language from the database (if you don't).
- The `.h` C-vs-C++ decision is centralized and testable. New ambiguous-extension cases (e.g. `.pl` Perl-vs-Prolog, `.m` Objective-C-vs-MATLAB) can use the same parser-error-count pattern.

**Harder**

- Source-aware detection requires content. Anywhere we only have a path, we must either fetch content first or fall back to the stored-language path. The stored-language path is the right answer for resolver-style scoring; content-loading is the right answer for live-rewrite-style operations.
- The parser-error-count comparison runs both C and C++ parsers on every `.h` file at index time. This is paid once per file per indexing pass; the cost is small but measurable on large C/C++ codebases.

## Applies To

- `crates/julie-extractors/src/language_spec/mod.rs::{detect_language_for_source, header_contains_cpp_syntax}`
- `crates/julie-extractors/src/pipeline.rs` (indexing)
- `src/tools/editing/rewrite_symbol.rs::parse_live_tree`
- `src/tools/refactoring/mod.rs` (rename_symbol AST rewrites)
- `src/watcher/handlers.rs` (watcher indexing)
- `src/tools/workspace/indexing/resolver.rs::caller_language_for_pending` (resolver same-language scoring)

## Future Agents

- First, ask yourself which question you're answering. If you're gating ("should this file be enqueued for indexing at all?"), extension lookup is correct — use `detect_language_from_extension`. If you're attributing ("what language IS this file, for parsing or scoring purposes?"), follow the rest of this ADR.
- Do not introduce a new "decide language from path" code path for attribution. If you have content, call `detect_language_for_source`. If you only have a path, fetch the stored language via `get_file_languages_by_paths` (or `get_file_language` for a single path) — that is the indexer's source-aware verdict.
- Do not assume `.h` is C in resolver-style scoring, language filtering, or relationship resolution code. Always consult stored language first; the database row is the authority.
- When adding a new ambiguous-extension language pair, extend `header_contains_cpp_syntax` (or write an analogous `*_contains_*_syntax`) and gate it on the extension. Do not embed substring sniffing in the indexer or extractor pipeline directly.
- The empty/whitespace short-circuit in `header_contains_cpp_syntax` is intentional. Do not remove it — empty `.h` files are common during file creation and parsing them produces noise without information.
- The `tracing::warn!` for parser-error mismatch is intentionally a warn, not a debug, because it signals real disagreement between parsers on non-empty content. Do not downgrade it without a load-bearing reason.
