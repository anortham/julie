---
id: scala-and-elixir-language-support
title: Scala and Elixir Language Support
status: completed
created: 2026-03-16T21:49:10.012Z
updated: 2026-03-21T21:41:48.577Z
tags:
  - scala
  - elixir
  - language-support
  - extractors
---

# Scala & Elixir Language Support

## Goal
Add Scala (#28) and Elixir (#29) to Julie's language roster, bringing it to 33 entries.

## Approach
- **Scala**: Kotlin-templated extractor (rich named AST nodes, JVM sibling)
- **Elixir**: Novel call-dispatch pattern (everything is `call` nodes, Ruby `calls.rs` template)
- 7 new files per language, 10 registration points each

## Plan Document
`docs/superpowers/plans/2026-03-16-scala-elixir-language-support.md`

## Tasks
- [ ] Task 1-2: Scala dependency + skeleton + registration
- [ ] Task 3: Scala types (class, trait, object, enum)
- [ ] Task 4: Scala declarations (function, import, package)
- [ ] Task 5: Scala properties + Scala 3 features
- [ ] Task 6-7: Scala relationships + identifiers + types
- [ ] Task 8-9: Elixir dependency + skeleton + registration
- [ ] Task 10: Elixir defmodule + def/defp
- [ ] Task 11: Elixir defmacro + defprotocol + defimpl + defstruct
- [ ] Task 12: Elixir imports + module attributes
- [ ] Task 13: Elixir relationships + identifiers + type inference
- [ ] Task 14: End-to-end verification + dogfooding
