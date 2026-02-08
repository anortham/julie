# Round 5: Extractor Quality — B→B+ Promotions & A-Grade Push

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate the B tier entirely (promote C, C++, Ruby, R to B+), fix remaining cross-cutting quality issues, and push 4 key extractors from B+ to A.

**Architecture:** Each task targets 1-2 extractors with focused fixes. Tasks are independent — no cross-task dependencies. TDD: write failing test, implement fix, verify, commit.

**Tech Stack:** Rust, tree-sitter, regex, serde_json. Test harness: `cargo test -p julie-extractors`.

**Run all extractor tests:** `cargo test -p julie-extractors --lib 2>&1 | tail -5`

**Current state:** 1216 extractor tests pass. Ratings: 7 A, 1 A (TOML), 20 B+, 4 B.

---

### Task 1: C — Extract struct/union fields as Field children

**Problem:** The C extractor extracts structs and unions but doesn't extract their individual fields as `SymbolKind::Field` child symbols. The Rust extractor already does this (added in Round 1). Without fields, struct members are invisible to code intelligence.

**Files:**
- Modify: `crates/julie-extractors/src/c/structs.rs` (add field extraction)
- Modify: `crates/julie-extractors/src/c/mod.rs` (call field extraction from visit_node)
- Test: `crates/julie-extractors/src/tests/c/` (add field extraction tests)

**Approach:**

1. Write a test that parses a C struct with fields and asserts `SymbolKind::Field` children exist with correct parent_id, types in signature, and names.

```rust
#[test]
fn test_struct_field_extraction() {
    let code = r#"
struct Point {
    double x;
    double y;
    const char *label;
};
"#;
    let symbols = extract_symbols(code);
    let struct_sym = symbols.iter().find(|s| s.name == "Point").unwrap();
    let fields: Vec<_> = symbols.iter()
        .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&struct_sym.id))
        .collect();
    assert_eq!(fields.len(), 3);
    assert!(fields.iter().any(|f| f.name == "x"));
    assert!(fields.iter().any(|f| f.name == "y"));
    assert!(fields.iter().any(|f| f.name == "label"));
}
```

2. In `structs.rs`, add a new function `extract_struct_fields`:
   - After extracting the struct symbol, walk `field_declaration` children inside the struct body (`field_declaration_list`)
   - For each field: extract name from `field_declarator` or `identifier`, extract type from the type specifier
   - Create `SymbolKind::Field` with parent_id set to the struct's ID
   - Include type in signature (e.g., `"double x"`)
   - Handle pointer declarators, array declarators, and multi-field declarations (`int x, y;`)

3. Apply the same pattern for union fields.

4. Run: `cargo test -p julie-extractors --lib -- c 2>&1 | tail -20`

5. Commit: `fix(c): extract struct/union fields as SymbolKind::Field children`

**Also fix:** Remove the hardcoded `AtomicCounter` hack in declarations.rs (the audit noted this at line ~731-735 as a test-specific hack in production code). Search for "AtomicCounter" and remove/generalize.

**Target rating:** B → B+

---

### Task 2: C++ — Template variable extraction + typedef handler

**Problem:** `extract_template` in `declarations.rs:117-125` is a stub returning `None`. Template variable declarations (`template<class T> constexpr T pi = T(3.14)`) are missed entirely. Also, `typedef` declarations have no handler — `type_definition` nodes are not processed in visit_node.

**Files:**
- Modify: `crates/julie-extractors/src/cpp/declarations.rs` (implement `extract_template`)
- Modify: `crates/julie-extractors/src/cpp/mod.rs` (add `type_definition` to visit_node match)
- Test: `crates/julie-extractors/src/tests/cpp/` (add template variable + typedef tests)

**Approach:**

1. Write tests:

```rust
#[test]
fn test_template_variable() {
    let code = r#"template<class T> constexpr T pi = T(3.14159);"#;
    let symbols = extract_symbols(code);
    let pi = symbols.iter().find(|s| s.name == "pi");
    assert!(pi.is_some(), "Template variable 'pi' should be extracted");
    let pi = pi.unwrap();
    assert_eq!(pi.kind, SymbolKind::Constant);
}

#[test]
fn test_typedef() {
    let code = r#"typedef unsigned long size_t;"#;
    let symbols = extract_symbols(code);
    let typedef = symbols.iter().find(|s| s.name == "size_t");
    assert!(typedef.is_some(), "Typedef 'size_t' should be extracted");
    assert_eq!(typedef.unwrap().kind, SymbolKind::Type);
}
```

