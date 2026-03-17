# Scala & Elixir Language Support Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Scala and Elixir as supported languages (#28 and #29), bringing Julie to 33 language entries (29 languages + tsx/jsx/markdown/json aliases).

**Architecture:** Each language gets a tree-sitter extractor module under `crates/julie-extractors/src/{language}/` with 6-7 files (mod.rs, declarations/calls, types/attributes, helpers, properties, relationships, identifiers). Both languages wire into 10 shared registration points. Scala uses the Kotlin extractor as template (rich named AST nodes). Elixir uses a novel call-dispatch pattern inspired by Ruby's `calls.rs` (all definitions are generic `call` nodes requiring semantic inspection of call targets).

**Tech Stack:** Rust, tree-sitter 0.25, `tree-sitter-scala` (crates.io), `tree-sitter-elixir` (crates.io)

---

## Context

GPT's external review of Julie noted that Scala and Elixir are popular languages missing from the 31-language roster. Both have mature tree-sitter grammars with confirmed compatibility (`tree-sitter-language = "0.1"` interop layer). Scala is structurally similar to Kotlin (JVM sibling), making it straightforward. Elixir's flat grammar where everything is `call` nodes requires a different extraction strategy.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Execution order | Scala first, then Elixir | Scala mirrors Kotlin closely; Elixir needs novel call-dispatch pattern |
| Scala template | Kotlin extractor | JVM sibling, nearly identical AST structure |
| Elixir template | Ruby `calls.rs` pattern | Both dispatch on call target names to identify definitions |
| `defprotocol` mapping | `SymbolKind::Interface` | Protocols are behavioral contracts (like Java interfaces) |
| `defimpl` mapping | `SymbolKind::Class` + metadata `"protocol_impl"` | Implementation of a protocol for a type |
| `defmodule` mapping | `SymbolKind::Module` | Elixir modules are namespaces, not classes |
| `given_definition` (Scala 3) | `SymbolKind::Variable` + metadata `"given"` | Given instances are values, not types |
| `extension_definition` (Scala 3) | `SymbolKind::Function` + metadata `"extension"` | Extensions define methods on types |
| Elixir multi-clause `def` | Each clause = separate symbol | Consistent with how overloads are handled elsewhere |
| Elixir `properties.rs` | Replace with `attributes.rs` | No classes/fields; module attributes serve a different role |

## File Structure

### New Files — Scala (7 files)

```
crates/julie-extractors/src/scala/
├── mod.rs           (~200 lines) — ScalaExtractor struct, visit_node dispatch
├── declarations.rs  (~350 lines) — extract_function, extract_package, extract_import, extract_type_alias, extract_given, extract_extension
├── types.rs         (~300 lines) — extract_class, extract_trait, extract_object, extract_enum, extract_package_object
├── helpers.rs       (~300 lines) — extract_modifiers, extract_type_parameters, extract_parameters, extract_return_type, extract_extends
├── properties.rs    (~200 lines) — extract_val, extract_var, class parameters
├── relationships.rs (~350 lines) — extract_inheritance_relationships, extract_call_relationships
└── identifiers.rs   (~150 lines) — walk tree for identifier references
```

### New Files — Elixir (7 files)

```
crates/julie-extractors/src/elixir/
├── mod.rs           (~250 lines) — ElixirExtractor struct, visit_node dispatch (call + unary_operator)
├── calls.rs         (~400 lines) — dispatch_call: defmodule, def/defp, defmacro, defprotocol, defimpl, defstruct, import/use/alias/require
├── attributes.rs    (~200 lines) — extract_module_attribute: @doc, @spec, @type, @callback, @behaviour
├── helpers.rs       (~200 lines) — extract_function_head, extract_module_name, extract_do_block, extract_guard_clause
├── relationships.rs (~250 lines) — @behaviour→Implements, use→Uses, defimpl→Implements, function calls
├── identifiers.rs   (~150 lines) — walk tree for call/dot/alias identifier references
└── types_inference.rs (~100 lines) — infer types from @spec annotations
```

