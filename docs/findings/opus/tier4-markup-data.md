# Tier 4 Extractor Audit — Markup, Data, Specialized

## Summary

These 11 extractors range from competent (Markdown, Regex, YAML) to architecturally
problematic (Vue, HTML). The most impactful systemic issues: (1) Vue performs a
full SFC reparse for every identifier extracted, runs Options API entirely
through regex (so methods/computed/props themselves never become symbols), and
ignores the template section for definitions; (2) HTML and CSS share a non-unique
naming pattern where every `<a>` becomes a symbol named "a" and every `@import`
becomes "@import", crippling search and identifier resolution; (3) SQL has a
self-referential JOIN bug, dozens of inline `Regex::new(...).unwrap()` calls
inside hot paths, and views that only get extracted via regex over the whole
node text; (4) Markup languages with `NO_PENDING_CAPABILITIES` (HTML, Razor,
SQL, Regex) silently lose all cross-file reference data even when the source
clearly addresses other files (`<script src>`, `@inject`, foreign-key REFERENCES,
backreferences across modular regex). Vue, Razor, and SQL deserve targeted
investments; the simpler data formats (JSON, TOML, YAML, Markdown) are mostly
fine but leave easy wins on the table.

## Per-Language Findings

### HTML
**Status**: Needs Work

**Strengths**:
- Smart filtering: only meaningful elements (`#id`, `name=`, semantic landmarks, custom elements) become symbols. The `should_extract_element` allowlist in `html/elements.rs:24-58` cuts noise effectively.
- Solid attribute prioritization for signature building (`html/attributes.rs:72-142`); per-tag priority lists for `img`, `input`, `iframe`, etc.
- Class identifiers are split per-token (`html/identifiers.rs:68-79`) so `class="foo bar"` produces two `MemberAccess` identifiers.
- DOCTYPE captured as `Namespace`.

**Gaps & Errors**:
- **Element symbols are named by tag, not by id/name**. `html/elements.rs:128` passes `tag_name` (e.g., "a", "div", "input") as the symbol name. Two `<a id="x">` and `<a id="y">` collide on the same name "a". The id is stored in metadata but not searchable. Real fix: prefer `attributes.get("id")` or `attributes.get("name")` when present, fallback to `tag_name`.
- **Script/style symbol names are the literal strings "script" and "style"** (`html/scripts.rs:70, 122`). Multiple scripts in one file all collide.
- **`<style>` is `SymbolKind::Variable`** (`html/scripts.rs:123`); should probably be `Module` or `Namespace` for consistency with other "container" semantics.
- **Inline `<script>` content is not parsed as JS**. The script's text becomes part of the signature/metadata but no JS symbols (functions, variables) are extracted. Vue handles this; HTML doesn't.
- **Inline `<style>` content is not parsed as CSS**. Same problem — selectors and custom properties inside `<style>` are invisible.
- **`script_relationships` always picks the first script element**. `html/relationships.rs:124-131` does `symbols.iter().find(|s| ... script-element)` instead of finding the script that *owns* this node. Subsequent scripts with `src` attributes get attached to the first script's symbol.
- **`find_element_symbol` matches by tag + ±2 lines** (`html/relationships.rs:172-176`), so href/src on adjacent elements with the same tag name fight each other.
- **No HTML form linkages**. `<label for="x">` -> `<input id="x">`, `<input form="formid">`, `<a href="#anchor">` to local anchors are not relationships even though they're in-file resolvable.
- **No SVG `<use href="#sym">` references** are tracked.
- **`NO_PENDING_CAPABILITIES`** means cross-file refs (`<script src="../x.js">`, `<img src="...">`, `<link href="x.css">`) become `url:foo` / `resource:foo` placeholders that never resolve. These could be `pending_relationships`.

### CSS
**Status**: Good

**Strengths**:
- Tracks CSS custom property usage via `var(--name)` with proper comment stripping (`css/relationships.rs:9-14, 142-167`).
- Tracks `animation-name: foo` -> `@keyframes foo` references.
- Per-symbol-kind classification: `.class` is `Property`, `#id` is `Variable`, `:root` is `Property` (`css/rules.rs:39-47`). The split is debatable but consistent.
- LazyLock-cached regexes for hot paths.