2. Implement `extract_template`: When the child of a `template_declaration` is a `declaration` node (not a function/class), extract it as a template variable with `SymbolKind::Constant` (for constexpr) or `SymbolKind::Variable`. Include template parameters in signature.

3. Add a `extract_typedef` function: Handle `type_definition` nodes. Extract the alias name from the `type_declarator`, use `SymbolKind::Type`. Include the aliased type in signature.

4. Wire `"type_definition" => extract_typedef(...)` in mod.rs visit_node.

5. Run tests and commit.

**Target rating:** B → B+

---

### Task 3: Ruby — Struct.new detection + module_function + dead code

**Problem:** Ruby `Struct.new` class definitions (`Person = Struct.new(:name, :age)`) are a very common Ruby pattern but not extracted. `module_function` declarations that change method visibility are not handled. Dead code exists in `helpers.rs`.

**Files:**
- Modify: `crates/julie-extractors/src/ruby/calls.rs` (add Struct.new detection)
- Modify: `crates/julie-extractors/src/ruby/helpers.rs` (remove dead code)
- Test: `crates/julie-extractors/src/tests/ruby/` (add tests)

**Approach:**

1. Write tests:

```rust
#[test]
fn test_struct_new_class() {
    let code = r#"Person = Struct.new(:name, :age, :email)"#;
    let symbols = extract_symbols(code);
    let person = symbols.iter().find(|s| s.name == "Person");
    assert!(person.is_some(), "Struct.new should create a Class symbol");
    assert_eq!(person.unwrap().kind, SymbolKind::Class);
    // Fields should be Property children
    let fields: Vec<_> = symbols.iter()
        .filter(|s| s.kind == SymbolKind::Property && s.parent_id == person.map(|p| p.id.clone()))
        .collect();
    assert!(fields.len() >= 3, "Struct.new fields should be extracted as Property children");
}

#[test]
fn test_module_function() {
    let code = r#"
module MyModule
  def helper
    "help"
  end
  module_function :helper
end
"#;
    let symbols = extract_symbols(code);
    let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
    // module_function makes it public at module level
    assert_eq!(helper.visibility, Some(Visibility::Public));
}
```

2. In `calls.rs`, add handling for `Struct.new` calls in `extract_call`:
   - Detect when the method name is "new" and the receiver is "Struct"
   - Walk backward to find the assignment target (the class name)
   - Extract as `SymbolKind::Class` with signature `"Struct.new(:field1, :field2)"`
   - Extract each symbol argument (`:name`, `:age`) as `SymbolKind::Property` children

3. Add handling for `module_function` calls:
   - When method name is "module_function", extract the symbol argument
   - Find the matching method in already-extracted symbols and set visibility to Public
   - Or track it as a post-processing step

4. Remove dead `extract_assignment_symbols` and `extract_parallel_assignment_fallback` from `helpers.rs` (marked `#[allow(dead_code)]`).

5. Run tests and commit.

**Target rating:** B → B+

---

### Task 4: R — Fix identifier matching + remove synthetic IDs

**Problem:** The R extractor's `find_containing_symbol_id` uses line-number matching instead of the standard `base.find_containing_symbol()` pattern. The relationship extractor creates synthetic IDs like `builtin_print`, `piped_filter`, `member_data` that don't match any real symbol and create dead-end references.

**Files:**
- Modify: `crates/julie-extractors/src/r/identifiers.rs` (fix `find_containing_symbol_id`)
- Modify: `crates/julie-extractors/src/r/relationships.rs` (remove synthetic IDs)
- Test: `crates/julie-extractors/src/tests/r/` (add/update tests)

**Approach:**

1. Write a test that verifies identifiers inside functions have correct `scope_id`:

```rust
#[test]
fn test_identifier_scope_resolution() {
    let code = r#"
my_function <- function(x) {
    result <- process(x)
    print(result)
}
"#;
    let (symbols, _, identifiers) = extract_all(code);
    let func = symbols.iter().find(|s| s.name == "my_function").unwrap();
    let print_id = identifiers.iter().find(|i| i.name == "print").unwrap();
    // print() call should be scoped to my_function
    assert_eq!(print_id.scope_id.as_deref(), Some(func.id.as_str()));
}
```