### New Test Files

```
src/tests/extractors/scala.rs    — Scala extractor tests
src/tests/extractors/elixir.rs   — Elixir extractor tests
fixtures/scala/basic.scala       — Scala test fixture
fixtures/elixir/basic.ex         — Elixir test fixture
```

### Modified Files (10 registration points + 2 test files)

| # | File | Change |
|---|------|--------|
| 1 | `crates/julie-extractors/Cargo.toml` | Add `tree-sitter-scala` and `tree-sitter-elixir` deps |
| 2 | `crates/julie-extractors/src/lib.rs` | Add `pub mod scala;` and `pub mod elixir;` |
| 3 | `crates/julie-extractors/src/language.rs` | 6 match arms per language (12 total additions) |
| 4 | `crates/julie-extractors/src/factory.rs` | 2 match arms + update test count 29→31 |
| 5 | `crates/julie-extractors/src/manager.rs` | Add both to `supported_languages()` |
| 6 | `crates/julie-extractors/src/routing_symbols.rs` | 2 match arms |
| 7 | `crates/julie-extractors/src/routing_relationships.rs` | 2 match arms |
| 8 | `crates/julie-extractors/src/routing_identifiers.rs` | 2 match arms |
| 9 | `crates/julie-extractors/src/test_detection.rs` | Add `"scala"` to JVM arm + new `detect_elixir()` |
| 10 | `src/tools/refactoring/utils.rs` | 2 extension mappings |
| 11 | `src/tests/core/language.rs` | Update language count 27→29, add to vec |
| 12 | `src/tests/mod.rs` or `src/tests/extractors/mod.rs` | Register new test modules |

## Key Reference Files

- `crates/julie-extractors/src/kotlin/` — Scala template (all 7 files)
- `crates/julie-extractors/src/ruby/calls.rs` — Elixir call-dispatch template
- `crates/julie-extractors/src/base/creation_methods.rs` — `create_symbol()` API
- `crates/julie-extractors/src/base/types.rs` — `SymbolKind`, `Visibility`, `SymbolOptions`

---

## Chunk 1: Scala Language Support

### Task 1: Add tree-sitter-scala dependency and verify compilation

**Files:**
- Modify: `crates/julie-extractors/Cargo.toml`

- [ ] **Step 1: Add dependency**
  ```toml
  tree-sitter-scala = "0.25"
  ```

- [ ] **Step 2: Verify it compiles**
  Run: `cargo build -p julie-extractors 2>&1 | tail -5`
  Expected: successful compilation

- [ ] **Step 3: Commit**
  `feat(scala): add tree-sitter-scala dependency`

### Task 2: Create Scala extractor skeleton + registration

**Files:**
- Create: `crates/julie-extractors/src/scala/mod.rs` (skeleton with empty extract methods)
- Create: `crates/julie-extractors/src/scala/declarations.rs` (empty)
- Create: `crates/julie-extractors/src/scala/types.rs` (empty)
- Create: `crates/julie-extractors/src/scala/helpers.rs` (empty)
- Create: `crates/julie-extractors/src/scala/properties.rs` (empty)
- Create: `crates/julie-extractors/src/scala/relationships.rs` (empty)
- Create: `crates/julie-extractors/src/scala/identifiers.rs` (empty)
- Modify: All 10 registration files (see registration checklist above)
- Modify: `src/tests/core/language.rs` — add `"scala"`, update count 27→28

- [ ] **Step 1: Create `mod.rs` skeleton**

  Follow Kotlin's pattern exactly:
  ```rust
  pub struct ScalaExtractor {
      base: BaseExtractor,
      pending_relationships: Vec<PendingRelationship>,
  }
  ```
  With `new()`, `extract_symbols()` (returns empty vec), `visit_node()` (empty match), `infer_types()`, `extract_relationships()`, `extract_identifiers()`, `get_pending_relationships()`.

- [ ] **Step 2: Create empty sub-module files**

  Each file just needs imports and empty pub(super) functions matching the pattern from Kotlin.

