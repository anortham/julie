/// Class, interface, enum, and record extraction
use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::java::JavaExtractor;
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers;

/// Extract class declaration from a node
pub(super) fn extract_class(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("class {}", name)
    } else {
        format!("{} class {}", modifiers.join(" "), name)
    };

    // Handle generic type parameters
    if let Some(type_params) = helpers::extract_type_parameters(extractor.base(), node) {
        signature = signature.replace(
            &format!("class {}", name),
            &format!("class {}{}", name, type_params),
        );
    }

    // Check for inheritance and implementations
    let superclass = helpers::extract_superclass(extractor.base(), node);
    if let Some(ref superclass) = superclass {
        signature.push_str(&format!(" extends {}", superclass));
    }

    let interfaces = helpers::extract_implemented_interfaces(extractor.base(), node);
    if !interfaces.is_empty() {
        signature.push_str(&format!(" implements {}", interfaces.join(", ")));
    }

    // Handle sealed class permits clause (Java 17+)
    if let Some(permits_clause) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "permits")
    {
        signature.push_str(&format!(
            " {}",
            extractor.base().get_node_text(&permits_clause)
        ));
    }

    // Class-level annotations (e.g. JUnit 5 `@Nested` test containers). The class
    // extractor previously dropped these — capturing them lets the test-role
    // classifier recognize annotated container classes.
    let annotations = helpers::extract_annotations(extractor.base(), node);

    // Canonical base-type signal (Miller bridge test-roles): superclass +
    // implemented interfaces. Lets `src/analysis/test_roles.rs` flag a JUnit 3
    // `extends TestCase` class as a TestContainer with no annotation.
    let mut base_types: Vec<String> = Vec::new();
    if let Some(superclass) = superclass {
        base_types.push(superclass);
    }
    base_types.extend(interfaces);
    let mut metadata = HashMap::new();
    if !base_types.is_empty() {
        metadata.insert("base_types".to_string(), serde_json::json!(base_types));
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        metadata: if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        },
        doc_comment,
        annotations,
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Class, options),
    )
}

/// Extract interface declaration from a node
pub(super) fn extract_interface(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("interface {}", name)
    } else {
        format!("{} interface {}", modifiers.join(" "), name)
    };

    // Check for interface inheritance (extends)
    let super_interfaces = helpers::extract_extended_interfaces(extractor.base(), node);
    if !super_interfaces.is_empty() {
        signature.push_str(&format!(" extends {}", super_interfaces.join(", ")));
    }

    // Handle generic type parameters
    if let Some(type_params) = helpers::extract_type_parameters(extractor.base(), node) {
        signature = signature.replace(
            &format!("interface {}", name),
            &format!("interface {}{}", name, type_params),
        );
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Interface, options),
    )
}

/// Extract enum declaration from a node
pub(super) fn extract_enum(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("enum {}", name)
    } else {
        format!("{} enum {}", modifiers.join(" "), name)
    };

    // Check for interface implementations (enums can implement interfaces)
    let interfaces = helpers::extract_implemented_interfaces(extractor.base(), node);
    if !interfaces.is_empty() {
        signature.push_str(&format!(" implements {}", interfaces.join(", ")));
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Enum, options),
    )
}

/// Extract enum constant from a node
pub(super) fn extract_enum_constant(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);

    // Build signature - include arguments if present
    let mut signature = name.clone();
    let argument_list = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "argument_list");
    if let Some(args) = argument_list {
        signature.push_str(&extractor.base().get_node_text(&args));
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public), // Enum constants are always public in Java
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::EnumMember, options),
    )
}

/// Extract record declaration from a node
pub(super) fn extract_record(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Get record parameters (record components)
    let param_list = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "formal_parameters");
    let params = param_list
        .map(|p| extractor.base().get_node_text(&p))
        .unwrap_or_else(|| "()".to_string());

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("record {}{}", name, params)
    } else {
        format!("{} record {}{}", modifiers.join(" "), name, params)
    };

    // Handle generic type parameters
    if let Some(type_params) = helpers::extract_type_parameters(extractor.base(), node) {
        signature = signature.replace(
            &format!("record {}", name),
            &format!("record {}{}", name, type_params),
        );
    }

    // Check for interface implementations (records can implement interfaces)
    let interfaces = helpers::extract_implemented_interfaces(extractor.base(), node);
    if !interfaces.is_empty() {
        signature.push_str(&format!(" implements {}", interfaces.join(", ")));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("record".to_string()),
    );

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        metadata: Some(metadata),
        doc_comment,
        annotations: Vec::new(),
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Class, options),
    )
}

pub(super) fn extract_record_components(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Vec<Symbol> {
    let Some(param_list) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "formal_parameters")
    else {
        return Vec::new();
    };

    let mut components = Vec::new();
    let mut cursor = param_list.walk();
    for parameter in param_list.children(&mut cursor) {
        if parameter.kind() != "formal_parameter" {
            continue;
        }

        let name_node = parameter.child_by_field_name("name").or_else(|| {
            parameter
                .children(&mut parameter.walk())
                .find(|child| child.kind() == "identifier")
        });
        let Some(name_node) = name_node else {
            continue;
        };

        let name = extractor.base().get_node_text(&name_node);
        let component_type = parameter
            .child_by_field_name("type")
            .map(|node| extractor.base().get_node_text(&node))
            .unwrap_or_else(|| "Object".to_string());
        let signature = format!("{} {}", component_type, name);
        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            ..Default::default()
        };

        components.push(extractor.base_mut().create_symbol(
            &parameter,
            name,
            SymbolKind::Property,
            options,
        ));
    }

    components
}
