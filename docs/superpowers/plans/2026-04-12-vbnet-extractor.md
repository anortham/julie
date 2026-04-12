# VB.NET Extractor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add VB.NET as the 34th language in Julie's tree-sitter extractors with full extraction (symbols, relationships, identifiers, type inference).

**Architecture:** Standalone `vbnet` extractor module in `crates/julie-extractors/src/vbnet/` following the same pattern as the C# extractor. Uses `BaseExtractor` for shared machinery. Grammar dependency via local path to `tree-sitter-vb-dotnet`. Language identifier: `"vbnet"`, file extension: `.vb`.

**Tech Stack:** Rust, tree-sitter, tree-sitter-vb-dotnet grammar (local path dependency)

**Spec:** `docs/superpowers/specs/2026-04-12-vbnet-extractor-design.md`

---

### Task 1: Add tree-sitter-vb-dotnet dependency and scaffold module

**Files:**
- Modify: `crates/julie-extractors/Cargo.toml`
- Create: `crates/julie-extractors/src/vbnet/mod.rs`
- Create: `crates/julie-extractors/src/vbnet/types.rs`
- Create: `crates/julie-extractors/src/vbnet/members.rs`
- Create: `crates/julie-extractors/src/vbnet/relationships.rs`
- Create: `crates/julie-extractors/src/vbnet/identifiers.rs`
- Create: `crates/julie-extractors/src/vbnet/type_inference.rs`
- Create: `crates/julie-extractors/src/vbnet/helpers.rs`
- Modify: `crates/julie-extractors/src/lib.rs`

- [ ] **Step 1: Add the grammar dependency to Cargo.toml**

Add to `crates/julie-extractors/Cargo.toml` in the `[dependencies]` section, after the existing `tree-sitter-yaml` entry:

```toml
tree-sitter-vb-dotnet = { path = "../../../tree-sitter-vb-dotnet" }
```

The path is relative from `crates/julie-extractors/` to `/mnt/work/projects/tree-sitter-vb-dotnet`.

- [ ] **Step 2: Create the helpers module**

Create `crates/julie-extractors/src/vbnet/helpers.rs`:

```rust
use crate::base::{BaseExtractor, Visibility};
use tree_sitter::Node;

/// Extract modifier keywords from a VB.NET node's `modifiers` field.
/// VB modifiers node contains individual `modifier` tokens like `Public`, `Private`, etc.
/// The grammar lowercases them via case-insensitive matching, but the source text
/// preserves original casing. We lowercase for consistent comparison.
pub fn extract_modifiers(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut modifiers = Vec::new();
    if let Some(mod_node) = node.child_by_field_name("modifiers") {
        let mut cursor = mod_node.walk();
        for child in mod_node.children(&mut cursor) {
            if child.kind() == "modifier" {
                modifiers.push(base.get_node_text(&child).to_lowercase());
            }
        }
    }
    modifiers
}

/// Determine Julie Visibility from VB modifier keywords.
/// VB defaults: class members default to Private.
/// `Friend` maps to Internal (stored as Private, with metadata for actual value).
pub fn determine_visibility(modifiers: &[String]) -> Visibility {
    for m in modifiers {
        match m.as_str() {
            "public" => return Visibility::Public,
            "private" => return Visibility::Private,
            "protected" => return Visibility::Protected,
            "friend" => return Visibility::Private,
            _ => {}
        }
    }
    // VB default visibility for class members is Private
    Visibility::Private
}

/// Get the raw VB visibility string for metadata storage.
pub fn get_vb_visibility_string(modifiers: &[String]) -> String {
    for m in modifiers {
        match m.as_str() {
            "public" | "private" | "protected" | "friend" => return m.clone(),
            _ => {}
        }
    }
    // Check for compound: "protected friend"
    if modifiers.contains(&"protected".to_string()) && modifiers.contains(&"friend".to_string()) {
        return "protected friend".to_string();
    }
    "private".to_string()
}

/// Extract the return type from a VB method/function/property/delegate node.
/// Looks for `As <type>` clause via the `return_type` field.
pub fn extract_return_type(base: &BaseExtractor, node: &Node) -> Option<String> {
    node.child_by_field_name("return_type")
        .map(|n| base.get_node_text(&n))
}

/// Extract parameter list text from a node with a `parameters` field.
pub fn extract_parameters(base: &BaseExtractor, node: &Node) -> String {
    node.child_by_field_name("parameters")
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "()".to_string())
}

/// Extract type parameters (Of T) from a node.
pub fn extract_type_parameters(base: &BaseExtractor, node: &Node) -> Option<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == "type_parameters")
        .map(|n| base.get_node_text(&n))
}

/// Extract the type from an `as_clause` child node.
/// VB `as_clause` has a `type` field.
pub fn extract_as_clause_type(base: &BaseExtractor, node: &Node) -> Option<String> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| c.kind() == "as_clause")
        .and_then(|ac| ac.child_by_field_name("type"))
        .map(|t| base.get_node_text(&t))
}

/// Extract inherits clause types from a class/structure/interface node.
/// Returns the list of type names from the `inherits` field.
pub fn extract_inherits(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let Some(inherits_node) = node.child_by_field_name("inherits") else {
        return Vec::new();
    };
    // inherits_clause: `Inherits Type1, Type2`
    // Children are: keyword "Inherits", then comma-separated types
    let mut types = Vec::new();
    let mut cursor = inherits_node.walk();
    for child in inherits_node.children(&mut cursor) {
        let kind = child.kind();
        if kind != "," && !kind.eq_ignore_ascii_case("inherits") {
            types.push(base.get_node_text(&child));
        }
    }
    types
}

/// Extract implements clause types from a class/structure node.
/// A class can have multiple `implements` fields (one per Implements line).
pub fn extract_implements(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut types = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "implements_clause" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                let kind = inner.kind();
                if kind != "," && !kind.eq_ignore_ascii_case("implements") {
                    types.push(base.get_node_text(&inner));
                }
            }
        }
    }
    types
}

/// Extract attribute names from attribute_block children.
/// VB attributes use angle brackets: `<Test>`, `<TestMethod()>`.
pub fn extract_attributes(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_block" {
            let mut block_cursor = child.walk();
            for attr in child.children(&mut block_cursor) {
                if attr.kind() == "attribute" {
                    if let Some(name_node) = attr.child_by_field_name("name") {
                        attrs.push(base.get_node_text(&name_node));
                    }
                }
            }
        }
    }
    attrs
}

/// Build a modifier prefix string for signatures.
/// Returns "public shared " or "" etc.
pub fn modifier_prefix(modifiers: &[String]) -> String {
    if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    }
}
```