- [ ] **Step 3: Wire into all 10 registration points**

  Key entries:
  - `language.rs`: `"scala" => Ok(tree_sitter_scala::LANGUAGE.into())`
  - `language.rs`: `"scala" | "sc" => Some("scala")`
  - `language.rs`: `get_function_node_kinds("scala")` → `vec!["function_definition", "function_declaration"]`
  - `language.rs`: `get_import_node_kinds("scala")` → `vec!["import_declaration"]`
  - `language.rs`: `get_symbol_node_kinds("scala")` → `vec!["function_definition", "class_definition", "object_definition", "trait_definition", "enum_definition", "type_definition"]`
  - `language.rs`: Add `"scala"` to the `"name"` arm of `get_symbol_name_field`
  - `test_detection.rs`: Add `"scala"` to the `"java" | "kotlin"` arm
  - `refactoring/utils.rs`: `Some("scala") | Some("sc") => "scala".to_string()`

- [ ] **Step 4: Update test counts**

  - `language.rs` test: 27→28 in vec + assertion
  - `factory.rs` test: 29→30 in assertion
  - `manager.rs` vec: add `"scala"`

- [ ] **Step 5: Verify compilation and factory test**
  Run: `cargo test --lib test_all_languages_in_factory 2>&1 | tail -10`
  Expected: PASS (empty extractor returns 0 symbols, but no "No extractor available" error)

- [ ] **Step 6: Commit**
  `feat(scala): scaffold extractor module and wire registration`

### Task 3: Scala symbol extraction — classes, traits, objects, enums

**Files:**
- Modify: `crates/julie-extractors/src/scala/types.rs`
- Modify: `crates/julie-extractors/src/scala/helpers.rs`
- Modify: `crates/julie-extractors/src/scala/mod.rs` (add visit_node dispatch)
- Create: `src/tests/extractors/scala.rs` (or appropriate test location)
- Create: `fixtures/scala/basic.scala`

- [ ] **Step 1: Write test fixture `fixtures/scala/basic.scala`**
  ```scala
  package com.example

  sealed trait Animal {
    def speak(): String
  }

  case class Dog(name: String) extends Animal {
    override def speak(): String = s"Woof! I'm $name"
  }

  object DogFactory {
    def create(name: String): Dog = Dog(name)
  }

  enum Color {
    case Red, Green, Blue
    case Custom(hex: String)
  }

  abstract class Shape(val sides: Int) {
    def area(): Double
  }
  ```

- [ ] **Step 2: Write failing test for class/trait/object/enum extraction**

  Parse the fixture, run `extract_symbols()`, assert:
  - `Animal` → Trait
  - `Dog` → Class (case class)
  - `DogFactory` → Class (object) with metadata `"type": "object"`
  - `Color` → Enum
  - `Red`, `Green`, `Blue`, `Custom` → EnumMember
  - `Shape` → Class (abstract)

- [ ] **Step 3: Run test — verify RED**
  Run: `cargo test --lib test_scala_class_extraction 2>&1 | tail -10`

- [ ] **Step 4: Implement `helpers.rs`** — `extract_modifiers()`, `extract_type_parameters()`, `extract_extends()`, `determine_visibility()`

- [ ] **Step 5: Implement `types.rs`** — `extract_class()`, `extract_trait()`, `extract_object()`, `extract_enum()`

  Scala AST node mapping:
  - `class_definition` → `SymbolKind::Class`. Check modifiers for `case`, `abstract`, `sealed`.
  - `trait_definition` → `SymbolKind::Trait`
  - `object_definition` → `SymbolKind::Class` with metadata `"type": "object"`. If companion (name matches a class), add metadata `"companion": true`.
  - `enum_definition` → `SymbolKind::Enum`. Extract `simple_enum_case` and `full_enum_case` children as `SymbolKind::EnumMember`.