**Gaps & Errors**:
- **`@import` symbols all named "@import"** (`css/at_rules.rs:49`). Multiple imports collide. Name should include the URL or be the imported filename.
- **Other at-rules (`@media`, `@supports`, `@charset`, `@namespace`) all named with the bare keyword**. Consequently 3 `@media` queries in one file produce 3 symbols all named "@media".
- **No `@font-face` handling at all**. `grep` for `font-face` returns nothing in `css/`. These declare named fonts that *should* be symbols.
- **No `@import url("...")` resolution**. The signature contains the URL but no relationship is created (Imported file might be in workspace).
- **Class/id selectors don't link to HTML elements**. Even though both extractors emit `MemberAccess` identifiers for the same names, no relationship is built. Cross-file is impossible (`RELATIONSHIP_DATA_CAPABILITIES` blocks pending), but in-workspace selectors that match HTML ids/classes could be surfaced via the identifier system.
- **No CSS Nesting (CSS Nesting Module Level 1) handling**. Modern CSS allows nested selectors; if the tree-sitter grammar exposes them, the extractor doesn't recurse meaningfully into them.
- **No `:has()`, attribute selector, or pseudo-class argument tracking**. `css/identifiers.rs:108-112` notes "Future: pseudo_class_selector, attribute_selector".
- **`var(--name)` in identifiers**: only class/id selectors are extracted as identifiers; `var()` references are only captured at the relationship level. Identifier parity with the relationship layer would help find_references.

### Vue
**Status**: Significant Gaps

**Strengths**:
- `<script setup>` (Composition API) DOES use tree-sitter (`vue/script_setup.rs:36-48`) — parses TS/JS properly.
- Recognizes Vue compiler macros (`defineProps`, `defineEmits`, `defineExpose`) and tags variables with `compositionApi` metadata (`vue/script_setup.rs:228-247`).
- Component-level symbol synthesized from filename (`vue/mod.rs:65-101`).
- Template component-tag pending relationships emitted (`vue/relationships.rs:57-101`) — e.g., `<UserProfile />` becomes a structured pending relationship that can resolve cross-file.

**Gaps & Errors**:
- **Vue SFC structure parsed by REGEX, not tree-sitter** (`vue/parsing.rs:62-143`), despite `language_spec/specs.rs:92-99` declaring `tree-sitter-html`. `<template>`, `<script>`, `<style>` are matched line-by-line. This breaks for: nested template tags, multiline tag attributes, malformed/incomplete files.
- **Options API extraction is shallow regex matching** (`vue/script.rs:18-121`): `data`, `methods`, `computed`, `props` are extracted as **single five-byte symbols**. The actual props/methods/computed properties inside them are NEVER extracted. So `methods: { increment() {...}, decrement() {...} }` becomes ONE symbol "methods", not "increment" and "decrement".
- **Plain function regex `FUNCTION_DEF_RE`** (`vue/script.rs:88`) catches top-level `function foo()` but misses class methods, arrow functions, named exports, async functions. Test coverage at `tests/vue/mod.rs:80-101` indicates this is what's expected, but the result is severely incomplete.
- **`extract_identifiers` reparses the entire SFC FOR EACH IDENTIFIER** (`vue/identifiers.rs:185-225`). The function `create_identifier_with_offset` calls `parse_vue_sfc(&original_content)` for every identifier extracted. For a 100-identifier Vue file this means 100 SFC reparses with regex.
- **Template section returns empty symbols** (`vue/mod.rs:172-176`). Template refs (`ref="myButton"`), `v-model` bindings, slots (`<template #default="...">`) are never captured as definitions.
- **Template relationships only track `{{ x }}` and `@event="x"`** (`vue/relationships.rs:179-206`). Missing: `v-bind:prop="x"` / `:prop="x"`, `v-if`, `v-for`, `v-show`, `v-model`, scoped slot binding expressions.
- **`unique_symbols_by_name` filters duplicates entirely** (`vue/relationships.rs:280-298`). If two functions share a name (rare in JS but possible across script blocks), neither resolves. This is heavy-handed; should use scope info instead.
- **`defineProps({ name: String })` doesn't extract `name` as a symbol**. Only the receiving variable (`const props = ...`) gets a symbol.
- **`defineEmits(['change'])` doesn't extract `change` as a symbol**. Same problem; emit names are invisible.
- **`start_byte: 0, end_byte: 0` for ALL Vue symbols** (`vue/script.rs:213-214`). Code-context extraction (used by `get_context`, search snippets) cannot extract Vue symbol bodies.
- **Symbol IDs are `format!("{}:{}:{}", name, start_line, start_column)`** (`vue/script.rs:201`). Two functions with the same name on the same line would collide; multi-script files have higher collision risk.
- **Style section uses regex for CSS** (`vue/style.rs:14-89`). No `@keyframes`, no nested selectors, doesn't preserve `:scoped` or `:deep()` semantics.
- **Two `<script>` blocks in same file**: only the first is processed in `extract_identifiers` (`vue/identifiers.rs:18-35` returns after the first match). Vue 3 allows companion `<script>` blocks alongside `<script setup>` for top-level code; both should be parsed.