- [ ] **Step 3: Create the types module**

Create `crates/julie-extractors/src/vbnet/types.rs`:

```rust
use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

pub fn extract_namespace(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let signature = format!("Namespace {}", name);
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Namespace, options))
}

pub fn extract_imports(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    // imports_statement children: "Imports" keyword, then namespace_name or alias=namespace
    // Try alias first
    let alias_node = node.child_by_field_name("alias");
    let namespace_node = node.child_by_field_name("namespace")?;
    let namespace = base.get_node_text(&namespace_node);

    let (name, signature) = if let Some(alias) = alias_node {
        let alias_name = base.get_node_text(&alias);
        (alias_name.clone(), format!("Imports {} = {}", alias_name, namespace))
    } else {
        // Use the last segment of the namespace as the symbol name
        let name = namespace.rsplit('.').next().unwrap_or(&namespace).to_string();
        (name, format!("Imports {}", namespace))
    };

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Import, options))
}

pub fn extract_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!("{}Class {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature.push_str(&type_params);
    }

    let inherits = helpers::extract_inherits(base, &node);
    if !inherits.is_empty() {
        signature.push_str(&format!(" Inherits {}", inherits.join(", ")));
    }

    let implements = helpers::extract_implements(base, &node);
    if !implements.is_empty() {
        signature.push_str(&format!(" Implements {}", implements.join(", ")));
    }

    let doc_comment = base.find_doc_comment(&node);

    let mut metadata = HashMap::new();
    let vb_visibility = helpers::get_vb_visibility_string(&modifiers);
    metadata.insert(
        "vb_visibility".to_string(),
        serde_json::Value::String(vb_visibility),
    );

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
    };

    Some(base.create_symbol(&node, name, SymbolKind::Class, options))
}

pub fn extract_module(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let signature = format!("{}Module {}", helpers::modifier_prefix(&modifiers), name);
    let doc_comment = base.find_doc_comment(&node);

    let mut metadata = HashMap::new();
    metadata.insert(
        "vb_module".to_string(),
        serde_json::Value::Bool(true),
    );
    let vb_visibility = helpers::get_vb_visibility_string(&modifiers);
    metadata.insert(
        "vb_visibility".to_string(),
        serde_json::Value::String(vb_visibility),
    );

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
    };

    Some(base.create_symbol(&node, name, SymbolKind::Class, options))
}

pub fn extract_structure(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!("{}Structure {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature.push_str(&type_params);
    }

    let implements = helpers::extract_implements(base, &node);
    if !implements.is_empty() {
        signature.push_str(&format!(" Implements {}", implements.join(", ")));
    }

    let doc_comment = base.find_doc_comment(&node);

    let mut metadata = HashMap::new();
    let vb_visibility = helpers::get_vb_visibility_string(&modifiers);
    metadata.insert(
        "vb_visibility".to_string(),
        serde_json::Value::String(vb_visibility),
    );

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
    };

    Some(base.create_symbol(&node, name, SymbolKind::Struct, options))
}

pub fn extract_interface(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!("{}Interface {}", helpers::modifier_prefix(&modifiers), name);

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        signature.push_str(&type_params);
    }

    let inherits = helpers::extract_inherits(base, &node);
    if !inherits.is_empty() {
        signature.push_str(&format!(" Inherits {}", inherits.join(", ")));
    }

    let doc_comment = base.find_doc_comment(&node);

    let mut metadata = HashMap::new();
    let vb_visibility = helpers::get_vb_visibility_string(&modifiers);
    metadata.insert(
        "vb_visibility".to_string(),
        serde_json::Value::String(vb_visibility),
    );

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        metadata: Some(metadata),
        doc_comment,
    };

    Some(base.create_symbol(&node, name, SymbolKind::Interface, options))
}

pub fn extract_enum(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!("{}Enum {}", helpers::modifier_prefix(&modifiers), name);

    // Check for underlying type: `Enum Foo As Integer`
    if let Some(underlying) = node.child_by_field_name("underlying_type") {
        signature.push_str(&format!(" As {}", base.get_node_text(&underlying)));
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Enum, options))
}

pub fn extract_enum_member(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);

    let mut signature = name.clone();
    if let Some(value_node) = node.child_by_field_name("value") {
        signature = format!("{} = {}", name, base.get_node_text(&value_node));
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::EnumMember, options))
}

pub fn extract_delegate(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let params = helpers::extract_parameters(base, &node);
    let return_type = helpers::extract_return_type(base, &node);

    // Determine Sub vs Function from the keyword child
    let mut cursor = node.walk();
    let is_function = node.children(&mut cursor).any(|c| {
        let text = base.get_node_text(&c);
        text.eq_ignore_ascii_case("function")
    });

    let keyword = if is_function { "Function" } else { "Sub" };
    let mut signature = format!(
        "{}Delegate {} {}{}",
        helpers::modifier_prefix(&modifiers),
        keyword,
        name,
        params,
    );

    if let Some(ret) = &return_type {
        signature.push_str(&format!(" As {}", ret));
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Delegate, options))
}
```

- [ ] **Step 4: Create the members module**

Create `crates/julie-extractors/src/vbnet/members.rs`:

```rust
use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use crate::test_detection::is_test_symbol;
use std::collections::HashMap;
use tree_sitter::Node;

pub fn extract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let params = helpers::extract_parameters(base, &node);
    let return_type = helpers::extract_return_type(base, &node);

    // Determine Sub vs Function
    let mut cursor = node.walk();
    let is_function = node.children(&mut cursor).any(|c| {
        let text = base.get_node_text(&c);
        text.eq_ignore_ascii_case("function")
    });

    let keyword = if is_function { "Function" } else { "Sub" };
    let mut signature = format!(
        "{}{} {}{}",
        helpers::modifier_prefix(&modifiers),
        keyword,
        name,
        params,
    );

    if let Some(type_params) = helpers::extract_type_parameters(base, &node) {
        // Insert type params before the parameter list
        let insert_pos = signature.find(&name).unwrap_or(0) + name.len();
        signature.insert_str(insert_pos, &type_params);
    }

    if let Some(ret) = &return_type {
        signature.push_str(&format!(" As {}", ret));
    }

    let doc_comment = base.find_doc_comment(&node);
    let attributes = helpers::extract_attributes(base, &node);

    let mut metadata = HashMap::new();
    if is_test_symbol(
        "vbnet",
        &name,
        &base.file_path,
        &SymbolKind::Method,
        &[],
        &attributes,
        doc_comment.as_deref(),
    ) {
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
    }

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        metadata: if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        },
    };

    Some(base.create_symbol(&node, name, SymbolKind::Function, options))
}

pub fn extract_abstract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    // Same structure as method_declaration but no body
    extract_method(base, node, parent_id)
}

pub fn extract_constructor(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let params = helpers::extract_parameters(base, &node);

    let signature = format!("{}Sub New{}", helpers::modifier_prefix(&modifiers), params);
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, "New".to_string(), SymbolKind::Constructor, options))
}

pub fn extract_property(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!(
        "{}Property {}",
        helpers::modifier_prefix(&modifiers),
        name,
    );

    // Indexed property parameters
    if let Some(params_node) = node.child_by_field_name("parameters") {
        signature.push_str(&base.get_node_text(&params_node));
    }

    if let Some(prop_type) = helpers::extract_as_clause_type(base, &node) {
        signature.push_str(&format!(" As {}", prop_type));
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Property, options))
}

pub fn extract_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    // field_declaration has one or more variable_declarator children
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(&node);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name_node = child.child_by_field_name("name")?;
            let name = base.get_node_text(&name_node);

            let mut signature = format!(
                "{}Dim {}",
                helpers::modifier_prefix(&modifiers),
                name,
            );

            if let Some(field_type) = helpers::extract_as_clause_type(base, &child) {
                signature.push_str(&format!(" As {}", field_type));
            }

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility.clone()),
                parent_id: parent_id.clone(),
                doc_comment: doc_comment.clone(),
                ..Default::default()
            };

            // Return the first declarator as the symbol.
            // Multi-declarator fields (Dim a, b As Integer) produce one symbol for the first.
            return Some(base.create_symbol(&node, name, SymbolKind::Field, options));
        }
    }
    None
}

pub fn extract_event(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);

    let mut signature = format!(
        "{}Event {}",
        helpers::modifier_prefix(&modifiers),
        name,
    );

    if let Some(event_type) = helpers::extract_as_clause_type(base, &node) {
        signature.push_str(&format!(" As {}", event_type));
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Event, options))
}

pub fn extract_operator(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let op_node = node.child_by_field_name("operator")?;
    let op = base.get_node_text(&op_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let params = helpers::extract_parameters(base, &node);
    let return_type = helpers::extract_return_type(base, &node);

    let mut signature = format!(
        "{}Operator {}{}",
        helpers::modifier_prefix(&modifiers),
        op,
        params,
    );

    if let Some(ret) = &return_type {
        signature.push_str(&format!(" As {}", ret));
    }

    let doc_comment = base.find_doc_comment(&node);

    let name = format!("operator {}", op);
    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Operator, options))
}

pub fn extract_const(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    // const_declaration: modifiers? Const name [As type] = value
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(&node);

    // The first name/identifier in the commaSep1 list
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);

    let signature = format!(
        "{}Const {}",
        helpers::modifier_prefix(&modifiers),
        base.get_node_text(&node).lines().next().unwrap_or(&name),
    );

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Constant, options))
}

pub fn extract_declare(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = base.get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(base, &node);
    let visibility = helpers::determine_visibility(&modifiers);
    let params = helpers::extract_parameters(base, &node);
    let return_type = helpers::extract_return_type(base, &node);

    let lib_node = node.child_by_field_name("library");
    let lib = lib_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_default();

    let mut signature = format!(
        "{}Declare Function {} Lib {}{}",
        helpers::modifier_prefix(&modifiers),
        name,
        lib,
        params,
    );

    if let Some(ret) = &return_type {
        signature.push_str(&format!(" As {}", ret));
    }

    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id,
        doc_comment,
        ..Default::default()
    };

    Some(base.create_symbol(&node, name, SymbolKind::Function, options))
}
```

- [ ] **Step 5: Create the relationships module**

Create `crates/julie-extractors/src/vbnet/relationships.rs`:

```rust
use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::vbnet::VbNetExtractor;
use tree_sitter::Tree;

pub fn extract_relationships(
    extractor: &mut VbNetExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    visit_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

fn visit_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_block" | "structure_block" => {
            extract_inheritance_relationships(extractor, node, symbols, relationships);
        }
        "interface_block" => {
            extract_interface_inherits(extractor, node, symbols, relationships);
        }
        "invocation_expression" => {
            extract_call_relationships(extractor, node, symbols, relationships);
        }
        "member_access_expression" => {
            // Check if this is a method call (followed by argument_list)
            if let Some(parent) = node.parent() {
                if parent.kind() == "invocation_expression" {
                    // Handled by invocation_expression case above
                    // Skip to avoid double-processing
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_relationships(extractor, child, symbols, relationships);
    }
}

fn extract_inheritance_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let name = base.get_node_text(&name_node);
    let current_symbol = match symbols.iter().find(|s| s.name == name) {
        Some(s) => s,
        None => return,
    };
    let file_path = base.file_path.clone();
    let line_number = (node.start_position().row + 1) as u32;
    let current_id = current_symbol.id.clone();

    // Inherits clause
    let inherits = super::helpers::extract_inherits(base, &node);
    // Implements clause
    let implements = super::helpers::extract_implements(base, &node);

    // Process inherits (Extends relationship)
    for type_name in inherits {
        if let Some(target) = symbols.iter().find(|s| s.name == type_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_id,
                    target.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: current_id.clone(),
                to_symbol_id: target.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: None,
            });
        } else {
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: current_id.clone(),
                callee_name: type_name,
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
        }
    }

    // Process implements
    for type_name in implements {
        if let Some(target) = symbols.iter().find(|s| s.name == type_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_id,
                    target.id,
                    RelationshipKind::Implements,
                    node.start_position().row
                ),
                from_symbol_id: current_id.clone(),
                to_symbol_id: target.id.clone(),
                kind: RelationshipKind::Implements,
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: None,
            });
        } else {
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: current_id.clone(),
                callee_name: type_name,
                kind: RelationshipKind::Implements,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
        }
    }
}

fn extract_interface_inherits(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let name = base.get_node_text(&name_node);
    let current_symbol = match symbols.iter().find(|s| s.name == name) {
        Some(s) => s,
        None => return,
    };
    let file_path = base.file_path.clone();
    let line_number = (node.start_position().row + 1) as u32;
    let current_id = current_symbol.id.clone();

    // Interfaces use Inherits (not Implements) for interface inheritance
    let inherits = super::helpers::extract_inherits(base, &node);
    for type_name in inherits {
        if let Some(target) = symbols.iter().find(|s| s.name == type_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_id,
                    target.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: current_id.clone(),
                to_symbol_id: target.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: None,
            });
        } else {
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: current_id.clone(),
                callee_name: type_name,
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
        }
    }
}

fn extract_call_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();

    // Get the callee name from invocation_expression
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();

    let method_name = if let Some(first) = children.first() {
        match first.kind() {
            "identifier" => base.get_node_text(first),
            "member_access_expression" => {
                // Get the last identifier (method name) from dotted access
                let mut inner_cursor = first.walk();
                first
                    .children(&mut inner_cursor)
                    .filter(|c| c.kind() == "identifier")
                    .last()
                    .map(|n| base.get_node_text(&n))
                    .unwrap_or_default()
            }
            _ => String::new(),
        }
    } else {
        String::new()
    };

    if method_name.is_empty() {
        return;
    }

    // Find the containing method
    let symbol_map: std::collections::HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();
    let mut parent = node.parent();
    let mut caller_symbol = None;
    while let Some(p) = parent {
        if p.kind() == "method_declaration" || p.kind() == "abstract_method_declaration" {
            if let Some(name_node) = p.child_by_field_name("name") {
                let name = base.get_node_text(&name_node);
                caller_symbol = symbol_map.get(&name).copied();
                break;
            }
        }
        parent = p.parent();
    }

    let Some(caller) = caller_symbol else {
        return;
    };

    let line_number = node.start_position().row as u32 + 1;
    let file_path = base.file_path.clone();

    match symbol_map.get(&method_name) {
        Some(called) if called.kind == SymbolKind::Import => {
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: method_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.8,
            });
        }
        Some(called) => {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller.id,
                    called.id,
                    RelationshipKind::Calls,
                    node.start_position().row
                ),
                from_symbol_id: caller.id.clone(),
                to_symbol_id: called.id.clone(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: method_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.7,
            });
        }
    }
}
```

- [ ] **Step 6: Create the identifiers module**

Create `crates/julie-extractors/src/vbnet/identifiers.rs`:

```rust
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();
    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);
    base.identifiers.clone()
}

fn walk_tree_for_identifiers(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    extract_identifier_from_node(base, node, symbol_map);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(base, child, symbol_map);
    }
}

fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        "invocation_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let name = base.get_node_text(&child);
                    let containing = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(&child, name, IdentifierKind::Call, containing);
                    break;
                } else if child.kind() == "member_access_expression" {
                    // Get the method name (last identifier in dotted chain)
                    let mut inner_cursor = child.walk();
                    if let Some(name_node) = child
                        .children(&mut inner_cursor)
                        .filter(|c| c.kind() == "identifier")
                        .last()
                    {
                        let name = base.get_node_text(&name_node);
                        let containing = find_containing_symbol_id(base, node, symbol_map);
                        base.create_identifier(
                            &name_node,
                            name,
                            IdentifierKind::Call,
                            containing,
                        );
                    }
                    break;
                }
            }
        }
        "member_access_expression" => {
            // Skip if parent is invocation_expression (handled above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "invocation_expression" {
                    return;
                }
            }
            let mut cursor = node.walk();
            if let Some(name_node) = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .last()
            {
                let name = base.get_node_text(&name_node);
                let containing = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing,
                );
            }
        }
        _ => {}
    }
}

fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == base.file_path)
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
```

- [ ] **Step 7: Create the type_inference module**

Create `crates/julie-extractors/src/vbnet/type_inference.rs`:

```rust
use crate::base::Symbol;
use std::collections::HashMap;

pub fn infer_types(symbols: &[Symbol]) -> HashMap<String, String> {
    let mut type_map = HashMap::new();

    for symbol in symbols {
        let inferred_type = match symbol.kind {
            crate::base::SymbolKind::Function | crate::base::SymbolKind::Method => {
                infer_method_return_type(symbol)
            }
            crate::base::SymbolKind::Property => infer_property_type(symbol),
            crate::base::SymbolKind::Field | crate::base::SymbolKind::Constant => {
                infer_field_type(symbol)
            }
            _ => None,
        };

        if let Some(t) = inferred_type {
            type_map.insert(symbol.id.clone(), t);
        }
    }

    type_map
}

fn infer_method_return_type(symbol: &Symbol) -> Option<String> {
    // VB signatures look like: "Public Function DoWork(x As Integer) As String"
    // Extract the type after the last " As "
    let sig = symbol.signature.as_ref()?;
    // Only Functions have return types, Subs do not
    if !sig.contains("Function") {
        return None;
    }
    // Find " As " after the closing paren
    let paren_pos = sig.rfind(')')?;
    let after_paren = &sig[paren_pos..];
    let as_pos = after_paren.find(" As ")?;
    Some(after_paren[as_pos + 4..].trim().to_string())
}

fn infer_property_type(symbol: &Symbol) -> Option<String> {
    // "Public Property Name As String"
    let sig = symbol.signature.as_ref()?;
    let as_pos = sig.rfind(" As ")?;
    Some(sig[as_pos + 4..].trim().to_string())
}

fn infer_field_type(symbol: &Symbol) -> Option<String> {
    // "Private Dim _count As Integer"
    let sig = symbol.signature.as_ref()?;
    let as_pos = sig.rfind(" As ")?;
    Some(sig[as_pos + 4..].trim().to_string())
}
```

- [ ] **Step 8: Create the mod.rs (main extractor)**

Create `crates/julie-extractors/src/vbnet/mod.rs`:

```rust
pub(crate) mod helpers;
mod identifiers;
mod members;
mod relationships;
mod type_inference;
mod types;

use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, Symbol, SymbolKind,
};
use std::collections::HashMap;
use tree_sitter::Tree;

pub struct VbNetExtractor {
    base: BaseExtractor,
    pending_relationships: Vec<PendingRelationship>,
}

impl VbNetExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            pending_relationships: Vec::new(),
        }
    }

    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }

    pub(crate) fn get_base(&self) -> &BaseExtractor {
        &self.base
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        self.walk_tree(root, &mut symbols, None);
        symbols
    }

    fn walk_tree(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let symbol = self.extract_symbol(node, parent_id.clone());
        let current_parent_id = if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, current_parent_id.clone());
        }
    }

    fn extract_symbol(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<String>,
    ) -> Option<Symbol> {
        match node.kind() {
            "namespace_block" => types::extract_namespace(&mut self.base, node, parent_id),
            "imports_statement" => types::extract_imports(&mut self.base, node, parent_id),
            "class_block" => types::extract_class(&mut self.base, node, parent_id),
            "module_block" => types::extract_module(&mut self.base, node, parent_id),
            "structure_block" => types::extract_structure(&mut self.base, node, parent_id),
            "interface_block" => types::extract_interface(&mut self.base, node, parent_id),
            "enum_block" => types::extract_enum(&mut self.base, node, parent_id),
            "enum_member" => types::extract_enum_member(&mut self.base, node, parent_id),
            "delegate_declaration" => types::extract_delegate(&mut self.base, node, parent_id),
            "method_declaration" => members::extract_method(&mut self.base, node, parent_id),
            "abstract_method_declaration" => {
                members::extract_abstract_method(&mut self.base, node, parent_id)
            }
            "constructor_declaration" => {
                members::extract_constructor(&mut self.base, node, parent_id)
            }
            "property_declaration" => members::extract_property(&mut self.base, node, parent_id),
            "field_declaration" => members::extract_field(&mut self.base, node, parent_id),
            "event_declaration" => members::extract_event(&mut self.base, node, parent_id),
            "operator_declaration" => members::extract_operator(&mut self.base, node, parent_id),
            "const_declaration" => members::extract_const(&mut self.base, node, parent_id),
            "declare_statement" => members::extract_declare(&mut self.base, node, parent_id),
            _ => None,
        }
    }

    pub fn extract_relationships(
        &mut self,
        tree: &Tree,
        symbols: &[Symbol],
    ) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    pub fn extract_identifiers(
        &mut self,
        tree: &Tree,
        symbols: &[Symbol],
    ) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        type_inference::infer_types(symbols)
    }
}
```

- [ ] **Step 9: Register the module in lib.rs**

Add `pub mod vbnet` in `crates/julie-extractors/src/lib.rs` in the language modules section, after `pub mod vue` (line 68):

```rust
pub mod vbnet;
```

- [ ] **Step 10: Verify the scaffold compiles**

Run: `cargo build -p julie-extractors 2>&1 | tail -20`
Expected: Successful compilation (warnings are OK at this stage)

- [ ] **Step 11: Commit**

```bash
git add crates/julie-extractors/Cargo.toml crates/julie-extractors/src/vbnet/ crates/julie-extractors/src/lib.rs
git commit -m "feat(vbnet): scaffold VB.NET extractor module with full extraction pipeline"
```

---

### Task 2: Register VB.NET in language detection and factory

**Files:**
- Modify: `crates/julie-extractors/src/language.rs`
- Modify: `crates/julie-extractors/src/factory.rs`
- Modify: `crates/julie-extractors/src/test_detection.rs`

- [ ] **Step 1: Update `get_tree_sitter_language` in `language.rs`**

Add in the "Backend languages" section, after the `"dart"` arm:

```rust
// Visual Basic .NET
"vbnet" => Ok(tree_sitter_vb_dotnet::LANGUAGE.into()),
```

- [ ] **Step 2: Update `detect_language_from_extension` in `language.rs`**

Add in the "Backend" section, after `"dart"`:

```rust
"vb" => Some("vbnet"),
```

- [ ] **Step 3: Update `supported_extensions` in `language.rs`**

Add `"vb"` after `"dart"` in the Backend section.

- [ ] **Step 4: Update `supported_languages` in `language.rs`**

Add `"vbnet"` after `"dart"` in the Backend section.

- [ ] **Step 5: Update `get_function_node_kinds` in `language.rs`**

Add a new arm:

```rust
"vbnet" => vec!["method_declaration", "abstract_method_declaration"],
```

- [ ] **Step 6: Update `get_import_node_kinds` in `language.rs`**

Add:

```rust
"vbnet" => vec!["imports_statement"],
```

- [ ] **Step 7: Update `get_symbol_node_kinds` in `language.rs`**

Add:

```rust
"vbnet" => vec![
    "method_declaration",
    "abstract_method_declaration",
    "class_block",
    "module_block",
    "interface_block",
    "structure_block",
    "enum_block",
],
```

- [ ] **Step 8: Update the error message in `get_tree_sitter_language`**

The error message string lists all supported languages. Add `vbnet` to it.

- [ ] **Step 9: Add VB.NET arm to factory.rs**

In `extract_symbols_and_relationships`, add a new arm after the C# arm (or in alphabetical position near the end). Add before the `_ =>` fallback:

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

- [ ] **Step 10: Update test_detection.rs**

In the `is_test_symbol` function, find the line:

```rust
"csharp" | "razor" => detect_csharp(attributes),
```

Change it to:

```rust
"csharp" | "razor" | "vbnet" => detect_csharp(attributes),
```

Also update `detect_csharp` to strip angle brackets (VB uses `<Test>` not `[Test]`):

In the `detect_csharp` function, change the stripping logic from:

```rust
let stripped = a.strip_prefix('[').unwrap_or(a);
let stripped = stripped.strip_suffix(']').unwrap_or(stripped);
```

To:

```rust
let stripped = a.strip_prefix('[').or_else(|| a.strip_prefix('<')).unwrap_or(a);
let stripped = stripped.strip_suffix(']').or_else(|| stripped.strip_suffix('>')).unwrap_or(stripped);
```

- [ ] **Step 11: Verify compilation**

Run: `cargo build -p julie-extractors 2>&1 | tail -20`
Expected: Successful compilation

- [ ] **Step 12: Commit**

```bash
git add crates/julie-extractors/src/language.rs crates/julie-extractors/src/factory.rs crates/julie-extractors/src/test_detection.rs
git commit -m "feat(vbnet): register VB.NET in language detection, factory, and test detection"
```

---