- [ ] **Step 6: Wire dispatch in `mod.rs` `visit_node()`**
  ```rust
  match node.kind() {
      "class_definition" => types::extract_class(&mut self.base, &node, parent_id.as_deref()),
      "trait_definition" => types::extract_trait(&mut self.base, &node, parent_id.as_deref()),
      "object_definition" => types::extract_object(&mut self.base, &node, parent_id.as_deref()),
      "enum_definition" => types::extract_enum(&mut self.base, &node, parent_id.as_deref()),
      // ...
  }
  ```

- [ ] **Step 7: Run test — verify GREEN**

- [ ] **Step 8: Commit**
  `feat(scala): implement class, trait, object, and enum extraction`

### Task 4: Scala symbol extraction — functions, imports, packages

**Files:**
- Modify: `crates/julie-extractors/src/scala/declarations.rs`
- Modify: `crates/julie-extractors/src/scala/helpers.rs` (add `extract_parameters`, `extract_return_type`)
- Modify: `crates/julie-extractors/src/scala/mod.rs` (extend visit_node)
- Modify: test file

- [ ] **Step 1: Write failing test for function/import/package extraction**

  Test fixture additions:
  ```scala
  import scala.collection.mutable.{ListBuffer => LB, _}
  def factorial(n: Int): Int = if (n <= 1) 1 else n * factorial(n - 1)
  ```

  Assert:
  - `speak` → Function (from trait), `speak` → Method (from class override)
  - `create` → Function/Method
  - `factorial` → Function
  - `area` → Function (abstract)
  - `import scala.collection.mutable...` → Import
  - `package com.example` → Namespace

- [ ] **Step 2: Run test — verify RED**

- [ ] **Step 3: Implement `declarations.rs`** — `extract_function()`, `extract_import()`, `extract_package()`

  Scala function AST: `function_definition` has `name`, `parameters`, `return_type`, `body` fields.
  - If inside a class/trait/object body → `SymbolKind::Method`
  - If top-level or in package → `SymbolKind::Function`
  - Build signature: `def name(params): ReturnType`

- [ ] **Step 4: Implement `helpers.rs`** additions — `extract_parameters()`, `extract_return_type()`

- [ ] **Step 5: Wire in `mod.rs`**: Add `"function_definition"`, `"function_declaration"`, `"import_declaration"`, `"package_clause"` to visit_node.

- [ ] **Step 6: Run test — verify GREEN**

- [ ] **Step 7: Commit**
  `feat(scala): implement function, import, and package extraction`

### Task 5: Scala val/var properties + type aliases + Scala 3 features

**Files:**
- Modify: `crates/julie-extractors/src/scala/properties.rs`
- Modify: `crates/julie-extractors/src/scala/declarations.rs` (add type_alias, given, extension)
- Modify: `crates/julie-extractors/src/scala/mod.rs` (extend visit_node)
- Modify: test file

- [ ] **Step 1: Write failing test**

  ```scala
  val pi: Double = 3.14159
  var count: Int = 0
  lazy val config: Config = loadConfig()
  type StringList = List[String]
  given Ordering[Dog] with { ... }
  extension (s: String) { def greet: String = s"Hello, $s" }
  ```

  Assert symbols for `pi` (Constant), `count` (Variable), `config` (Variable, lazy), `StringList` (Type), given (Variable, metadata "given"), extension (Function, metadata "extension").

- [ ] **Step 2: Run test — verify RED**

- [ ] **Step 3: Implement `properties.rs`** — `extract_val()`, `extract_var()`
  - `val_definition` → `SymbolKind::Constant` (immutable). If `lazy` modifier → add to metadata.
  - `var_definition` → `SymbolKind::Variable`

- [ ] **Step 4: Implement `declarations.rs`** additions — `extract_type_alias()`, `extract_given()`, `extract_extension()`

- [ ] **Step 5: Wire in `mod.rs`**: Add `"val_definition"`, `"var_definition"`, `"val_declaration"`, `"var_declaration"`, `"type_definition"`, `"given_definition"`, `"extension_definition"` to visit_node.