### QML
**Status**: Good (with gaps)

**Strengths**:
- Tree-sitter-qmljs based; properly handles Composition-API-style `function_declaration`, `lexical_declaration`.
- Component name derived from file stem (`qml/mod.rs:78-83`) — correct QML convention.
- `id:` bindings extracted as `Property` symbols (`qml/mod.rs:122-154`) — these are the important referent IDs.
- Enum and enum member extraction (`qml/mod.rs:157-189`).
- Signal extraction as `SymbolKind::Event`.
- Property binding relationships traced (`qml/relationships.rs:175-236`).
- Pending relationships set up for cross-file calls; signal handlers (`onClicked`, etc.) properly recognized as containers via `find_containing_function` -> `ui_script_binding` (`qml/relationships.rs:283-287`).

**Gaps & Errors**:
- **No `anchors.fill`, `anchors.left: parent.left`, etc.** Anchors are central to QML positioning; they're treated as plain property assignments and lost.
- **No `Behavior on x { ... }` (animation declarations)** as symbols.
- **No `states: [...]` / `transitions: [...]`** explicit state/transition handling — state names are lost in array literals.
- **No `Connections { target: x; onChanged: ... }` block handling**. Connections are a key external-signal-handling pattern in QML.
- **Property aliases lose their target**. `property alias text: textArea.text` (line 108) keeps the full text in the signature but doesn't create a relationship to `textArea.text`.
- **Attached properties (`Layout.fillWidth`, `Keys.onPressed`)** aren't extracted as namespace-qualified symbols.
- **`find_containing_function` returns the *component* when the call is inside a signal handler** (`qml/relationships.rs:283-287`). So `onClicked: { foo() }` produces a relationship from the component to `foo`, not from the handler. The handler context is lost.
- **Pending call filter** (`qml/mod.rs:328-337`): only matches `function_declaration` parents — calls inside property bindings (`text: someFunction()`) won't be tracked as pending even though they should be.

### Razor
**Status**: Needs Work

**Strengths**:
- Comprehensive directive coverage (`razor/mod.rs:62-72, 75-77`): `@page`, `@model`, `@using`, `@inject`, `@inherits`, `@implements`, `@addTagHelper`, `@layout`, `@code`, `@functions`, etc.
- C# code blocks parsed via the same node visitor as the C# extractor (`razor/csharp.rs:30-66`).
- Doc comments handle both `@* *@` (RazorBlock) and C# `///` (TripleSlash) styles via `language_spec/mod.rs:153`.
- `@inject IService PropertyName` correctly extracts `PropertyName` as the symbol (`razor/directives.rs:28-40`).
- Component-tag references emitted in `extract_component_relationships` (`razor/relationships.rs:47-86`).

