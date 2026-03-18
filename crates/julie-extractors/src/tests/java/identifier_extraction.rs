// Java Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function/method calls (method_invocation)
// - Member/field access (field_access)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#![allow(unused_imports)]

use crate::base::{IdentifierKind, SymbolKind};
use crate::java::JavaExtractor;
use crate::tests::helpers::init_parser;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_function_calls() {
        let workspace_root = PathBuf::from("/tmp/test");
        let java_code = r#"
public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public int calculate() {
        int result = add(5, 3);           // Method call to add
        System.out.println(result);       // Method call to println
        return result;
    }
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
            &workspace_root,
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the method calls
        let add_call = identifiers.iter().find(|id| id.name == "add");
        assert!(
            add_call.is_some(),
            "Should extract 'add' method call identifier"
        );
        let add_call = add_call.unwrap();
        assert_eq!(add_call.kind, IdentifierKind::Call);

        let println_call = identifiers.iter().find(|id| id.name == "println");
        assert!(
            println_call.is_some(),
            "Should extract 'println' method call identifier"
        );
        let println_call = println_call.unwrap();
        assert_eq!(println_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside calculate method)
        assert!(
            add_call.containing_symbol_id.is_some(),
            "Method call should have containing symbol"
        );

        // Find the calculate method symbol
        let calculate_method = symbols.iter().find(|s| s.name == "calculate").unwrap();

        // Verify the add call is contained within calculate method
        assert_eq!(
            add_call.containing_symbol_id.as_ref(),
            Some(&calculate_method.id),
            "add call should be contained within calculate method"
        );
    }

    #[test]
    fn test_extract_member_access() {
        let workspace_root = PathBuf::from("/tmp/test");
        let java_code = r#"
public class User {
    public String name;
    public String email;

    public void printInfo() {
        System.out.println(this.name);    // Field access: this.name
        String userEmail = this.email;    // Field access: this.email
    }
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found field access identifiers
        let name_access = identifiers
            .iter()
            .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            name_access > 0,
            "Should extract 'name' field access identifier"
        );

        let email_access = identifiers
            .iter()
            .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            email_access > 0,
            "Should extract 'email' field access identifier"
        );
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        let workspace_root = PathBuf::from("/tmp/test");
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation (line 1311-1318 in rust.rs)
        let java_code = r#"
public class Service {
    public void process() {
        helper();              // Call to helper in same file
    }

    private void helper() {
        // Helper method
    }
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the helper call
        let helper_call = identifiers.iter().find(|id| id.name == "helper");
        assert!(helper_call.is_some());
        let helper_call = helper_call.unwrap();

        // Verify it has a containing symbol (the process method)
        assert!(
            helper_call.containing_symbol_id.is_some(),
            "helper call should have containing symbol from same file"
        );