- [ ] **Step 6: Run test — verify GREEN**

- [ ] **Step 7: Commit**
  `feat(scala): implement val/var, type alias, given, and extension extraction`

### Task 6: Scala relationships + identifiers

**Files:**
- Modify: `crates/julie-extractors/src/scala/relationships.rs`
- Modify: `crates/julie-extractors/src/scala/identifiers.rs`
- Modify: test file

- [ ] **Step 1: Write failing test for relationships**

  From the basic.scala fixture, assert:
  - `Dog` extends `Animal` → Extends relationship
  - `DogFactory.create` calls `Dog` constructor → Calls relationship
  - `Shape.area` is abstract → no body, still extracted

- [ ] **Step 2: Run test — verify RED**

- [ ] **Step 3: Implement `relationships.rs`**

  Follow Kotlin pattern:
  - `extract_inheritance_relationships()`: Walk `extends_clause` / `derives_clause` to create Extends/Implements relationships
  - `extract_call_relationships()`: Walk `call_expression` nodes, resolve to local symbols or create `PendingRelationship`

- [ ] **Step 4: Implement `identifiers.rs`**

  Walk tree for `call_expression` (Call kind), `field_expression` (MemberAccess kind), `identifier` in type positions (TypeUsage kind).

- [ ] **Step 5: Run test — verify GREEN**

- [ ] **Step 6: Run `cargo xtask test dev` — verify no regressions**

- [ ] **Step 7: Commit**
  `feat(scala): implement relationship and identifier extraction`

### Task 7: Scala type inference

**Files:**
- Modify: `crates/julie-extractors/src/scala/mod.rs` (`infer_types()`)
- Modify: test file

- [ ] **Step 1: Write failing test for type inference**

  Assert `infer_types()` returns type mappings for:
  - Functions with explicit return types
  - Val/var with explicit type annotations

- [ ] **Step 2: Implement `infer_types()`** — extract from signatures and explicit type annotations

- [ ] **Step 3: Run test — verify GREEN**

- [ ] **Step 4: Commit**
  `feat(scala): implement type inference`

---

## Chunk 2: Elixir Language Support

### Task 8: Add tree-sitter-elixir dependency and verify compilation

**Files:**
- Modify: `crates/julie-extractors/Cargo.toml`

- [ ] **Step 1: Add dependency**
  ```toml
  tree-sitter-elixir = "0.3"
  ```

- [ ] **Step 2: Verify it compiles**
  Run: `cargo build -p julie-extractors 2>&1 | tail -5`

- [ ] **Step 3: Commit**
  `feat(elixir): add tree-sitter-elixir dependency`

### Task 9: Create Elixir extractor skeleton + registration

**Files:**
- Create: `crates/julie-extractors/src/elixir/mod.rs`
- Create: `crates/julie-extractors/src/elixir/calls.rs`
- Create: `crates/julie-extractors/src/elixir/attributes.rs`
- Create: `crates/julie-extractors/src/elixir/helpers.rs`
- Create: `crates/julie-extractors/src/elixir/relationships.rs`
- Create: `crates/julie-extractors/src/elixir/identifiers.rs`
- Create: `crates/julie-extractors/src/elixir/types_inference.rs`
- Modify: All 10 registration files

- [ ] **Step 1: Create `mod.rs` skeleton**

  ```rust
  pub struct ElixirExtractor {
      base: BaseExtractor,
      pending_relationships: Vec<PendingRelationship>,
      module_stack: Vec<String>,  // Track nested defmodule nesting
  }
  ```

  Key difference from other extractors: `visit_node` primarily dispatches on `"call"` nodes:
  ```rust
  match node.kind() {
      "call" => calls::dispatch_call(self, &node, parent_id.as_deref()),
      "unary_operator" => attributes::extract_module_attribute(self, &node, parent_id.as_deref()),
      _ => { /* recurse children */ }
  }
  ```

- [ ] **Step 2: Create empty sub-module files**

