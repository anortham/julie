// Tests for Lua class pattern detection functionality that post-processes
// symbols to detect Lua class patterns:
// - Tables with metatable setup (local Class = {})
// - Variables created with setmetatable (local Dog = setmetatable({}, Animal))
// - Tables with __index pattern (Class.__index = Class)
// - Tables with new and colon methods (Class.new, Class:method)
// - Variables created with :extend() call (middleclass, classic, 30log patterns)

use super::*;
use std::path::PathBuf;

#[test]
fn test_class_detection_basic() {
    // Tests are run through the integration test suite
    // This ensures consistency with full extraction pipeline
}

/// Test that `Object:extend()` pattern is detected as a class.
///
/// In rxi/lite and middleclass-style Lua OOP, classes are created via:
///   local Doc = Object:extend()
///   function Doc:insert(line, col, text) end
///
/// The `Doc` variable should be upgraded to Class kind, and `insert` should
/// be a method with Doc as parent.
#[test]
fn test_extend_pattern_detected_as_class() {
    let code = r#"
local Object = {}
Object.__index = Object

function Object:extend()
    local cls = {}
    setmetatable(cls, self)
    cls.__index = cls
    return cls
end

local Doc = Object:extend()

function Doc:insert(line, col, text)
    -- insert text at position
end

function Doc:remove(line, col, len)
    -- remove text at position
end
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = LuaExtractor::new(
        "lua".to_string(),
        "doc.lua".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // Object should be detected as a class (has __index pattern + colon methods)
    let object = symbols.iter().find(|s| s.name == "Object");
    assert!(object.is_some(), "Object symbol not found");
    assert_eq!(
        object.unwrap().kind,
        SymbolKind::Class,
        "Object should be detected as Class"
    );

    // Doc should be detected as a class via :extend() pattern
    let doc = symbols.iter().find(|s| s.name == "Doc");
    assert!(doc.is_some(), "Doc symbol not found");
    assert_eq!(
        doc.unwrap().kind,
        SymbolKind::Class,
        "Doc should be detected as Class via :extend() pattern, got {:?}",
        doc.unwrap().kind
    );

    // Doc should have Object as its base class
    let doc_sym = doc.unwrap();
    let base_class = doc_sym
        .metadata
        .as_ref()
        .and_then(|m| m.get("baseClass"))
        .and_then(|v| v.as_str());
    assert_eq!(
        base_class,
        Some("Object"),
        "Doc should have Object as baseClass"
    );

    // Doc:insert should be a method with Doc as parent
    let insert = symbols
        .iter()
        .find(|s| s.name == "insert" && s.parent_id == Some(doc_sym.id.clone()));
    assert!(insert.is_some(), "Doc:insert method not found");
    assert_eq!(insert.unwrap().kind, SymbolKind::Method);

    // Doc:remove should also be a method
    let remove = symbols
        .iter()
        .find(|s| s.name == "remove" && s.parent_id == Some(doc_sym.id.clone()));
    assert!(remove.is_some(), "Doc:remove method not found");
    assert_eq!(remove.unwrap().kind, SymbolKind::Method);
}

/// Test extend pattern with multiple levels of inheritance.
#[test]
fn test_extend_pattern_inheritance_chain() {
    let code = r#"
local Object = {}
Object.__index = Object

function Object:extend()
    local cls = {}
    setmetatable(cls, self)
    cls.__index = cls
    return cls
end

local View = Object:extend()

function View:draw()
end

local CommandView = View:extend()

function CommandView:enter()
end
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = LuaExtractor::new(
        "lua".to_string(),
        "views.lua".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // View should be a class with Object as parent
    let view = symbols.iter().find(|s| s.name == "View");
    assert!(view.is_some(), "View symbol not found");
    assert_eq!(view.unwrap().kind, SymbolKind::Class);

    let view_base = view
        .unwrap()
        .metadata
        .as_ref()
        .and_then(|m| m.get("baseClass"))
        .and_then(|v| v.as_str());
    assert_eq!(
        view_base,
        Some("Object"),
        "View should have Object as baseClass"
    );

    // CommandView should be a class with View as parent
    let cmd_view = symbols.iter().find(|s| s.name == "CommandView");
    assert!(cmd_view.is_some(), "CommandView symbol not found");
    assert_eq!(cmd_view.unwrap().kind, SymbolKind::Class);

    let cmd_base = cmd_view
        .unwrap()
        .metadata
        .as_ref()
        .and_then(|m| m.get("baseClass"))
        .and_then(|v| v.as_str());
    assert_eq!(
        cmd_base,
        Some("View"),
        "CommandView should have View as baseClass"
    );
}