### Task 3: Write core symbol extraction tests

**Files:**
- Create: `crates/julie-extractors/src/tests/vbnet/mod.rs`
- Create: `crates/julie-extractors/src/tests/vbnet/core.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs`

- [ ] **Step 1: Create the test module scaffold**

Create `crates/julie-extractors/src/tests/vbnet/mod.rs`:

```rust
use crate::base::{Symbol, SymbolKind, Visibility};
use crate::vbnet::VbNetExtractor;
use tree_sitter::Parser;

pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_vb_dotnet::LANGUAGE.into())
        .expect("Error loading VB.NET grammar");
    parser
}

pub mod core;
```

- [ ] **Step 2: Register the test module**

In `crates/julie-extractors/src/tests/mod.rs`, add after `pub mod vue` (line 37):

```rust
pub mod vbnet;
```

- [ ] **Step 3: Write core symbol extraction tests**

Create `crates/julie-extractors/src/tests/vbnet/core.rs`:

```rust
use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_and_imports_extraction() {
        let code = r#"
Imports System
Imports System.Collections.Generic
Imports MyAlias = Some.Other.Namespace

Namespace MyCompany.MyProject
End Namespace
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let system_import = symbols.iter().find(|s| s.name == "System" && s.kind == SymbolKind::Import);
        assert!(system_import.is_some(), "Should find System import");

        let generic_import = symbols.iter().find(|s| s.name == "Generic");
        assert!(generic_import.is_some(), "Should find Generic import");

        let alias_import = symbols.iter().find(|s| s.name == "MyAlias");
        assert!(alias_import.is_some(), "Should find aliased import");
        assert!(
            alias_import.unwrap().signature.as_ref().unwrap().contains("Imports MyAlias ="),
            "Aliased import should have alias in signature"
        );

        let namespace = symbols.iter().find(|s| s.name == "MyCompany.MyProject");
        assert!(namespace.is_some(), "Should find namespace");
        assert_eq!(namespace.unwrap().kind, SymbolKind::Namespace);
    }

    #[test]
    fn test_class_extraction() {
        let code = r#"
Namespace TestNS

Public Class MyClass
    Inherits BaseClass
    Implements IDisposable
End Class

Public MustInherit Class AbstractBase
End Class

End Namespace
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let my_class = symbols.iter().find(|s| s.name == "MyClass");
        assert!(my_class.is_some(), "Should find MyClass");
        let mc = my_class.unwrap();
        assert_eq!(mc.kind, SymbolKind::Class);
        assert!(mc.signature.as_ref().unwrap().contains("Inherits BaseClass"));
        assert!(mc.signature.as_ref().unwrap().contains("Implements IDisposable"));

        let abstract_class = symbols.iter().find(|s| s.name == "AbstractBase");
        assert!(abstract_class.is_some(), "Should find AbstractBase");
    }

    #[test]
    fn test_module_extraction() {
        let code = r#"
Public Module StringHelpers
End Module
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let module = symbols.iter().find(|s| s.name == "StringHelpers");
        assert!(module.is_some(), "Should find Module");
        let m = module.unwrap();
        assert_eq!(m.kind, SymbolKind::Class, "Module maps to Class");
        assert_eq!(
            m.metadata.as_ref().and_then(|md| md.get("vb_module")).and_then(|v| v.as_bool()),
            Some(true),
            "Module should have vb_module metadata"
        );
    }

    #[test]
    fn test_structure_extraction() {
        let code = r#"
Public Structure Point
    Implements IEquatable(Of Point)
End Structure
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let structure = symbols.iter().find(|s| s.name == "Point");
        assert!(structure.is_some(), "Should find Structure");
        assert_eq!(structure.unwrap().kind, SymbolKind::Struct);
    }

    #[test]
    fn test_interface_extraction() {
        let code = r#"
Public Interface IRepository(Of T)
    Inherits IDisposable
End Interface
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let iface = symbols.iter().find(|s| s.name == "IRepository");
        assert!(iface.is_some(), "Should find Interface");
        let i = iface.unwrap();
        assert_eq!(i.kind, SymbolKind::Interface);
        assert!(i.signature.as_ref().unwrap().contains("(Of T)"));
        assert!(i.signature.as_ref().unwrap().contains("Inherits IDisposable"));
    }

    #[test]
    fn test_enum_extraction() {
        let code = r#"
Public Enum Color
    Red
    Green = 1
    Blue = 2
End Enum
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let color_enum = symbols.iter().find(|s| s.name == "Color");
        assert!(color_enum.is_some(), "Should find Enum");
        assert_eq!(color_enum.unwrap().kind, SymbolKind::Enum);

        let red = symbols.iter().find(|s| s.name == "Red");
        assert!(red.is_some(), "Should find enum member Red");
        assert_eq!(red.unwrap().kind, SymbolKind::EnumMember);

        let green = symbols.iter().find(|s| s.name == "Green");
        assert!(green.is_some(), "Should find enum member Green");
        assert!(green.unwrap().signature.as_ref().unwrap().contains("= 1"));
    }

    #[test]
    fn test_delegate_extraction() {
        let code = r#"
Public Delegate Function Converter(Of TInput, TOutput)(input As TInput) As TOutput
Public Delegate Sub Handler(sender As Object, e As EventArgs)
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let converter = symbols.iter().find(|s| s.name == "Converter");
        assert!(converter.is_some(), "Should find Function delegate");
        assert_eq!(converter.unwrap().kind, SymbolKind::Delegate);

        let handler = symbols.iter().find(|s| s.name == "Handler");
        assert!(handler.is_some(), "Should find Sub delegate");
        assert_eq!(handler.unwrap().kind, SymbolKind::Delegate);
    }
}
```

- [ ] **Step 4: Run the core tests**

Run: `cargo test -p julie-extractors --lib tests::vbnet::core 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/julie-extractors/src/tests/vbnet/ crates/julie-extractors/src/tests/mod.rs
git commit -m "test(vbnet): add core symbol extraction tests for types, namespace, imports"
```

---

### Task 4: Write member extraction tests

**Files:**
- Create: `crates/julie-extractors/src/tests/vbnet/members.rs`
- Modify: `crates/julie-extractors/src/tests/vbnet/mod.rs`

- [ ] **Step 1: Write member tests**

Create `crates/julie-extractors/src/tests/vbnet/members.rs`:

