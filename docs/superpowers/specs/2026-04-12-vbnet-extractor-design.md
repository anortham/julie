# VB.NET Extractor Integration

**Date**: 2026-04-12
**Status**: Approved
**Scope**: Add VB.NET as the 34th language in Julie's tree-sitter extractors

---

## Overview

Integrate the `tree-sitter-vb-dotnet` grammar (located at `/mnt/work/projects/tree-sitter-vb-dotnet/`) into Julie as a full extractor, supporting VB.NET up to version 15 (.NET Framework era). XML literals are not supported by the grammar.

**Internal language identifier**: `"vbnet"`
**File extension**: `.vb`

---

## Grammar Node → SymbolKind Mapping

| Grammar node | SymbolKind | Notes |
|---|---|---|
| `namespace_block` | Namespace | |
| `imports_statement` | Import | |
| `class_block` | Class | |
| `module_block` | Class | VB modules are static classes; metadata `"vb_module": "true"` |
| `structure_block` | Struct | |
| `interface_block` | Interface | |
| `enum_block` | Enum | |
| `enum_member` | EnumMember | |
| `method_declaration` | Function | Covers both Sub and Function |
| `abstract_method_declaration` | Function | MustOverride / interface methods (no body) |
| `constructor_declaration` | Constructor | `Sub New(...)` |
| `property_declaration` | Property | Auto-properties and block properties with Get/Set |
| `field_declaration` | Field | Dim / modifier-based declarations |
| `event_declaration` | Event | Standard and Custom events |
| `delegate_declaration` | Delegate | |
| `operator_declaration` | Operator | Overloaded operators |
| `const_declaration` | Constant | |
| `declare_statement` | Function | P/Invoke declarations |

---

## Module Structure

```
crates/julie-extractors/src/vbnet/
├── mod.rs              # VbNetExtractor struct, walk_tree, extract_symbol dispatch
├── types.rs            # namespace, class, module, structure, interface, enum, delegate
├── members.rs          # method, constructor, property, field, event, operator, const, declare
├── relationships.rs    # inherits, implements, calls, imports
├── identifiers.rs      # All identifier usages in code
├── type_inference.rs   # Return types, parameter types, field types
└── helpers.rs          # VB-specific utilities (modifier parsing, visibility mapping)
```

### VbNetExtractor struct

```rust
pub struct VbNetExtractor {
    base: BaseExtractor,
    pending_relationships: Vec<PendingRelationship>,
}
```

Standard 5-method contract:
- `extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol>`
- `extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship>`
- `extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier>`
- `infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String>`
- `get_pending_relationships(&self) -> Vec<PendingRelationship>`

---

## VB-Specific Considerations

### Visibility Mapping

| VB Modifier | Julie Visibility |
|---|---|
| `Public` | Public |
| `Private` | Private |
| `Protected` | Protected |
| `Friend` | Internal (C#'s `internal`) |
| `Protected Friend` | Protected |
| (no modifier) | Private (default for class members) |

### Module Handling

VB `Module` blocks are semantically static classes (all members are implicitly `Shared`). Map to `SymbolKind::Class` with metadata `"vb_module": "true"`.

### Sub vs Function

Both map to `SymbolKind::Function`. The signature captures which keyword was used. `Sub` methods have no return type; `Function` methods include the `As <type>` return type in the signature.

### Handles Clause

Methods with `Handles Button.Click` create a pending relationship (event handler binding) targeting the event source.

### Case Insensitivity

VB.NET is case-insensitive. Store names as-written (preserving source casing). Julie's search tokenizer already handles case-insensitive matching.

### type_declaration Wrapper

The grammar wraps type blocks in a `type_declaration` node that holds attributes. The `walk_tree` traversal naturally descends through this — no special handling needed since we match on the inner `class_block`, `module_block`, etc.

---

## Language Registration (language.rs)

Seven functions need `"vbnet"` arms:

| Function | Value |
|---|---|
| `get_tree_sitter_language` | `tree_sitter_vb_dotnet::LANGUAGE.into()` |
| `detect_language_from_extension` | `"vb"` → `Some("vbnet")` |
| `supported_extensions` | Add `"vb"` |
| `supported_languages` | Add `"vbnet"` |
| `get_function_node_kinds` | `["method_declaration", "abstract_method_declaration"]` |
| `get_import_node_kinds` | `["imports_statement"]` |
| `get_symbol_node_kinds` | `["method_declaration", "class_block", "interface_block", "structure_block", "enum_block", "module_block"]` |
| `get_symbol_name_field` | `"name"` (existing default covers this) |

---

## Factory Registration (factory.rs)

Add `"vbnet"` arm in `extract_symbols_and_relationships` using the full extraction path:

```rust
"vbnet" => {
    let mut ext = crate::vbnet::VbNetExtractor::new(
        language.to_string(),
        file_path.to_string(),
        content.to_string(),
        workspace_root,
    );
    let symbols = ext.extract_symbols(tree);
    let relationships = ext.extract_relationships(tree, &symbols);
    let identifiers = ext.extract_identifiers(tree, &symbols);
    let types = ext.infer_types(&symbols);
    let pending = ext.get_pending_relationships();
    Ok(ExtractionResults {
        symbols,
        relationships,
        pending_relationships: pending,
        identifiers,
        types: convert_types_map(types, language),
    })
}
```

---

## Module Declaration (lib.rs)

Add `pub mod vbnet` in the language modules section.

---

## Dependency Setup (Cargo.toml)

Add to `crates/julie-extractors/Cargo.toml`:

```toml
tree-sitter-vb-dotnet = { path = "../../tree-sitter-vb-dotnet" }  # Local path during development
```

The path is relative, pointing to the grammar repo at `/mnt/work/projects/tree-sitter-vb-dotnet` (sibling of the julie repo at `/mnt/work/projects/julie`).

---

## Test Plan

Tests under `crates/julie-extractors/src/tests/vbnet/`:

| Test file | Coverage |
|---|---|
| `mod.rs` | `init_parser()` helper, module declarations |
| `core.rs` | Namespace, imports, class, module, structure, interface, enum, delegate extraction |
| `members.rs` | Method (Sub/Function), constructor, property, field, event, operator, const, declare |
| `relationships.rs` | Inherits, implements, calls, imports relationships |
| `identifiers.rs` | Identifier extraction across VB constructs |
| `types.rs` | Type inference for fields, parameters, return types |

Each test parses a VB.NET snippet, runs the extractor, and asserts on output symbols/relationships.

Register test module in `crates/julie-extractors/src/tests/mod.rs`.

---

## Documentation Updates

- Update language count from 33 → 34 in `CLAUDE.md` and `lib.rs` doc comments
- Add `"vbnet"` to the language filter list in tool descriptions where languages are enumerated
- Update `supported_languages()` doc comment

---

## Out of Scope

- DI relationship detection (not relevant for .NET Framework VB.NET)
- Member-type relationship tracking (C#-specific patterns)
- XML literal support (grammar does not support them)
- VB6/VBA/VBScript (different languages entirely)