        // Verify the containing symbol is the process method
        let process_method = symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(
            helper_call.containing_symbol_id.as_ref(),
            Some(&process_method.id),
            "helper call should be contained within process method"
        );
    }

    #[test]
    fn test_chained_member_access() {
        let workspace_root = PathBuf::from("/tmp/test");
        let java_code = r#"
public class DataService {
    public void execute() {
        double result = user.account.balance;      // Chained field access
        String name = customer.profile.fullName;   // Chained field access
    }
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained field access"
        );

        let fullname_access = identifiers
            .iter()
            .find(|id| id.name == "fullName" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            fullname_access.is_some(),
            "Should extract 'fullName' from chained field access"
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let workspace_root = PathBuf::from("/tmp/test");
        let java_code = r#"
public class Test {
    public void run() {
        process();
        process();  // Same call twice
    }

    private void process() {
    }
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
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

    #[test]
    fn test_java_type_usage_identifiers() {
        // Java type annotations should produce TypeUsage identifiers.
        // These drive centrality scoring for classes like Gson, TypeAdapter, etc.
        let workspace_root = PathBuf::from("/tmp/test");
        let java_code = r#"
public class GsonService {
    private Gson gson;
    private TypeAdapter<JsonElement> adapter;

    public GsonService(Gson gson, TypeAdapter<JsonElement> adapter) {
        this.gson = gson;
        this.adapter = adapter;
    }

    public JsonElement parse(String input) {
        return gson.fromJson(input, JsonElement.class);
    }

    public List<UserProfile> getProfiles() {
        return null;
    }
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();

        let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

        // Core type references that MUST be extracted
        assert!(
            type_names.contains(&"Gson"),
            "Field/param type 'Gson' must be extracted. Got: {:?}",
            type_names
        );
        assert!(
            type_names.contains(&"TypeAdapter"),
            "Generic type 'TypeAdapter' must be extracted. Got: {:?}",
            type_names
        );
        assert!(
            type_names.contains(&"JsonElement"),
            "Generic arg 'JsonElement' must be extracted. Got: {:?}",
            type_names
        );
        assert!(
            type_names.contains(&"UserProfile"),
            "Generic arg 'UserProfile' must be extracted. Got: {:?}",
            type_names
        );
        // String and List are common standard library types but should NOT be filtered
        assert!(
            type_names.contains(&"String"),
            "Standard library type 'String' must be extracted (not filtered). Got: {:?}",
            type_names
        );
        assert!(
            type_names.contains(&"List"),
            "Standard library type 'List' must be extracted (not filtered). Got: {:?}",
            type_names
        );

        // Class declaration name should NOT be extracted as TypeUsage
        assert!(
            !type_names.contains(&"GsonService"),
            "Class declaration name 'GsonService' must NOT be extracted as TypeUsage. Got: {:?}",
            type_names
        );
    }

    #[test]
    fn test_java_type_usage_skips_single_letter_generics() {
        // Single-letter generics (T, K, V, E) carry no cross-file signal
        let workspace_root = PathBuf::from("/tmp/test");
        let java_code = r#"
public class Container<T> {
    private T value;

    public <K, V> Map<K, V> toMap() {
        return null;
    }

    public <E extends Exception> void handle(E error) {
    }
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();

        let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

        // Single-letter generics should be filtered
        assert!(
            !type_names.contains(&"T"),
            "Single-letter generic 'T' must be filtered. Got: {:?}",
            type_names
        );
        assert!(
            !type_names.contains(&"K"),
            "Single-letter generic 'K' must be filtered. Got: {:?}",
            type_names
        );
        assert!(
            !type_names.contains(&"V"),
            "Single-letter generic 'V' must be filtered. Got: {:?}",
            type_names
        );
        assert!(
            !type_names.contains(&"E"),
            "Single-letter generic 'E' must be filtered. Got: {:?}",
            type_names
        );

        // But Map and Exception should be extracted
        assert!(
            type_names.contains(&"Map"),
            "Type 'Map' must be extracted. Got: {:?}",
            type_names
        );
        assert!(
            type_names.contains(&"Exception"),
            "Type 'Exception' must be extracted. Got: {:?}",
            type_names
        );

        // Container (declaration name) should NOT be extracted
        assert!(
            !type_names.contains(&"Container"),
            "Class declaration name 'Container' must NOT be extracted. Got: {:?}",
            type_names
        );
    }

    #[test]
    fn test_java_type_usage_excludes_declaration_names() {
        // Declaration-position type_identifier nodes must NOT be extracted as TypeUsage
        let workspace_root = PathBuf::from("/tmp/test");
        let java_code = r#"
public class MyClass {
    private SomeType field;
}

public interface MyInterface {
    OtherType process();
}

public enum MyEnum {
    VALUE_ONE
}
"#;

        let tree = init_parser(java_code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            java_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let type_usages: Vec<_> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::TypeUsage)
            .collect();

        let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

        // Declaration names must NOT appear
        assert!(
            !type_names.contains(&"MyClass"),
            "Class declaration 'MyClass' must not be TypeUsage. Got: {:?}",
            type_names
        );
        assert!(
            !type_names.contains(&"MyInterface"),
            "Interface declaration 'MyInterface' must not be TypeUsage. Got: {:?}",
            type_names
        );
        assert!(
            !type_names.contains(&"MyEnum"),
            "Enum declaration 'MyEnum' must not be TypeUsage. Got: {:?}",
            type_names
        );

        // Reference-position types MUST appear
        assert!(
            type_names.contains(&"SomeType"),
            "Field type 'SomeType' must be extracted. Got: {:?}",
            type_names
        );
        assert!(
            type_names.contains(&"OtherType"),
            "Return type 'OtherType' must be extracted. Got: {:?}",
            type_names
        );
    }
}