```rust
use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_extraction() {
        let code = r#"
Public Class Calculator
    Public Function Add(a As Integer, b As Integer) As Integer
        Return a + b
    End Function

    Public Sub Reset()
    End Sub

    Private Shared Function Parse(input As String) As Integer
        Return Integer.Parse(input)
    End Function
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let add = symbols.iter().find(|s| s.name == "Add");
        assert!(add.is_some(), "Should find Function Add");
        let add = add.unwrap();
        assert_eq!(add.kind, SymbolKind::Function);
        assert!(add.signature.as_ref().unwrap().contains("Function"));
        assert!(add.signature.as_ref().unwrap().contains("As Integer"));

        let reset = symbols.iter().find(|s| s.name == "Reset");
        assert!(reset.is_some(), "Should find Sub Reset");
        assert!(reset.unwrap().signature.as_ref().unwrap().contains("Sub"));

        let parse = symbols.iter().find(|s| s.name == "Parse");
        assert!(parse.is_some(), "Should find Shared Function Parse");
    }

    #[test]
    fn test_constructor_extraction() {
        let code = r#"
Public Class Service
    Public Sub New()
    End Sub

    Public Sub New(name As String, count As Integer)
    End Sub
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let constructors: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constructor)
            .collect();
        assert!(constructors.len() >= 1, "Should find at least one constructor");
        assert!(constructors[0].signature.as_ref().unwrap().contains("Sub New"));
    }

    #[test]
    fn test_property_extraction() {
        let code = r#"
Public Class Person
    Public Property Name As String
    Public ReadOnly Property Age As Integer

    Public Property Item(index As Integer) As String
        Get
            Return ""
        End Get
        Set(value As String)
        End Set
    End Property
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let name_prop = symbols.iter().find(|s| s.name == "Name" && s.kind == SymbolKind::Property);
        assert!(name_prop.is_some(), "Should find Name property");
        assert!(name_prop.unwrap().signature.as_ref().unwrap().contains("As String"));

        let age_prop = symbols.iter().find(|s| s.name == "Age" && s.kind == SymbolKind::Property);
        assert!(age_prop.is_some(), "Should find readonly Age property");

        let item_prop = symbols.iter().find(|s| s.name == "Item" && s.kind == SymbolKind::Property);
        assert!(item_prop.is_some(), "Should find indexed Item property");
    }

    #[test]
    fn test_field_extraction() {
        let code = r#"
Public Class DataStore
    Private _count As Integer
    Public Shared MaxSize As Integer
    Private Dim _name As String
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let count = symbols.iter().find(|s| s.name == "_count");
        assert!(count.is_some(), "Should find _count field");
        assert_eq!(count.unwrap().kind, SymbolKind::Field);

        let max_size = symbols.iter().find(|s| s.name == "MaxSize");
        assert!(max_size.is_some(), "Should find MaxSize field");
    }

    #[test]
    fn test_event_extraction() {
        let code = r#"
Public Class Button
    Public Event Click As EventHandler
    Public Event ValueChanged(sender As Object, value As Integer)
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let click = symbols.iter().find(|s| s.name == "Click");
        assert!(click.is_some(), "Should find Click event");
        assert_eq!(click.unwrap().kind, SymbolKind::Event);
    }

    #[test]
    fn test_operator_extraction() {
        let code = r#"
Public Structure Money
    Public Shared Operator +(a As Money, b As Money) As Money
        Return New Money()
    End Operator
End Structure
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let op = symbols.iter().find(|s| s.kind == SymbolKind::Operator);
        assert!(op.is_some(), "Should find operator");
    }

    #[test]
    fn test_const_extraction() {
        let code = r#"
Public Class Config
    Public Const MaxRetries As Integer = 3
    Private Const DefaultName As String = "test"
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let max_retries = symbols.iter().find(|s| s.name == "MaxRetries");
        assert!(max_retries.is_some(), "Should find Const MaxRetries");
        assert_eq!(max_retries.unwrap().kind, SymbolKind::Constant);
    }
}
```

- [ ] **Step 2: Register the test module**

Add to `crates/julie-extractors/src/tests/vbnet/mod.rs`:

```rust
pub mod members;
```

- [ ] **Step 3: Run the member tests**

Run: `cargo test -p julie-extractors --lib tests::vbnet::members 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/julie-extractors/src/tests/vbnet/
git commit -m "test(vbnet): add member extraction tests for methods, properties, fields, events"
```

---

### Task 5: Write relationship and identifier tests

**Files:**
- Create: `crates/julie-extractors/src/tests/vbnet/relationships.rs`
- Create: `crates/julie-extractors/src/tests/vbnet/identifiers.rs`
- Modify: `crates/julie-extractors/src/tests/vbnet/mod.rs`

- [ ] **Step 1: Write relationship tests**

Create `crates/julie-extractors/src/tests/vbnet/relationships.rs`:

```rust
use super::*;
use crate::base::RelationshipKind;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inheritance_relationships() {
        let code = r#"
Public Class Animal
End Class

Public Class Dog
    Inherits Animal
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let extends = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Extends);
        assert!(extends.is_some(), "Should find Extends relationship");

        let dog = symbols.iter().find(|s| s.name == "Dog").unwrap();
        let animal = symbols.iter().find(|s| s.name == "Animal").unwrap();
        let ext = extends.unwrap();
        assert_eq!(ext.from_symbol_id, dog.id);
        assert_eq!(ext.to_symbol_id, animal.id);
    }

    #[test]
    fn test_implements_relationships() {
        let code = r#"
Public Interface IAnimal
End Interface

Public Class Dog
    Implements IAnimal
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let implements = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Implements);
        assert!(implements.is_some(), "Should find Implements relationship");
    }

    #[test]
    fn test_call_relationships() {
        let code = r#"
Public Class Service
    Public Function Process() As String
        Return Format("done")
    End Function

    Private Function Format(input As String) As String
        Return input.ToUpper()
    End Function
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let calls = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Calls);
        assert!(calls.is_some(), "Should find Calls relationship for Format()");
    }

    #[test]
    fn test_cross_file_pending_relationships() {
        let code = r#"
Public Class Service
    Inherits ExternalBase
    Implements IExternalInterface
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        assert!(pending.len() >= 2, "Should have pending relationships for cross-file types");
        assert!(
            pending.iter().any(|p| p.callee_name == "ExternalBase"),
            "Should have pending for ExternalBase"
        );
        assert!(
            pending.iter().any(|p| p.callee_name == "IExternalInterface"),
            "Should have pending for IExternalInterface"
        );
    }
}
```