2. Fix `find_containing_symbol_id` (lines 179-200):
   - Replace the manual line-number scanning with `extractor.base.find_containing_symbol(symbols, node)` or equivalent byte-range containment check
   - This is the pattern used by all other extractors

3. In `relationships.rs`, find and remove synthetic ID creation:
   - Lines ~95, ~164, ~236 create IDs like `builtin_print`, `piped_filter`, `member_data`
   - Replace with proper `PendingRelationship` for cross-file resolution, or skip when target can't be resolved
   - For builtins (print, cat, paste), either skip relationship creation or use `PendingRelationship`

4. Run tests and commit.

**Target rating:** B → B+

---

### Task 5: Cross-cutting — Final LazyLock regex sweep

**Problem:** Several extractors still have inline `Regex::new()` calls that compile on every invocation. Previous rounds fixed most sites but some remain.

**Confirmed remaining sites (verified via `fast_search`):**
- `crates/julie-extractors/src/zig/error_handling.rs:16` — `let partial_match = Regex::new(...)`
- `crates/julie-extractors/src/vue/component.rs:13` — `regex::Regex::new(...)`
- `crates/julie-extractors/src/sql/helpers.rs:11` — inline `Regex::new(` (not in LazyLock)
- `crates/julie-extractors/src/sql/helpers.rs:23` — inline `Regex::new(` (not in LazyLock)
- `crates/julie-extractors/src/sql/helpers.rs:36` — inline `Regex::new(` (not in LazyLock)
- `crates/julie-extractors/src/java/types.rs:12` — inline `Regex::new(` (not in LazyLock)
- `crates/julie-extractors/src/java/types.rs:18` — inline `Regex::new(` (not in LazyLock)

**Already fixed (verified — no action needed):**
- Bash variables.rs: already uses `LazyLock ENV_VAR_RE`
- PowerShell helpers.rs: already has LazyLock statics
- SQL constraints.rs: no `Regex::new` found
- Python mod.rs: no `Regex::new` found
- Rust mod.rs: no `Regex::new` found

**Files:**
- Modify: `crates/julie-extractors/src/zig/error_handling.rs`
- Modify: `crates/julie-extractors/src/vue/component.rs`
- Modify: `crates/julie-extractors/src/sql/helpers.rs`
- Modify: `crates/julie-extractors/src/java/types.rs`

**Approach:**

For each site:
1. Move the `Regex::new(...)` call to a `static LazyLock<Regex>` at module level
2. Replace the inline call with a reference to the static
3. Add `use std::sync::LazyLock;` if not already imported
4. Verify with `cargo test -p julie-extractors --lib` — no behavior change expected

No new tests needed — this is a pure performance refactor. Existing tests verify correctness.

**Commit:** `perf(extractors): convert remaining inline Regex::new to LazyLock statics`

---

### Task 6: Cross-cutting — Code quality fixes (SymbolKind, dead code, safety)

**Problem:** Several extractors have minor code quality issues that prevent A rating.

**Issues to fix:**

**6a. CSS SymbolKind corrections:**
- `crates/julie-extractors/src/css/rules.rs:40` — class selectors (`.button`) use `SymbolKind::Class`. CSS classes aren't OOP classes. Change to `SymbolKind::Property` (consistent with how CSS custom properties are already handled, and matches the "selector as property" semantic).
- `crates/julie-extractors/src/css/rules.rs:42-47` — ID selectors use `SymbolKind::Variable` as catch-all. No change needed for IDs but verify consistency.

**6b. Regex dead code cleanup:**
- `crates/julie-extractors/src/regex/` — narrow broad `#[allow(dead_code)]` on helpers, patterns, and signatures modules. Remove the attribute and delete any actually-dead functions, or narrow to specific items.
- Remove duplicate `extract_group_name` that exists in both `groups.rs` and `identifiers.rs`. Keep one, import from the other.

**6c. SQL dead code:**
- `crates/julie-extractors/src/sql/relationships.rs:155-175` — `extract_table_references` assigns to `_table_symbol` and does nothing. Either implement or remove.

