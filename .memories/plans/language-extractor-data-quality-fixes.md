---
id: language-extractor-data-quality-fixes
title: Language Extractor Data Quality Fixes
status: completed
created: 2026-04-03T00:24:16.145Z
updated: 2026-04-03T12:16:58.502Z
tags:
  - data-quality
  - extractors
  - language-verification
---

# Language Extractor Data Quality Fixes

## Goal
Fix outstanding language verification failures to establish trustworthy data across all 33 languages. This is prerequisite work before any enterprise intelligence features.

## Batch 1: Missing Class Extraction (Parallel - Agent Team)
Three independent extractor bugs where language-specific class modifiers produce different AST nodes that `extract_class()` doesn't match.

### Swift: `open class Session: @unchecked Sendable` not extracted
- **File:** `crates/julie-extractors/src/swift/types.rs` (extract_class at line 11)
- **Tests:** `crates/julie-extractors/src/tests/swift/`
- **Bug:** `extract_class()` returns None for this file. Other classes with `@unchecked Sendable` work. ~80 methods orphaned as top-level functions.
- **Fix pattern:** Check tree-sitter AST for the failing syntax, add missing node type match
- **Verification project:** Alamofire/Alamofire (`~/source/Alamofire`)

### Kotlin: `sealed class JsonReader` not extracted
- **File:** `crates/julie-extractors/src/kotlin/types.rs` (extract_class at line 13)
- **Tests:** `crates/julie-extractors/src/tests/kotlin/`
- **Bug:** `sealed class JsonReader` absent from symbols. 30+ member functions orphaned. `JsonWriter` (also sealed) IS extracted correctly.
- **Fix pattern:** Compare AST for working vs failing sealed class, find the difference
- **Verification project:** square/moshi (`~/source/moshi`)
- **Also:** Missing space in Moshi class signature: `Moshiprivate constructor(builder: Builder)`

### Dart: Dart 3 class modifiers not handled
- **File:** `crates/julie-extractors/src/dart/mod.rs` (line 325 area)
- **Tests:** `crates/julie-extractors/src/tests/dart/`
- **Bug:** `base class`, `sealed class`, `final class`, `interface class` produce different AST node types than `class_definition`. ProviderContainer and AsyncValue not extracted.
- **Fix pattern:** Check tree-sitter-dart grammar for Dart 3 modifier node types, add matches
- **Verification project:** rrousselGit/riverpod (`~/source/riverpod`)

## Batch 2: Centrality/Identity Problems (Sequential - Lead investigates)
These touch shared infrastructure and need careful design.

### Python: Test subclass steals centrality
- Pending relationship resolution picks test `Flask` subclass over real Flask
- Need test-scope awareness in resolver

### Ruby: Class + constant dual symbols
- Tree-sitter produces both class and constant for same definition
- Centrality goes to constant, class gets 0

### C: Header/implementation centrality split
- References attributed to header declaration, implementation gets 0

## Batch 3: Relationship Gaps (Lower priority)
- PHP: Class-level relationship tracking weak
- C++: Zero cross-file refs in header-only projects

## Success Criteria
- All Batch 1 languages pass verification checks 1-6
- No regressions in `cargo xtask test dev`