- [ ] **Step 2: Write identifier tests**

Create `crates/julie-extractors/src/tests/vbnet/identifiers.rs`:

```rust
use super::*;
use crate::base::IdentifierKind;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_identifiers() {
        let code = r#"
Public Class Service
    Public Sub Process()
        DoWork()
        Helper.Format("test")
    End Sub

    Private Sub DoWork()
    End Sub
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let call_ids: Vec<_> = identifiers
            .iter()
            .filter(|i| i.kind == IdentifierKind::Call)
            .collect();
        assert!(!call_ids.is_empty(), "Should find call identifiers");

        let dowork_call = call_ids.iter().find(|i| i.name == "DoWork");
        assert!(dowork_call.is_some(), "Should find DoWork call identifier");
    }

    #[test]
    fn test_member_access_identifiers() {
        let code = r#"
Public Class Viewer
    Public Sub Show()
        Dim x = Console.Title
    End Sub
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let member_access: Vec<_> = identifiers
            .iter()
            .filter(|i| i.kind == IdentifierKind::MemberAccess)
            .collect();
        assert!(!member_access.is_empty(), "Should find member access identifiers");
    }
}
```

- [ ] **Step 3: Register the test modules**

Add to `crates/julie-extractors/src/tests/vbnet/mod.rs`:

```rust
pub mod relationships;
pub mod identifiers;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p julie-extractors --lib tests::vbnet 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/julie-extractors/src/tests/vbnet/
git commit -m "test(vbnet): add relationship and identifier extraction tests"
```

---

### Task 6: Write type inference tests and run full validation

**Files:**
- Create: `crates/julie-extractors/src/tests/vbnet/types.rs`
- Modify: `crates/julie-extractors/src/tests/vbnet/mod.rs`

- [ ] **Step 1: Write type inference tests**

Create `crates/julie-extractors/src/tests/vbnet/types.rs`:

```rust
use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_return_type_inference() {
        let code = r#"
Public Class Service
    Public Function GetName() As String
        Return ""
    End Function

    Public Sub Reset()
    End Sub
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let get_name = symbols.iter().find(|s| s.name == "GetName").unwrap();
        assert_eq!(
            types.get(&get_name.id),
            Some(&"String".to_string()),
            "Should infer String return type for GetName"
        );

        let reset = symbols.iter().find(|s| s.name == "Reset").unwrap();
        assert!(
            types.get(&reset.id).is_none(),
            "Sub should not have inferred return type"
        );
    }

    #[test]
    fn test_property_type_inference() {
        let code = r#"
Public Class Person
    Public Property Name As String
    Public Property Age As Integer
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let name_prop = symbols.iter().find(|s| s.name == "Name" && s.kind == SymbolKind::Property).unwrap();
        assert_eq!(types.get(&name_prop.id), Some(&"String".to_string()));

        let age_prop = symbols.iter().find(|s| s.name == "Age" && s.kind == SymbolKind::Property).unwrap();
        assert_eq!(types.get(&age_prop.id), Some(&"Integer".to_string()));
    }

    #[test]
    fn test_field_type_inference() {
        let code = r#"
Public Class Data
    Private _count As Integer
    Public Shared MaxSize As Long
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let count = symbols.iter().find(|s| s.name == "_count").unwrap();
        assert_eq!(types.get(&count.id), Some(&"Integer".to_string()));
    }
}
```

- [ ] **Step 2: Register the test module**

Add to `crates/julie-extractors/src/tests/vbnet/mod.rs`:

```rust
pub mod types;
```

- [ ] **Step 3: Run all VB.NET tests**

Run: `cargo test -p julie-extractors --lib tests::vbnet 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 4: Run the dev test tier for regressions**

Run: `cargo xtask test dev 2>&1 | tail -30`
Expected: All existing tests still pass

- [ ] **Step 5: Commit**

```bash
git add crates/julie-extractors/src/tests/vbnet/
git commit -m "test(vbnet): add type inference tests and validate full extraction pipeline"
```

---

### Task 7: Update documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `crates/julie-extractors/src/lib.rs` (doc comments)

- [ ] **Step 1: Update language count in CLAUDE.md**

Find all occurrences of "33" referring to language count and update to "34". Key locations:
- The "Key Project Facts" section: `33 tree-sitter extractors` → `34 tree-sitter extractors`
- The "Crown Jewels" line: `33 tree-sitter extractors` → `34 tree-sitter extractors`
- The "Current Language Support" header: `33 - Complete` → `34 - Complete`
- Add VB.NET to the language list under an appropriate category

- [ ] **Step 2: Update language count in lib.rs doc comments**

In `crates/julie-extractors/src/lib.rs`, update the description line that says "33 programming languages" to "34 programming languages".

- [ ] **Step 3: Update Cargo.toml description**

In `crates/julie-extractors/Cargo.toml`, update the description from "33 programming" to "34 programming".

- [ ] **Step 4: Update the `fast_search` language filter doc**

The `fast_search` tool has a `language` parameter with a list of supported languages in its description. This is defined in the tool's Rust code. Search for the language list and add `"vbnet"`.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md crates/julie-extractors/src/lib.rs crates/julie-extractors/Cargo.toml
git commit -m "docs: update language count to 34, add VB.NET to supported languages"
```

---

### Task 8: Final integration verification

**Files:** None (verification only)

- [ ] **Step 1: Run full VB.NET test suite**

Run: `cargo test -p julie-extractors --lib tests::vbnet 2>&1 | tail -30`
Expected: All VB.NET tests pass

- [ ] **Step 2: Run factory consistency test**

Run: `cargo test -p julie-extractors --lib factory_consistency 2>&1 | tail -20`
Expected: The factory test that checks all languages have extractors passes

- [ ] **Step 3: Run the dev test tier**

Run: `cargo xtask test dev 2>&1 | tail -30`
Expected: All tests pass, no regressions

- [ ] **Step 4: Verify the grammar parses real VB.NET code**

Run a quick manual test to confirm the extractor works on a realistic VB.NET file:

```bash
cargo test -p julie-extractors --lib tests::vbnet 2>&1 | grep -E "test result|FAILED"
```

Expected: `test result: ok` with zero failures