**6d. Razor UTF-8 safety:**
- `crates/julie-extractors/src/razor/directives.rs:292` — `content[..content.len().min(200)]` is NOT UTF-8 boundary safe. Replace with `BaseExtractor::truncate_string(&content, 200)` or equivalent char-boundary-safe truncation.

**6e. HTML SymbolKind:**
- `crates/julie-extractors/src/html/elements.rs:164` — DOCTYPE uses `SymbolKind::Variable`. Change to `SymbolKind::Namespace` (DOCTYPE declares the document type/namespace).

**Approach:**

For each sub-task:
1. Write a test if the change affects behavior (CSS SymbolKind, HTML SymbolKind)
2. Make the change
3. Update/fix any broken tests
4. Verify with `cargo test -p julie-extractors --lib`
5. Commit each logical group separately

---

### Task 7: Rust → A — Grouped/glob imports + macro fix + static→Constant

**Problem:** The Rust extractor has three remaining quality issues preventing A rating:
1. Use declaration extraction (`signatures.rs:167-219`) uses regex instead of tree-sitter. Grouped imports (`use foo::{bar, baz}`) and glob imports (`use foo::*`) are not handled.
2. `extract_macro_invocation` uses `unwrap_or_default()` on the macro name (`signatures.rs:131`), producing empty string instead of returning None.
3. Static items use `SymbolKind::Variable` (`types.rs:327`). Statics are semantically constants.

**Files:**
- Modify: `crates/julie-extractors/src/rust/signatures.rs` (imports + macro fix)
- Modify: `crates/julie-extractors/src/rust/types.rs` (static → Constant)
- Test: `crates/julie-extractors/src/tests/rust/`

**Approach:**

1. Write tests:

```rust
#[test]
fn test_grouped_use_declaration() {
    let code = r#"use std::collections::{HashMap, BTreeMap, HashSet};"#;
    let symbols = extract_symbols(code);
    let imports: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Import).collect();
    // Should extract all three imports, or at least the grouped import with all names
    assert!(imports.len() >= 1);
    // Verify all names appear somewhere (either as separate symbols or in signature)
    let all_text: String = imports.iter().map(|i| format!("{} {}", i.name, i.signature.as_deref().unwrap_or(""))).collect();
    assert!(all_text.contains("HashMap"), "Should include HashMap");
    assert!(all_text.contains("BTreeMap"), "Should include BTreeMap");
}

#[test]
fn test_glob_import() {
    let code = r#"use std::collections::*;"#;
    let symbols = extract_symbols(code);
    let imports: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Import).collect();
    assert!(!imports.is_empty(), "Glob import should be extracted");
}

#[test]
fn test_static_is_constant() {
    let code = r#"static MAX_SIZE: usize = 1024;"#;
    let symbols = extract_symbols(code);
    let max = symbols.iter().find(|s| s.name == "MAX_SIZE").unwrap();
    assert_eq!(max.kind, SymbolKind::Constant);
}
```

2. Refactor use declaration extraction in `signatures.rs`:
   - Replace regex-based parsing with tree-sitter node traversal
   - Handle `use_declaration` → `use_list` (grouped) with individual `use_as_clause` or `identifier`/`scoped_identifier` children
   - Handle `use_wildcard` for glob imports
   - Each item in a grouped import becomes a separate `SymbolKind::Import` symbol

3. Fix macro invocation: replace `unwrap_or_default()` with `?` to return `None` early when no name found.

4. Change `SymbolKind::Variable` to `SymbolKind::Constant` for static items in `types.rs:327`.

5. Run tests and commit.

**Target rating:** B+ → A

---

### Task 8: Python → A — Wildcard imports + code quality fixes

**Problem:** The Python extractor has three remaining issues:
1. Wildcard imports (`from module import *`) not handled
2. `extract_class` finds class name by `node.children().nth(1)` instead of `child_by_field_name("name")` (fragile)
3. Lambda naming uses `<lambda:row>` format with angle brackets that may break search indexing

**Files:**
- Modify: `crates/julie-extractors/src/python/imports.rs` (wildcard imports)
- Modify: `crates/julie-extractors/src/python/types.rs` (class name extraction)
- Modify: `crates/julie-extractors/src/python/functions.rs` (lambda naming)
- Test: `crates/julie-extractors/src/tests/python/`

**Approach:**

1. Write tests:

```rust
#[test]
fn test_wildcard_import() {
    let code = r#"from os.path import *"#;
    let symbols = extract_symbols(code);
    let imports: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Import).collect();
    assert!(!imports.is_empty(), "Wildcard import should be extracted");
    let import = imports.first().unwrap();
    assert!(import.name.contains("*") || import.signature.as_deref().unwrap_or("").contains("*"),
        "Wildcard import should indicate * in name or signature");
}

#[test]
fn test_lambda_naming_no_angle_brackets() {
    let code = r#"transform = lambda x: x * 2"#;
    let symbols = extract_symbols(code);
    let lambda = symbols.iter().find(|s| s.name.contains("lambda"));
    if let Some(l) = lambda {
        assert!(!l.name.contains('<'), "Lambda name should not contain angle brackets");
        assert!(!l.name.contains('>'), "Lambda name should not contain angle brackets");
    }
}
```

2. In `imports.rs`, handle `wildcard_import` node type:
   - When the import statement contains `*`, extract as `SymbolKind::Import` with name `"*"` and the module path in the signature (e.g., `"from os.path import *"`)

3. In `types.rs`, replace `node.children().nth(1)` with `node.child_by_field_name("name")` for class name extraction. This is more robust against parser changes.

4. In `functions.rs`, change lambda naming from `<lambda:N>` to `lambda_N` (or just `lambda` with line number in metadata). Removes angle brackets that could interfere with search tokenization.

5. Run tests and commit.

**Target rating:** B+ → A

---

### Task 9: TypeScript → A — Decorators + access modifiers

**Problem:** TypeScript decorators (`@Component`, `@Injectable()`) and access modifiers (`private`, `protected`, `public`, `readonly`) on class members are not extracted. These are core TypeScript features.

**Files:**
- Modify: `crates/julie-extractors/src/typescript/` (decorators in classes.rs or new decorators.rs, access modifiers in interfaces.rs or members)
- Test: `crates/julie-extractors/src/tests/typescript/`

**Approach:**

1. Write tests:

```rust
#[test]
fn test_decorator_extraction() {
    // Must use tree_sitter_typescript parser, not JS
    let code = r#"
@Component({
    selector: 'app-root'
})
class AppComponent {
    @Input() title: string;
}
"#;
    let symbols = extract_symbols_ts(code); // use TS parser
    let class_sym = symbols.iter().find(|s| s.name == "AppComponent").unwrap();
    // Decorator should appear in signature or as metadata
    assert!(class_sym.signature.as_deref().unwrap_or("").contains("@Component")
        || class_sym.doc_comment.as_deref().unwrap_or("").contains("@Component"),
        "Class decorator should be captured");
}

#[test]
fn test_access_modifiers() {
    let code = r#"
class User {
    private name: string;
    protected age: number;
    public readonly email: string;

    private getName(): string { return this.name; }
}
"#;
    let symbols = extract_symbols_ts(code);
    let name_prop = symbols.iter().find(|s| s.name == "name").unwrap();
    assert_eq!(name_prop.visibility, Some(Visibility::Private));

    let age_prop = symbols.iter().find(|s| s.name == "age").unwrap();
    assert_eq!(age_prop.visibility, Some(Visibility::Protected));

    let email_prop = symbols.iter().find(|s| s.name == "email").unwrap();
    assert_eq!(email_prop.visibility, Some(Visibility::Public));
}
```

2. **Decorators:** In the class/method/property extraction functions:
   - Check if the node has a `decorator` parent/sibling (tree-sitter-typescript produces `decorator` nodes as children of `class_declaration`, `method_definition`, etc.)
   - Extract decorator names and include them in the symbol's signature or doc_comment
   - For class-level decorators: prepend to class signature (e.g., `"@Component class AppComponent"`)
   - For member-level decorators: include in member signature

3. **Access modifiers:** In property/method extraction:
   - Look for `accessibility_modifier` child nodes (`public`, `private`, `protected`)
   - Set `Visibility` accordingly (currently defaults to Public for everything)
   - Look for `readonly` modifier and include in signature
   - Include modifiers in property/method signatures

4. Run tests with the TypeScript parser (NOT JavaScript). Commit.

**Target rating:** B+ → A

---

### Task 10: Kotlin → A — Secondary constructors + code quality