- [ ] **Step 3: Wire into all 10 registration points**

  Key entries:
  - `language.rs`: `"elixir" => Ok(tree_sitter_elixir::LANGUAGE.into())`
  - `language.rs`: `"ex" | "exs" => Some("elixir")`
  - `language.rs`: `get_function_node_kinds("elixir")` → `vec!["call"]`
  - `language.rs`: `get_import_node_kinds("elixir")` → `vec!["call"]`
  - `language.rs`: `get_symbol_node_kinds("elixir")` → `vec!["call"]`
  - `test_detection.rs`: Add `"elixir" => detect_elixir(name, file_path)` — detect by `test_` prefix or `test/` directory path
  - `refactoring/utils.rs`: `Some("ex") | Some("exs") => "elixir".to_string()`

- [ ] **Step 4: Update test counts** (from 28→29 and 30→31)

- [ ] **Step 5: Verify compilation and factory test**

- [ ] **Step 6: Commit**
  `feat(elixir): scaffold extractor module and wire registration`

### Task 10: Elixir call dispatch — defmodule + def/defp

**Files:**
- Modify: `crates/julie-extractors/src/elixir/calls.rs`
- Modify: `crates/julie-extractors/src/elixir/helpers.rs`
- Modify: `crates/julie-extractors/src/elixir/mod.rs`
- Create: `fixtures/elixir/basic.ex`
- Create: `src/tests/extractors/elixir.rs`

- [ ] **Step 1: Write test fixture `fixtures/elixir/basic.ex`**
  ```elixir
  defmodule MyApp.Calculator do
    @moduledoc "A simple calculator"

    @doc "Adds two numbers"
    def add(a, b), do: a + b

    defp validate(n) when is_number(n), do: n
    defp validate(_), do: raise("not a number")

    def multiply(a, b) do
      validate(a)
      validate(b)
      a * b
    end
  end
  ```

- [ ] **Step 2: Write failing test**

  Assert:
  - `MyApp.Calculator` → Module
  - `add` → Function (public)
  - `validate` → Function (private, two clauses = two symbols)
  - `multiply` → Function (public)

- [ ] **Step 3: Run test — verify RED**

- [ ] **Step 4: Implement `helpers.rs`** — `extract_call_target_name()`, `extract_function_head()`, `extract_module_name()`, `extract_do_block()`

  The call-target extraction pattern:
  ```rust
  pub fn extract_call_target_name(base: &BaseExtractor, node: &Node) -> Option<String> {
      // The target field of a call node is the function being called
      let target = node.child_by_field_name("target")?;
      match target.kind() {
          "identifier" => Some(base.get_node_text(&target).to_string()),
          "dot" => { /* qualified call: Module.function */ }
          _ => None,
      }
  }
  ```

- [ ] **Step 5: Implement `calls.rs`** — `dispatch_call()`, `extract_defmodule()`, `extract_def()`

  Dispatch pattern (inspired by Ruby `calls.rs`):
  ```rust
  pub fn dispatch_call(extractor: &mut ElixirExtractor, node: &Node, parent_id: Option<&str>) -> Option<Symbol> {
      let target_name = helpers::extract_call_target_name(&extractor.base, node)?;
      match target_name.as_str() {
          "defmodule" => extract_defmodule(extractor, node, parent_id),
          "def" => extract_def(extractor, node, parent_id, Visibility::Public),
          "defp" => extract_def(extractor, node, parent_id, Visibility::Private),
          // ... more in later tasks
          _ => None,
      }
  }
  ```

  For `defmodule`: arguments contain an `alias` node (module name) + `do_block` (body). Extract alias text, create Module symbol, recursively visit do_block children.

  For `def`/`defp`: first argument is itself a `call` node (the function head). Extract name from that call's target, parameters from its arguments.

- [ ] **Step 6: Wire dispatch in `mod.rs` `visit_node()`**

- [ ] **Step 7: Run test — verify GREEN**

- [ ] **Step 8: Commit**
  `feat(elixir): implement defmodule and def/defp extraction`

