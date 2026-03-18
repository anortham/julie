// C Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (call_expression)
// - Member access (field_expression)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

use crate::base::IdentifierKind;
use crate::c::CExtractor;
use crate::tests::c::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_extract_function_calls() {
        let c_code = r#"
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int calculate() {
    int result = add(5, 3);      // Function call to add
    printf("Result: %d\n", result);    // Function call to printf
    return result;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the function calls
        let add_call = identifiers.iter().find(|id| id.name == "add");
        assert!(
            add_call.is_some(),
            "Should extract 'add' function call identifier"
        );
        let add_call = add_call.unwrap();
        assert_eq!(add_call.kind, IdentifierKind::Call);

        let printf_call = identifiers.iter().find(|id| id.name == "printf");
        assert!(
            printf_call.is_some(),
            "Should extract 'printf' function call identifier"
        );
        let printf_call = printf_call.unwrap();
        assert_eq!(printf_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside calculate function)
        assert!(
            add_call.containing_symbol_id.is_some(),
            "Function call should have containing symbol"
        );

        // Find the calculate function symbol
        let calculate_function = symbols.iter().find(|s| s.name == "calculate").unwrap();

        // Verify the add call is contained within calculate function
        assert_eq!(
            add_call.containing_symbol_id.as_ref(),
            Some(&calculate_function.id),
            "add call should be contained within calculate function"
        );
    }

    #[test]
    fn test_extract_member_access() {
        let c_code = r#"
typedef struct {
    int x;
    int y;
} Point;

void print_point(Point* p) {
    printf("x: %d\n", p->x);   // Member access: p->x
    printf("y: %d\n", p->y);   // Member access: p->y
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found member access identifiers
        let x_access = identifiers
            .iter()
            .filter(|id| id.name == "x" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(x_access > 0, "Should extract 'x' member access identifier");

        let y_access = identifiers
            .iter()
            .filter(|id| id.name == "y" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(y_access > 0, "Should extract 'y' member access identifier");
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
        let c_code = r#"
void helper() {
    // Helper function
}

void process() {
    helper();              // Call to helper in same file
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the helper call
        let helper_call = identifiers.iter().find(|id| id.name == "helper");
        assert!(helper_call.is_some());
        let helper_call = helper_call.unwrap();

        // Verify it has a containing symbol (the process function)
        assert!(
            helper_call.containing_symbol_id.is_some(),
            "helper call should have containing symbol from same file"
        );

        // Verify the containing symbol is the process function
        let process_function = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(
            helper_call.containing_symbol_id.as_ref(),
            Some(&process_function.id),
            "helper call should be contained within process function"
        );
    }

    #[test]
    fn test_chained_member_access() {
        let c_code = r#"
typedef struct {
    int balance;
} Account;

typedef struct {
    Account* account;
} User;

void execute(User* user) {
    int balance = user->account->balance;   // Chained member access
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifier in the chain
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained member access"
        );
    }

    #[test]
    fn test_c_type_usage_identifiers() {
        // C type references should produce TypeUsage identifiers.
        // Same bug fixed in TypeScript, Scala, GDScript, Zig extractors.
        let c_code = r#"
typedef struct {
    int x;
    int y;
} Point;

typedef struct {
    Point center;       // field type → TypeUsage for Point
    float radius;
} Circle;

// struct tag used in a typed parameter
struct NodeData {
    int value;
};

// function using user types in params, return type, locals, and casts
Circle* make_circle(Point origin, float r) {
    Circle* c = (Circle*)malloc(sizeof(Circle));
    c->center = origin;
    c->radius = r;
    return c;
}

void process(struct NodeData* data) {
    Point p;
    p.x = data->value;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();
        let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

        // Field type: Point center → TypeUsage for "Point"
        assert!(
            type_names.contains(&"Point"),
            "Should extract 'Point' TypeUsage from field type.\nFound: {:?}",
            type_names
        );

        // Return type: Circle* make_circle → TypeUsage for "Circle"
        assert!(
            type_names.contains(&"Circle"),
            "Should extract 'Circle' TypeUsage from return type / params.\nFound: {:?}",
            type_names
        );

        // Parameter type: Point origin → TypeUsage for "Point"
        // Local variable type: Point p → TypeUsage for "Point"
        let point_count = type_names.iter().filter(|n| **n == "Point").count();
        assert!(
            point_count >= 2,
            "Should extract 'Point' TypeUsage from field, param, and local var (got {})\nFound: {:?}",
            point_count,
            type_names
        );

        // Cast expression: (Circle*) → TypeUsage for "Circle"
        // sizeof(Circle) → TypeUsage for "Circle"
        let circle_count = type_names.iter().filter(|n| **n == "Circle").count();
        assert!(
            circle_count >= 2,
            "Should extract multiple 'Circle' TypeUsage (return, param decl, cast, sizeof) (got {})\nFound: {:?}",
            circle_count,
            type_names
        );

        // Struct declaration tag names should NOT appear as TypeUsage.
        // "Point" and "Circle" are typedef aliases of anonymous structs, so no tag name.
        // "NodeData" IS a struct tag — it appears in `struct NodeData { ... }` as a definition.
        // But `struct NodeData*` in the parameter is a usage — the parent there is
        // `struct_specifier` in a type position, not at declaration level.
        // We skip tag names only when the struct_specifier is the BODY of a declaration
        // (i.e., when its parent is a `type_definition` or at top-level `declaration`
        // and the struct has a body).
        // For now: NodeData in `struct NodeData { ... }` definition should be skipped.
        // NodeData in `struct NodeData*` parameter usage is fine to include.
        let nodedata_usages: Vec<_> = type_usages
            .iter()
            .filter(|id| id.name == "NodeData")
            .collect();
        // At minimum, the parameter usage of `struct NodeData*` should produce a TypeUsage
        assert!(
            !nodedata_usages.is_empty(),
            "Should extract 'NodeData' TypeUsage from parameter `struct NodeData*`.\nFound type_usages: {:?}",
            type_names
        );
    }

    #[test]
    fn test_c_type_usage_skips_declaration_tags() {
        // Struct/enum/union tags at DEFINITION sites should NOT produce TypeUsage
        let c_code = r#"
struct MyStruct {
    int value;
};

enum Color {
    RED,
    GREEN,
    BLUE
};

union Data {
    int i;
    float f;
};

typedef int MyInt;
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();
        let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

        // These are all DEFINITIONS, not usages — none should appear as TypeUsage
        assert!(
            !type_names.contains(&"MyStruct"),
            "Struct definition tag should NOT be TypeUsage.\nFound: {:?}",
            type_names
        );
        assert!(
            !type_names.contains(&"Color"),
            "Enum definition tag should NOT be TypeUsage.\nFound: {:?}",
            type_names
        );
        assert!(
            !type_names.contains(&"Data"),
            "Union definition tag should NOT be TypeUsage.\nFound: {:?}",
            type_names
        );
        // typedef alias name (MyInt) should NOT be TypeUsage
        assert!(
            !type_names.contains(&"MyInt"),
            "Typedef alias name should NOT be TypeUsage.\nFound: {:?}",
            type_names
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let c_code = r#"
void process() {
    // Process implementation
}

void run() {
    process();
    process();  // Same call twice
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(c_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CExtractor::new(
            "c".to_string(),
            "test.c".to_string(),
            c_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH calls (they're at different locations)
        let process_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "process" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            process_calls.len(),
            2,
            "Should extract both process calls at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            process_calls[0].start_line, process_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }
}
