# Base Infrastructure Audit

This document audits the foundation underlying all 33 language extractors: `crates/julie-extractors/src/base/` plus the language registry, span normalization, and result aggregation.

## Summary

The base infrastructure is well-organized — clean separation between span normalization, ID generation, symbol/identifier/relationship creation, and result aggregation. The registry pattern with capability tiers is excellent for a multi-language system. However, there are seven concrete defects in the base layer that cascade into every extractor: a fragile visibility fallback, an over-greedy doc-comment recognizer for hash/dash/double-slash idioms, a hash-based symbol ID with realistic collision potential, a hardcoded `markdown` content-type special case, identifier extraction defined as "Phase 1 — basic" in Rust with the same shape across most languages, an inconsistent `extract_documentation` alias that does the same thing as `find_doc_comment`, and a relationship_id format that collides on multiple calls per line.

## 1. Architecture Overview

`crates/julie-extractors/src/base/` provides:

| File | Responsibility |
|---|---|
| `extractor.rs` | `BaseExtractor` struct + node text + doc comment + ID generation + context extraction |
| `creation_methods.rs` | `create_symbol`, `create_identifier`, `create_relationship`, visibility detection, containing-symbol search |
| `relationship_resolution.rs` | `UnresolvedTarget`, `StructuredPendingRelationship`, scoped symbol index for local resolution |
| `tree_methods.rs` | Tree walking: `find_nodes_by_type`, `find_parent_of_type`, `traverse_tree`, `get_field_text` |
| `span.rs` | `NormalizedSpan` (1-based lines), `RecordOffset` (for embedded language sections), path normalization |
| `types.rs` | All canonical types: `Symbol`, `Identifier`, `Relationship`, `SymbolKind` (23 variants), `RelationshipKind` (14 variants), `IdentifierKind` (4 variants), `Visibility`, `TypeInfo`, `ParseDiagnostic` |
| `results_normalization.rs` | `ExtractionResults::extend`, `apply_record_offset`, `rekey_normalized_locations` |
| `annotations.rs` | `AnnotationMarker` for decorators/attributes |

Capability tiers (`language_spec/mod.rs`):

- `FULL_CAPABILITIES` — symbols + relationships + pending_relationships + identifiers + types
- `NO_PENDING_CAPABILITIES` — full minus cross-file pending (HTML, Razor, SQL, Regex)
- `PENDING_NO_TYPES_CAPABILITIES` — full minus types (Lua, QML, R)
- `RELATIONSHIP_DATA_CAPABILITIES` — symbols + relationships + identifiers (CSS, Markdown, YAML)
- `DATA_ONLY_CAPABILITIES` — symbols + identifiers only (JSON, TOML)

The capability matrix at `fixtures/extraction/capabilities.json` enforces that every registry entry's declared capabilities match the test-time observed capabilities AND has at least one golden fixture. **Currently zero `capability_gaps` are recorded.** This means the project asserts every extractor is at parity with its declared tier — but as this audit shows, "tier parity" is a low bar; it does not require comprehensive symbol coverage.

## 2. Concrete Defects

### 2.1 Fragile visibility fallback (high impact)

`crates/julie-extractors/src/base/creation_methods.rs:264-288`

```rust
pub fn extract_visibility(&self, node: &Node) -> Option<Visibility> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            match child.kind() {
                "public" => return Some(Visibility::Public),
                "private" => return Some(Visibility::Private),
                "protected" => return Some(Visibility::Protected),
                _ => continue,
            }
        }
    }
    let text = self.get_node_text(node);
    if text.contains("public ") {
        Some(Visibility::Public)
    } else if text.contains("private ") {
        Some(Visibility::Private)
    } else if text.contains("protected ") {
        Some(Visibility::Protected)
    } else {
        None
    }
}
```

Two problems:

1. **The child-kind check is too narrow.** Real grammars expose visibility as named nodes like `accessibility_modifier` (TypeScript), `modifier` containing `public_keyword` (C#), `pub` (Rust), or trailing access specifiers like `public:` (C++). Only Java's tree-sitter exposes `public` as a direct kind. So for most languages the child loop never fires.

2. **The text fallback scans the entire node text including the body.** A function `function getKey(){ return "public_key"; }` gets classified as `Public` because the body contains `"public "`. A method whose body has a comment "// returns the public API" same problem. This is a real correctness defect.

The right fix is to remove the text-based fallback entirely from base and require each language to override visibility detection. C++ already does this correctly in `crates/julie-extractors/src/cpp/visibility.rs:19` — it walks named siblings looking for access specifiers and respects struct (default public) vs class (default private). Rust's `helpers.rs` has its own pub/pub(crate) handling. TypeScript/JavaScript currently fall through to the buggy text path; that should be replaced.

Languages that currently call the buggy base method:

```
crates/julie-extractors/src/typescript/classes.rs:17
crates/julie-extractors/src/typescript/functions.rs:32, 162
crates/julie-extractors/src/javascript/functions.rs:87, 166
crates/julie-extractors/src/javascript/variables.rs:50, 86, 120, 174, 213, 250
crates/julie-extractors/src/javascript/types.rs:54, 103, 141
```

TypeScript at line 162 wraps it in `or_else` after `extract_ts_visibility`, which is the right pattern; the others are bare calls.

### 2.2 Over-greedy doc-comment recognizers

`crates/julie-extractors/src/language_spec/mod.rs:60-80` defines `DocCommentStyle` matchers including:

```rust
Self::GoLine        => trimmed.starts_with("//"),
Self::LuaDoubleDash => trimmed.starts_with("--"),
Self::HashLine      => trimmed.starts_with("#"),
Self::SqlLine       => trimmed.starts_with("--"),
```

Combined with `find_doc_comment` (extractor.rs:103-184) walking *all* preceding sibling comments and only stopping at non-doc comments, this means **every single line comment preceding a symbol becomes its doc comment** in Go, Lua, SQL, Ruby, Bash, and PowerShell:

```bash
# Adjust temperature to fit the calling shell's locale.
# (And it is also not idiomatic style, the system was migrated last week.)
# TODO: revisit when the env var rollout finishes.
function set_locale() { ... }
```

Julie attaches all three lines as the doc comment for `set_locale`. For Go this is conventional (line comments that precede a declaration are godoc). For Bash, Ruby, Lua, R, PowerShell, SQL it is simply a misclassification — these communities use specific markers (`##` for sphinx-style, `--[[` block, `#'` Roxygen, `<# #>` PowerShell block) for documentation distinct from regular comments.

The right fix is to split `HashLine`/`LuaDoubleDash`/`SqlLine` into "any-comment" vs "doc-marker" matchers and have the extractors that genuinely treat all preceding comments as docs (Go) opt in explicitly. PowerShell is already configured with `EMPTY` doc styles; that should not be the case — PowerShell has well-defined `<# .SYNOPSIS #>` block comments that should be matched.

### 2.3 Symbol ID collision potential

`extractor.rs:187-191`:

```rust
pub fn generate_id(&self, name: &str, line: u32, column: u32) -> String {
    let input = format!("{}:{}:{}:{}", self.file_path, name, line, column);
    let digest = md5::compute(input.as_bytes());
    format!("{:x}", digest)
}
```

Symbol IDs are MD5 of `file_path:name:line:column`. Two ways this can collide for legitimate code:

- **Multiple calls/identifiers at the same column on the same line**: `obj.foo().bar()` where `foo` and `bar` both appear at the same start column relative to their containing expression — possible if a tree node and a child node share the same start position. This happens routinely with `scoped_identifier`, `field_expression`, and `call_expression` in Rust/Go where the outer node and the inner identifier both start at the same byte.
- **Same name at the same column on consecutive lines after a refresh**: `refresh_id` (types.rs:168-177) recomputes the ID after offset normalization, but if two symbols normalize to the same span (which can happen when an offset is applied to lines that were previously distinct in raw bytes but now collapse — rare but real for Vue/Razor where embedded sections share line numbers), they collide and one overwrites the other in `symbol_map`.

Switching to xxhash3 or blake3 of `(file_path, name, start_byte, end_byte)` would be both faster than MD5 and collision-safe (byte ranges are unique within a file). The current scheme also costs an extra hex encode allocation per symbol/identifier; with millions of symbols this is non-negligible.

### 2.4 `extract_visibility` exists in two unrelated forms

The `BaseExtractor::extract_visibility` at `creation_methods.rs:264` is the buggy one above. Each language then has its own mostly-private helper (e.g., `csharp::helpers::determine_visibility`, `cpp::visibility::extract_visibility_from_node`, `rust::helpers::extract_rust_visibility`). This is fine in isolation, but the base method should be deleted, not left dangling, because new extractors will reach for it (and get the wrong answer). At minimum it should be marked `#[doc(hidden)]` with a `// don't use; override per language` note.

### 2.5 `extract_documentation` is a redundant alias

`tree_methods.rs:157-159`:

```rust
pub fn extract_documentation(&self, node: &Node) -> Option<String> {
    self.find_doc_comment(node)
}
```

Nothing in the codebase uses this alias (verified via grep — zero call sites). Dead public API surface. Drop it.

### 2.6 `relationship_id` collision on multiple calls per line

`creation_methods.rs:127-132` and `results_normalization.rs:6-16` use the same format:

```rust
format!("{}_{}_{:?}_{}",  from_symbol_id, to_symbol_id, kind, line_number)
```

`foo(); bar(); foo();` on one line produces:

- `caller_FOO_Calls_42`
- `caller_BAR_Calls_42`
- `caller_FOO_Calls_42`  ← duplicate ID!

Two `Calls` from `caller` to `FOO` on line 42 get the same ID. If they're inserted into a HashMap or compared by id, one is lost. Adding column or a per-line counter would fix this.

### 2.7 Hardcoded `markdown` content-type

`creation_methods.rs:35-40`:

```rust
let content_type = if self.language == "markdown" {
    Some("documentation".to_string())
} else {
    None
};
```

This is the only place `content_type` is set. JSON, TOML, YAML — also documentation-adjacent — get `None`. SQL DDL files get `None`. The whole `content_type` field carries one binary signal: "is this Markdown or not." That is not a real type system; it's a hack hidden inside a base method. Either remove the field or extend it to a proper `ContentDomain` enum (`Code`, `Configuration`, `Documentation`, `Markup`, `Data`, ...) populated per-language in the registry.

## 3. Coverage Gaps in the Base API

### 3.1 No first-class identifier kinds for control flow / lifetime / annotation references

`IdentifierKind` (types.rs:247-256) has only four variants:

```rust
pub enum IdentifierKind { Call, VariableRef, TypeUsage, MemberAccess }
```

Real refactoring tools need more granularity:

- **AnnotationRef** — `@Override`, `[Authorize]`, `#[derive]`, `@Component` — currently classified as `Call` or skipped
- **MacroInvocation** — `vec![]`, `println!()` — Rust currently extracts these as `Function` symbol *definitions* in `signatures::extract_macro_invocation`, not as identifier references at the call site
- **ImportRef** — when an imported symbol is used in code, currently it's a `Call` or `VariableRef` with no signal that the binding came from an import
- **GenericArg** / **TypeParam** — `<T>` in `Vec<T>` is currently `TypeUsage` indistinguishable from a regular type reference

This isn't a defect per se — the limited enum keeps things simple — but a "world-class" semantic layer would distinguish these, especially for cross-language graph queries.

### 3.2 No `Definition` vs `Declaration` split on Symbol

C/C++ separate forward declarations from definitions. The current `Symbol` has no field that says "this is just `extern int foo;`, not the body." Some metadata gets set via `metadata.isDefinition` which `relationship_resolution::symbol_definition_status` reads (line 235-242), but it's per-extractor and ad-hoc. Headers vs implementation files in C/C++ projects produce duplicate symbols where one should be the definition and the other a declaration link. This is at the line between "base infra" and "C++ extractor"; the absence of a proper field on Symbol makes it hard for any extractor to handle correctly.

### 3.3 No `Async` / `Generator` / `Const` / `Static` modifier flags

`Symbol` has `signature` (string) and ad-hoc `metadata.isAsync` etc. that some extractors set. There's no canonical structured way to say "this is an async function." Search has to grep the signature for `"async "`. This works but is fragile for multilingual code (Python `async def`, Rust `async fn`, Kotlin `suspend fun`, C# `async Task`, JS `async function`).

A canonical `modifiers: BitFlags<SymbolModifier>` field with `Async, Generator, Const, Static, Override, Abstract, Final, Sealed, Inline, Comptime, Unsafe, Volatile` would make cross-language queries dramatically simpler.

### 3.4 No `ImportPath` / `ExportPath` on import symbols

Imports are stored as `Symbol { kind: Import, name: <imported name> }` but the **source module** is buried in metadata or signature text. Any cross-file resolver has to re-parse the signature to recover what was imported from where. A typed `import_source: Option<ImportPath>` field would make this a one-step lookup.

## 4. Dead / Nearly-Dead Code

- `tree_methods.rs:157-159` `extract_documentation` — alias with no callers.
- `tree_methods.rs:67-77` `get_children_of_type` — duplicate of `find_children_by_type` (line 132); both exist with the same behavior.
- `tree_methods.rs:50-59` `find_parent_of_type` — used by 2 callers across all extractors. Could move to a free function.
- `tree_methods.rs:87-117` `traverse_tree` with `catch_unwind` panic guard — used by zero language extractors (verified). Tree-sitter parsing itself doesn't panic; this guard is dead. Remove.
- `creation_methods.rs:194-261` `find_containing_symbol_from_iter` — heavy logic with priority-by-kind and size-by-byte-range. Used by the public `find_containing_symbol_from_map_filtered` only. Worth profiling under load — every identifier creation calls this, sorting all candidate symbols.

## 5. Symptoms in Test Fixtures

The `tests/capability_matrix.rs:138` test enforces every registry entry has at least one golden fixture. Inspecting `fixtures/extraction/<language>/basic/source.*` and `expected.json` reveals fixtures are uniformly small (one class, two methods). They don't exercise:

- Nested classes / nested traits / impl-on-impl
- Generic type parameters with bounds (`<T: Clone + Send>`)
- Multi-line decorators / annotations
- Anonymous classes / closures-as-args
- Macro invocations with embedded code
- Late-binding patterns (Ruby's `define_method`, Python's `setattr`, JS dynamic property access)
- Cross-section embedded language (Vue template references inside script)

The fixture suite is good as a smoke test ("the extractor runs and produces SOMETHING reasonable") but does not establish a high quality bar. Many of the per-language gaps in this audit would not be caught by the existing fixtures because the fixtures don't exercise those features.

## 6. Recommendations (ranked by impact)

1. **Delete the text fallback in `BaseExtractor::extract_visibility`.** Force each language to provide its own. Mark the base method `pub(super)` and rename to `extract_visibility_from_named_children` so callers can't reach for it without thinking.
2. **Split `HashLine`/`LuaDoubleDash`/`SqlLine`/`GoLine` doc-style matchers** so "any line comment" doesn't mean "any line comment is a doc comment." Reserve the universal `/**` heuristic; everything else opt-in.
3. **Switch symbol ID generation to xxhash3** of `(file_path, name, start_byte, end_byte)` and switch `relationship.id` to include start/end columns. Eliminates collision risk and is faster.
4. **Add `modifiers: BitFlags<SymbolModifier>` field to Symbol** for canonical async/static/const/abstract/etc. Saves every search query from string-matching the signature.
5. **Add `import_source: Option<String>` field to Symbol** so import-kind symbols carry their source module without round-tripping through metadata.
6. **Fix Vue Options API extraction** to use tree-sitter on the `<script>` block JS/TS content rather than regex on raw lines. The Composition API path already does this; symmetrize the two.
7. **Populate `capability_gaps` in the JSON matrix** for the gaps documented in the per-tier audits — not as a TODO list but as a structured statement of "this language declares X but extracts Y." Right now the matrix asserts perfection across 36 languages, hiding the real coverage map.
8. **Remove dead code**: `extract_documentation`, `get_children_of_type`, `traverse_tree` panic guard.

The base layer is solid scaffolding. The defects above are not "rewrite from scratch" issues — they're "tighten the bolts" issues. Done together they would meaningfully lift the floor under every language extractor.