### Task 11: Elixir call dispatch — defmacro, defprotocol, defimpl, defstruct

**Files:**
- Modify: `crates/julie-extractors/src/elixir/calls.rs` (extend dispatch)
- Modify: test file + fixture

- [ ] **Step 1: Extend test fixture**
  ```elixir
  defprotocol Printable do
    @doc "Prints to string"
    def to_string(data)
  end

  defimpl Printable, for: Integer do
    def to_string(n), do: Integer.to_string(n)
  end

  defmodule MyApp.User do
    defstruct [:name, :email, :age]

    defmacro validate_field(field) do
      quote do: is_binary(unquote(field))
    end
  end
  ```

- [ ] **Step 2: Write failing test**

  Assert:
  - `Printable` → Interface (protocol)
  - `Printable.to_string` → Function
  - `Printable for Integer` → Class with metadata `"protocol_impl"`
  - `MyApp.User` → Module
  - `MyApp.User struct` → Struct with fields `name`, `email`, `age`
  - `validate_field` → Function with metadata `"macro": true`

- [ ] **Step 3: Run test — verify RED**

- [ ] **Step 4: Implement** `extract_defprotocol()`, `extract_defimpl()`, `extract_defstruct()`, `extract_defmacro()`

  - `defprotocol`: Similar to `defmodule` but creates `SymbolKind::Interface`
  - `defimpl`: Extract protocol name + `for:` type from arguments. Create `SymbolKind::Class` with metadata.
  - `defstruct`: Extract field names from list argument. Create `SymbolKind::Struct` + `SymbolKind::Field` children.
  - `defmacro`/`defmacrop`: Same as `def`/`defp` but with metadata `"macro": true`

- [ ] **Step 5: Run test — verify GREEN**

- [ ] **Step 6: Commit**
  `feat(elixir): implement defmacro, defprotocol, defimpl, defstruct extraction`

### Task 12: Elixir imports and module attributes

**Files:**
- Modify: `crates/julie-extractors/src/elixir/calls.rs` (add import/use/alias/require)
- Modify: `crates/julie-extractors/src/elixir/attributes.rs`
- Modify: test file + fixture

- [ ] **Step 1: Extend test fixture**
  ```elixir
  defmodule MyApp.Service do
    use GenServer
    import Enum, only: [map: 2, filter: 2]
    alias MyApp.{User, Calculator}
    require Logger

    @type state :: %{count: integer()}
    @callback init(args :: term()) :: {:ok, state()}

    @spec start_link(keyword()) :: GenServer.on_start()
    def start_link(opts), do: GenServer.start_link(__MODULE__, opts)
  end
  ```

- [ ] **Step 2: Write failing test**

  Assert:
  - `use GenServer` → Import
  - `import Enum...` → Import
  - `alias MyApp.{User, Calculator}` → Import(s)
  - `require Logger` → Import
  - `@type state` → Type
  - `@callback init` → Function with metadata `"callback"`
  - `@spec` → attached to `start_link` function's metadata

- [ ] **Step 3: Run test — verify RED**

- [ ] **Step 4: Implement import calls** — `extract_import()`, `extract_use()`, `extract_alias()`, `extract_require()`

  All create `SymbolKind::Import`. Extract the module name from the first argument (an `alias` node).

- [ ] **Step 5: Implement `attributes.rs`** — `extract_module_attribute()`

  Match on `unary_operator` with `@` operator:
  - `@type` / `@typep` / `@opaque` → `SymbolKind::Type`
  - `@callback` → `SymbolKind::Function` with metadata
  - `@spec` → Store for later type inference attachment
  - `@behaviour` → Store for relationship extraction
  - `@moduledoc` / `@doc` → Attach as doc comment to next symbol

- [ ] **Step 6: Run test — verify GREEN**

- [ ] **Step 7: Commit**
  `feat(elixir): implement import/use/alias/require and module attribute extraction`

### Task 13: Elixir relationships + identifiers + type inference