**Problem:** Only primary constructor parameters are extracted. Secondary constructors (`constructor(...)`) are a common Kotlin pattern. Also, `extract_function` calls `extract_return_type` twice — once for signature, once for metadata.

**Files:**
- Modify: `crates/julie-extractors/src/kotlin/types.rs` (secondary constructors, double return type)
- Test: `crates/julie-extractors/src/tests/kotlin/`

**Approach:**

1. Write tests:

```rust
#[test]
fn test_secondary_constructor() {
    let code = r#"
class Person(val name: String) {
    var age: Int = 0

    constructor(name: String, age: Int) : this(name) {
        this.age = age
    }
}
"#;
    let symbols = extract_symbols(code);
    let constructors: Vec<_> = symbols.iter()
        .filter(|s| s.kind == SymbolKind::Constructor)
        .collect();
    assert!(constructors.len() >= 1, "Secondary constructor should be extracted");
    let secondary = constructors.iter().find(|c| {
        c.signature.as_deref().unwrap_or("").contains("age")
    });
    assert!(secondary.is_some(), "Secondary constructor should have parameters in signature");
}
```

2. In `types.rs`, add handling for `secondary_constructor` node type:
   - Extract as `SymbolKind::Constructor` with the class as parent
   - Build signature from parameters including types
   - Include delegation target in signature (`": this(name)"`)

3. Fix the double `extract_return_type` call:
   - In `extract_function` (types.rs around line 264 and 316), extract return type once, store in a variable, use for both signature building and metadata

4. Run tests and commit.

**Target rating:** B+ → A

---

### Task 11: Update audit doc — Re-rate all extractors

**Problem:** After Tasks 1-10, many extractors will have new ratings. Additionally, several B+ extractors with "None significant" remaining issues should be evaluated for A rating.

**File:** `docs/EXTRACTOR_AUDIT.md`

**Approach:**

1. Run the full test suite and record the new count: `cargo test -p julie-extractors --lib 2>&1 | tail -5`

2. Add a "Round 5" section at the top of the audit doc (below Round 4) documenting all changes made.

3. Update the status tables:
   - C: B → B+ (struct/union fields)
   - C++: B → B+ (template variables, typedef)
   - Ruby: B → B+ (Struct.new, module_function, dead code)
   - R: B → B+ (identifier fix, synthetic ID removal)
   - Rust: B+ → A (grouped imports, macro fix, static→Constant)
   - Python: B+ → A (wildcard imports, code quality)
   - TypeScript: B+ → A (decorators, access modifiers)
   - Kotlin: B+ → A (secondary constructors, code quality)
   - Plus cross-cutting: LazyLock, SymbolKind corrections, dead code removal

4. Evaluate these B+ extractors for A promotion (they have "None significant" remaining):
   - Zig, Lua, Bash, PowerShell, HTML, CSS, Razor, QML, SQL, Dart, Regex
   - For each: verify remaining P2 issues are acceptable (consistent with P2s in existing A-rated extractors like Java, C#, Go which also have P2s)
   - If the extractor's remaining issues are comparable to existing A-rated extractors, promote to A

5. Update the header with Round 5 date and new test count.

6. Commit: `docs: update extractor audit with Round 5 quality improvements`

---

## Execution Notes

**Task dependencies:** All tasks 1-10 are independent and can run in parallel.

**Task 11** depends on all other tasks completing first.

**Test command per extractor:**
- C: `cargo test -p julie-extractors --lib -- c:: 2>&1 | tail -20`
- C++: `cargo test -p julie-extractors --lib -- cpp:: 2>&1 | tail -20`
- Ruby: `cargo test -p julie-extractors --lib -- ruby:: 2>&1 | tail -20`
- R: `cargo test -p julie-extractors --lib -- r:: 2>&1 | tail -20`
- Rust: `cargo test -p julie-extractors --lib -- rust:: 2>&1 | tail -20`
- Python: `cargo test -p julie-extractors --lib -- python:: 2>&1 | tail -20`
- TypeScript: `cargo test -p julie-extractors --lib -- typescript:: 2>&1 | tail -20`
- Kotlin: `cargo test -p julie-extractors --lib -- kotlin:: 2>&1 | tail -20`
- Full suite: `cargo test -p julie-extractors --lib 2>&1 | tail -5`

**Expected outcome:** All 31 extractors at B+ or above. Target: 7+ at A, 0 at B.