**Gaps & Errors**:
- **Heavy inline `Regex::new(...).unwrap()` usage** in hot paths: `razor/directives.rs:95, 126, 134, 139, 173, 363`; `razor/relationships.rs:147, 203, 237`; `razor/mod.rs:168, 189`. Each call recompiles the regex per directive parsed. Should be `LazyLock<Regex>`.
- **`@page` mapped to `SymbolKind::Import`** (`razor/directives.rs:152`). It's a routing directive; arguably `Module` or `Namespace`. Same for `@layout` which falls through to the default `Variable`.
- **Symbol names for non-using/inject directives are the directive name itself**. `@page`, `@model`, `@layout` all produce a symbol named `@page`/`@model`/`@layout`. With multiple `@page` directives across files (likely in any sizeable .NET app) every page collides on the same name.
- **Razor is `NO_PENDING_CAPABILITIES`** but `<Component />` references in Razor templates are inherently cross-file (component definitions live in their own .razor files). The synthetic `to_symbol_id = format!("component-{}", name)` (`razor/relationships.rs:75`) is a placeholder that never gets resolved. These should be pending relationships.
- **`@inject` injects a service from another file/assembly**. The dependency is lost; could be a pending relationship.
- **`@bind-Value` / `@bind:Value` / `@onclick`** event/data bindings are scanned via inline regex (`razor/relationships.rs:203, 237`) but produce only "uses" relationships to a synthetic ID, never a real cross-file resolution.
- **`extract_from_text_content`** (`razor/mod.rs:156-208`) only handles `@inherits` and `@rendermode` for ERROR nodes — `@code`, `@functions`, `@inject`, `@using` recovery from broken trees is absent.
- **No `@section X { ... }` body extraction**. Sections are extracted as the directive (`razor/mod.rs:78`) but their content (HTML and inner C#) isn't separately processed.

### SQL
**Status**: Good (with significant bugs)

**Strengths**:
- Comprehensive DDL coverage: tables, views, indexes, triggers, schemas, sequences, domains, types/enums, CTEs, stored procedures, functions, columns, constraints (`sql/mod.rs:135-165`).
- ERROR-node fallback recovery for views, schemas, procedures, parameters (`sql/error_handling.rs`).
- Foreign-key relationship extraction with `isExternal` flag for unresolvable targets (`sql/relationships.rs:108-153`).
- Type inference for column data types via `SQL_TYPE_RE` (`sql/mod.rs:79`).
- SELECT alias / window-function extraction with OVER clause preservation (`sql/views.rs:43-100`).
- SqlLine doc comments (`-- ...`) supported.

**Gaps & Errors**:
- **Self-referential JOIN relationships**: `sql/relationships.rs:181-196` creates a relationship where `from_symbol_id == to_symbol_id` for every joined table. This is nonsensical — a JOIN should connect two distinct tables. As written, every table that appears in a JOIN clause "joins itself".
- **Massive inline `Regex::new(r"...").unwrap()` usage** in hot extractors, recompiling on every call:
  - `sql/schemas.rs:142, 151, 176, 238, 290, 298, 449, 459, 469, 476, 483` — index/schema/domain/sequence extraction
- **CREATE VIEW only extracted via regex** over node text (`sql/schemas.rs:81-115`). The implementation calls `CREATE_VIEW_RE` against the entire `create_view` node text. If the tree-sitter grammar parses the view properly with structured children, none of that information is used — the regex match is the sole extraction path.
- **Foreign keys to tables in OTHER files are dropped**. `sql/relationships.rs:111-153` falls back to `external_{name}` placeholders. Because SQL is `NO_PENDING_CAPABILITIES`, these never resolve. A multi-file schema (one file per table, common in Flyway/Liquibase migrations) loses all cross-file FKs.
- **No view-to-table dependencies**. A `CREATE VIEW v AS SELECT * FROM t1 JOIN t2` doesn't produce relationships from `v` to `t1` or `t2`. Comment in `sql/relationships.rs:31-34` admits this was a stub that "found table names but never created relationships".
- **No INDEX -> COLUMN relationships**. `CREATE INDEX idx ON t(col1, col2)` doesn't link the index to those columns even though both are extracted as symbols.
- **TRIGGER -> TABLE relationship missing**. Triggers reference tables via `ON tablename` but no relationship is created.
- **Indexes are `SymbolKind::Property`** (`sql/schemas.rs:197`). Mismatch with class-like signature.
- **CTEs are `SymbolKind::Interface`** (`sql/schemas.rs:421`). Plausible but inconsistent with views (`Interface`) and tables (`Class`); both views and CTEs use Interface. Should at least have differentiating metadata.
- **Schema regex doesn't handle quoted/case-variant CREATE SCHEMA**. `sql/schemas.rs:238` uses inline regex `CREATE\s+SCHEMA\s+([a-zA-Z_]...)` — won't match `create schema "public"` or `CREATE SCHEMA IF NOT EXISTS x`.
- **Column constraints lose their target tables**. A `FOREIGN KEY (uid) REFERENCES users(id)` produces a relationship at the constraint level (`sql/relationships.rs:48-153`), but column-level inline FKs (`uid INT REFERENCES users(id)`) might not.
- **Doc comment style overlap**: SQL `--` and Lua `--` are both registered. Scoped per-language so it works, but a multi-language pipeline reading raw file text would conflate them.

### Regex
**Status**: Good

**Strengths**:
- Smart filtering: only meaningful nodes (named groups, anonymous groups referenced by backref, character classes, lookarounds, unicode properties, conditionals) become symbols (`regex/mod.rs:53-119`). Noise (literals, anchors, quantifiers, alternation) is correctly skipped.
- Capture index tracking (`regex/mod.rs:174-179`) so `\1`, `\2` references can be resolved.
- "Skip-through parenting" so a `character_class` inside a non-capturing group correctly inherits its grandparent's id (`regex/mod.rs:127-131`).

**Gaps & Errors**:
- **No cross-file capture references**: `NO_PENDING_CAPABILITIES`. Modular regex (e.g., split across YAML config files) can't link backreferences across files. Probably acceptable since real cross-file regex is rare.
- **Tree-sitter-regex's grammar might not expose `predefined_character_class` consistently across regex flavors**. The current extractor skips them entirely (`regex/mod.rs:108`) — this is fine for symbols but means `\d`, `\w`, etc. aren't surfaced even as identifiers.
- **`infer_types` returns `regex:pattern` for all Variable symbols**. Not very informative.

### Markdown
**Status**: Good

**Strengths**:
- Section symbols include their full body content as `doc_comment` for RAG embedding (`markdown/mod.rs:240-264`).
- Frontmatter (YAML and TOML) handled with body-after-frontmatter capture (`markdown/mod.rs:84-171`).
- Local heading link relationships with proper slug normalization (`markdown/relationships.rs:104-121`).
- Self-referential link suppression (`markdown/relationships.rs:64-66`).

**Gaps & Errors**:
- **Heading level computed but discarded**: `markdown/mod.rs:296` (`let _level = ...`). All headings become `SymbolKind::Module`. Storing level in metadata would let consumers distinguish h1 from h6.
- **No code-block extraction with language tag**. ` ```rust ... ``` ` could be a `CodeBlock` symbol with language metadata, useful for pulling out runnable examples.
- **No image, footnote, or link-to-other-file relationships**. `[text](other.md#section)` and `![alt](image.png)` are invisible.
- **`extract_identifiers` returns empty unconditionally** (`markdown/mod.rs:344-347`). Even local `[link](#anchor)` could be an identifier — would let find_references work on heading anchors.
- **Cross-file links can't resolve**. `RELATIONSHIP_DATA_CAPABILITIES` lacks pending. `[link](other.md#section)` references could be pending and resolvable if the target file is indexed.
- **Slug normalization is custom** (`markdown/relationships.rs:104-121`). It mostly works but doesn't match GitHub's hash function exactly (e.g., handling of emoji, repeated punctuation, leading numbers). May produce false negatives on real-world links.

### JSON
**Status**: Good (within profile)

**Strengths**:
- Object/array values become `Module`, primitives become `Variable` (`json/mod.rs:86-89`) — coherent kind assignment.
- String values up to 2000 chars captured as `doc_comment` for semantic search (`json/mod.rs:92-109`).

**Gaps & Errors**:
- **No signature for any pair**. Even simple `"port": 8080` could have a signature `port: 8080` for at-a-glance reading. JSON pair signatures are always `None`.
- **String truncation uses byte slicing**: `trimmed[..2000]` (`json/mod.rs:101-103`) panics on multi-byte UTF-8 if char boundary doesn't fall on byte 2000. TOML version (`toml/mod.rs:155-158`) uses `chars().take(2000)` correctly — fix JSON to match.
- **`if children.len() < 3`** check is fragile. Some grammar shapes might have 2-child pairs (key + value, no explicit colon node). Use field-based access instead.
- **No `extract_relationships`**. JSON often references other files (e.g., `"$ref": "..."` in JSON Schema, `"extends": "../base.json"` in tsconfig, `"path"` in launch.json). The DATA_ONLY_CAPABILITIES profile blocks even relationships, but pattern-matching and synthetic relationships could be useful.
- **`extract_identifiers` returns empty** (`json/mod.rs:130-133`). String values referencing other JSON keys (e.g., schema `$ref` paths, package.json `"main"` -> file) are invisible.
- **Quoted-key handling is brittle**: `key_text.trim_matches('"')` (`json/mod.rs:80`) strips both outer and inner quotes. Generally fine but fails on keys containing escaped quotes.

### TOML
**Status**: Good (within profile)

**Strengths**:
- Differentiates `table` from `table_array_element` (`toml/mod.rs:60-63`) at the visitor level.
- UTF-8-safe truncation via `chars().take(...)` (`toml/mod.rs:155-158`).
- Signature includes `"key = value"` with proper truncation (`toml/mod.rs:138-147`).

**Gaps & Errors**:
- **`_is_array` parameter ignored** (`toml/mod.rs:73`). Both `[table]` and `[[array_table]]` produce identical metadata — no `is_array_table: true` flag, no different signature.
- **`extract_identifiers` returns empty** (`toml/mod.rs:204-211`). Cargo.toml's `path = "../foo"` could be an identifier referencing another package.
- **No relationships**. Cargo.toml dependency declarations, pyproject.toml URLs, etc. are inert.
- **Dotted keys preserved as text** but not exploded into a path. `toml/mod.rs:122-125` returns the full dotted key as the symbol name, so searching for the leaf name requires substring matching.
- **No signature for tables**. `[server]` produces a Module symbol with no signature, even though "server" is meaningful by itself.

### YAML
**Status**: Good

**Strengths**:
- Anchors (`&name`) captured in metadata; aliases (`*name`) emit `VariableRef` identifiers with resolved `target_symbol_id` (`yaml/mod.rs:251-263`).
- Container vs leaf distinction via `has_nested_mapping` (`yaml/mod.rs:159-172`).
- Skips merge keys (`<<:`) correctly.
- Proper relationship emission for alias->anchor pairs (`yaml/relationships.rs:41-80`).

**Gaps & Errors**:
- **No tag handling (`!!str`, `!Custom`)**. Tagged values are common in K8s manifests and could be extracted as type info.
- **No multi-document support (`---` separators)**. `yaml_specs.rs` doesn't note this; multi-document YAML files (common in K8s) might collapse documents.
- **No flow mapping (`{key: value}`) extraction**. Comments in `yaml/mod.rs:78-79` say it's intentional ("noise"), but K8s and CI YAML use flow mappings for tags/labels frequently.
- **Leaf values not in signature**. `host: localhost` produces a Variable named "host" with NO signature, so the value "localhost" is invisible without reading raw content.
- **No cross-file references**. GitHub Actions `uses: actions/checkout@v3`, K8s `kind`/`apiVersion`/`metadata.name` references between resources, Docker Compose `extends` clauses are all invisible.
- **`anchor_name_from_signature` parses signature text** (`yaml/mod.rs:294-306`) — fallback when metadata is missing. This is fragile; metadata is the canonical source.

## Cross-Cutting Patterns

**1. Inline regex compilation in hot paths.**
SQL (`sql/schemas.rs` ~10 sites), Razor (`razor/directives.rs`, `razor/relationships.rs` ~8 sites), HTML (`html/attributes.rs:222`), and CSS already mostly use `LazyLock`. A pass to extract all inline `Regex::new` calls into static `LazyLock<Regex>` constants would be a measurable perf win for SQL-heavy or Razor-heavy codebases.

**2. Symbol naming collisions for repeated tokens.**
HTML elements named by tag, CSS at-rules named by keyword, Razor directives named by directive name. Each collides on common values. Solution: prefer the most specific identifier (id, name, URL, target type), fallback to the keyword. The metadata is rich enough that the search index could disambiguate, but the *symbol name* should be unique within a file.

**3. `NO_PENDING_CAPABILITIES` is too aggressive for HTML, Razor, SQL.**
HTML has cross-file `<script src>`, `<img src>`, `<a href>`. Razor has `@inject Service`, `<Component />`. SQL has `FOREIGN KEY REFERENCES` to other files in migration sets. All of these are real cross-file relationships and could be `pending_relationships` like other languages. The current behavior creates synthetic placeholder IDs (`url:foo`, `external_users`, `component-MyComp`) that never resolve and pollute the relationship graph with dead targets.

**4. Embedded language extraction is inconsistent.**
- Vue: `<script>` regex, `<script setup>` tree-sitter, `<style>` regex.
- Razor: HTML and C# both via tree-sitter (good).
- HTML: `<script>` and `<style>` content not parsed at all.
- Markdown: fenced code blocks not parsed; could embed a Rust/Python parser per language tag.

A standard "embedded language" framework (parse range with appropriate parser, offset positions) would unify Vue/Razor/HTML and enable Markdown code-block extraction.

**5. Manual symbol creation skips byte ranges.**
Vue's `create_symbol_manual` (`vue/script.rs:177-226`) sets `start_byte: 0, end_byte: 0`. Anything that depends on byte ranges (code-context extraction, snippet generation, character-level features) won't work for these symbols.

**6. Test coverage doesn't catch behavioral bugs.**
SQL JOIN test (`tests/sql/relationships.rs:74-78`) checks only `!join_relations.is_empty()`. It doesn't verify the from/to are different tables. Vue tests verify the wrapper symbol exists (`props`, `emit`) but not the inner names. A test bar that asserts on relationship semantics — not just counts — would catch the self-referential JOIN bug, the Vue prop-omission, and HTML duplicate-tag-name issues.

**7. File size violations.**
Implementation files exceeding 500 lines: `sql/routines.rs` (521), `sql/schemas.rs` (507). The CLAUDE.md target is 500. These are close but should be refactored when next touched (per project standards).

## Top 10 Highest-Impact Findings (ranked)

1. **SQL self-referential JOIN bug** (`sql/relationships.rs:181-196`). Every JOIN produces `from_symbol_id == to_symbol_id`. Fix: track the FROM-table from the enclosing `select_statement` / `from_clause` and emit a real edge between the FROM table and each joined table.

2. **Vue Options API loses all method/prop/computed names** (`vue/script.rs:42-85`). Only the wrapping object key is extracted. Fix: parse the script section as JS, walk the exported object's `data`, `methods`, `computed`, `props`, `watch`, `setup` properties and extract each member.

3. **Vue identifier extraction reparses SFC per identifier** (`vue/identifiers.rs:185-225`). O(N) reparses per file. Fix: parse the SFC once, pass the script section content/offset down, use byte-relative addressing.

4. **HTML/CSS/Razor symbol-name collisions**. Every `<a>`, `@import`, `@page` in a file shares the same name. Fix: derive symbol names from the most specific available identifier (id/name/target/URL) with fallback to the tag/keyword.

5. **SQL massive inline `Regex::new` recompilation**. ~10 sites in `sql/schemas.rs` alone. Fix: hoist all to `LazyLock<Regex>`.

6. **`NO_PENDING_CAPABILITIES` is wrong for HTML, Razor, and SQL.** Cross-file references in `<script src>`, `@inject`, `FOREIGN KEY REFERENCES` are common and could be pending. Fix: bump these to `RELATIONSHIP_DATA_CAPABILITIES + pending_relationships`, then synthesize pending instead of `external_*` placeholders.

7. **Vue template never produces definitions**, despite being where Vue components are wired together (`vue/mod.rs:172-176`). Template refs (`ref="x"`), `v-model`, slot definitions need to become symbols. The pending-component-tag handling shows the parsing is feasible.

8. **Embedded `<script>`/`<style>` in HTML never parsed**. JS in HTML is a major source of code; CSS in `<style>` similarly. Fix: invoke the JS/CSS extractors on the inner ranges with offset adjustment (Vue and Razor already prove this is doable).

9. **SQL view-to-table and trigger-to-table relationships missing entirely**. Comments in `sql/relationships.rs:31-34` admit a previous stub did nothing. Fix: when extracting CREATE VIEW or CREATE TRIGGER, walk the body for `FROM t1`, `JOIN t2`, `ON tablename` and emit `Uses` relationships.

10. **Markdown discards heading level and ignores code blocks**. Two cheap improvements: store `heading_level` in metadata (`markdown/mod.rs:296`) and emit fenced code blocks as `CodeBlock`-kind symbols with the language tag. Both unlock real downstream features (TOC generation, runnable example extraction) and are tiny diffs.