**Files:**
- Modify: `crates/julie-extractors/src/elixir/relationships.rs`
- Modify: `crates/julie-extractors/src/elixir/identifiers.rs`
- Modify: `crates/julie-extractors/src/elixir/types_inference.rs`
- Modify: test file

- [ ] **Step 1: Write failing test for relationships**

  Assert:
  - `use GenServer` → Uses relationship
  - `@behaviour GenServer` → Implements relationship
  - `defimpl Printable, for: Integer` → Implements relationship (Printable)
  - `Calculator.add(1, 2)` → Calls relationship

- [ ] **Step 2: Run test — verify RED**

- [ ] **Step 3: Implement `relationships.rs`**

  - `extract_use_relationships()`: `use` calls → Uses relationship
  - `extract_behaviour_relationships()`: `@behaviour` attributes → Implements relationship
  - `extract_protocol_impl_relationships()`: `defimpl` → Implements relationship
  - `extract_call_relationships()`: Walk tree for `call` nodes that aren't definition macros → Calls relationships or PendingRelationship

- [ ] **Step 4: Implement `identifiers.rs`**

  Walk tree for: `call` nodes (function invocations), `dot` nodes (qualified calls like `Module.function`), `alias` nodes (module references → TypeUsage kind).

- [ ] **Step 5: Implement `types_inference.rs`**

  `infer_types()`: Look for `@spec` attributes collected during extraction, parse return type portion, associate with the corresponding function symbol.

- [ ] **Step 6: Run test — verify GREEN**

- [ ] **Step 7: Run `cargo xtask test dev` — verify no regressions**

- [ ] **Step 8: Commit**
  `feat(elixir): implement relationships, identifiers, and type inference`

---

## Final Verification

### Task 14: End-to-end verification

- [ ] **Step 1: Run full dev test tier**
  `cargo xtask test dev 2>&1 | tail -20`
  Expected: all previously-passing tests still pass, new Scala/Elixir tests pass

- [ ] **Step 2: Verify language counts are consistent**
  - `language.rs` test asserts 29 languages
  - `manager.rs` vec has 31 entries (29 langs + tsx + jsx)
  - `factory.rs` test asserts 31

- [ ] **Step 3: Build release for dogfooding**
  `cargo build --release`

- [ ] **Step 4: Dogfood — index a Scala project**
  Restart Claude Code, point at a Scala codebase:
  - `fast_search(query="SomeClass")` — should find Scala symbols
  - `get_symbols(file_path="src/main/scala/Example.scala")` — should show classes/traits/objects
  - `deep_dive(symbol="SomeClass")` — should show callers/callees

- [ ] **Step 5: Dogfood — index an Elixir project**
  Same tests against an Elixir codebase:
  - `fast_search(query="GenServer")` — should find modules
  - `get_symbols(file_path="lib/my_app/server.ex")` — should show modules/functions
  - `deep_dive(symbol="start_link")` — should show callers

- [ ] **Step 6: Update CLAUDE.md** — bump language count from 31 to 33, add Scala and Elixir to the language support table

- [ ] **Step 7: Final commit**
  `feat: add Scala and Elixir language support (33 languages)`

---

## Potential Challenges

| Risk | Mitigation |
|------|------------|
| `tree-sitter-elixir` ABI mismatch with tree-sitter 0.25 | Runtime dep is `tree-sitter-language = "0.1"`, not tree-sitter itself. Verify with `cargo build` immediately. |
| Elixir `calls.rs` exceeds 500 lines | Split into `calls_definitions.rs` (def/defp/defmacro) and `calls_modules.rs` (defmodule/defprotocol/defimpl) |
| Elixir multi-clause functions | Extract each clause as separate symbol (same name, different line). Consistent with overload handling elsewhere. |
| Scala 3 features not in grammar | `tree-sitter-scala` covers Scala 2 and 3. `given_definition` and `extension_definition` are named nodes. |
| Pipe operator `\|>` chains | Binary operators with function calls on right side. Handle in relationship extraction, not blocking for initial impl. |
