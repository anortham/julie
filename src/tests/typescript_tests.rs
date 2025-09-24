// TypeScript Extractor Tests
//
// Direct port of Miller's TypeScript extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/typescript-extractor.test.ts

use crate::extractors::base::{Symbol, SymbolKind};
use crate::extractors::typescript::TypeScriptExtractor;
use tree_sitter::Parser;

/// Initialize JavaScript parser for TypeScript files (Miller used JavaScript parser for TS)
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).expect("Error loading JavaScript grammar");
    parser
}

#[cfg(test)]
mod typescript_extractor_tests {
    use super::*;

    #[test]
    fn test_extract_function_declarations() {
        let code = r#"
        function getUserDataAsyncAsyncAsyncAsyncAsyncAsync(id) {
          return fetch(`/api/users/${id}`).then(r => r.json());
        }

        const arrow = (x) => x * 2;

        async function asyncFunc() {
          await Promise.resolve();
        }
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        assert!(symbols.len() >= 3);

        // Check function declaration
        let get_user_data_func = symbols.iter()
            .find(|s| s.name == "getUserDataAsyncAsyncAsyncAsyncAsyncAsync");
        assert!(get_user_data_func.is_some());
        assert_eq!(get_user_data_func.unwrap().kind, SymbolKind::Function);
        assert!(get_user_data_func.unwrap().signature.as_ref().unwrap().contains("getUserDataAsyncAsyncAsyncAsyncAsyncAsync(id)"));

        // Check arrow function
        let arrow = symbols.iter().find(|s| s.name == "arrow");
        assert!(arrow.is_some());
        assert_eq!(arrow.unwrap().kind, SymbolKind::Function);

        // Check async function
        let async_func = symbols.iter().find(|s| s.name == "asyncFunc");
        assert!(async_func.is_some());
        assert_eq!(async_func.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class_declarations() {
        let code = r#"
        class BaseEntity {
          constructor(id) {
            this.id = id;
          }

          save() {
            throw new Error('Must implement save method');
          }
        }

        class User extends BaseEntity {
          constructor(id, name, email) {
            super(id);
            this.name = name;
            this.email = email;
          }

          serialize() {
            return JSON.stringify({ id: this.id, name: this.name, email: this.email });
          }

          async save() {
            await fetch('/api/users', { method: 'POST', body: this.serialize() });
          }
        }
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Check base class
        let base_entity = symbols.iter().find(|s| s.name == "BaseEntity");
        assert!(base_entity.is_some());
        assert_eq!(base_entity.unwrap().kind, SymbolKind::Class);

        // Check derived class
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert_eq!(user.unwrap().kind, SymbolKind::Class);

        // Check class methods
        let serialize = symbols.iter().find(|s| s.name == "serialize");
        assert!(serialize.is_some());
        assert_eq!(serialize.unwrap().kind, SymbolKind::Method);
        assert_eq!(serialize.unwrap().parent_id, Some(user.unwrap().id.clone()));

        // Check constructor
        let constructor = symbols.iter().find(|s| s.name == "constructor");
        assert!(constructor.is_some());
        assert_eq!(constructor.unwrap().kind, SymbolKind::Constructor);
    }

    #[test]
    fn test_extract_variable_and_property_declarations() {
        let code = r#"
        const API_URL = 'https://api.example.com';
        let counter = 0;
        var legacy = true;

        const config = {
          timeout: 5000,
          retries: 3
        };
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Check constants
        let api_url = symbols.iter().find(|s| s.name == "API_URL");
        assert!(api_url.is_some());
        assert_eq!(api_url.unwrap().kind, SymbolKind::Variable);

        // Check variables
        let counter = symbols.iter().find(|s| s.name == "counter");
        assert!(counter.is_some());
        assert_eq!(counter.unwrap().kind, SymbolKind::Variable);

        // Check object literal
        let config = symbols.iter().find(|s| s.name == "config");
        assert!(config.is_some());
        assert_eq!(config.unwrap().kind, SymbolKind::Variable);
    }

    #[test]
    fn test_extract_function_call_relationships() {
        let code = r#"
        function helper(x: number): number {
          return x * 2;
        }

        function main(): void {
          const result = helper(42);
          console.log(result);
          Math.max(1, 2, 3);
        }
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should find call relationships
        let call_relationships: Vec<_> = relationships.iter()
            .filter(|r| r.kind.to_string() == "calls")
            .collect();
        assert!(call_relationships.len() > 0);

        // Check helper function call
        let helper_call = call_relationships.iter().find(|r| {
            let from_symbol = symbols.iter().find(|s| s.id == r.from_symbol_id);
            let to_symbol = symbols.iter().find(|s| s.id == r.to_symbol_id);
            matches!((from_symbol, to_symbol), (Some(from), Some(to)) if from.name == "main" && to.name == "helper")
        });
        assert!(helper_call.is_some());
    }

    #[test]
    fn test_extract_inheritance_relationships() {
        let code = r#"
        class Shape {
          constructor() {
            if (this.constructor === Shape) {
              throw new Error('Cannot instantiate abstract class');
            }
          }

          area() {
            throw new Error('Must implement area method');
          }

          draw() {
            console.log('Drawing shape');
          }
        }

        class Circle extends Shape {
          constructor(radius) {
            super();
            this.radius = radius;
          }

          area() {
            return Math.PI * this.radius ** 2;
          }
        }
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Check extends relationship
        let extends_rel = relationships.iter()
            .find(|r| r.kind.to_string() == "extends");
        assert!(extends_rel.is_some());
    }

    #[test]
    fn test_infer_basic_types() {
        let code = r#"
        const name = 'John';
        const age = 30;
        const isActive = true;
        const scores = [95, 87, 92];
        const user = { name: 'John', age: 30 };
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        assert!(types.len() > 0);

        // Check some basic type inferences
        let name_symbol = symbols.iter().find(|s| s.name == "name");
        let age_symbol = symbols.iter().find(|s| s.name == "age");
        let is_active_symbol = symbols.iter().find(|s| s.name == "isActive");

        if let Some(name_sym) = name_symbol {
            if let Some(name_type) = types.get(&name_sym.id) {
                assert!(name_type.contains("string"));
            }
        }
        if let Some(age_sym) = age_symbol {
            if let Some(age_type) = types.get(&age_sym.id) {
                assert!(age_type.contains("number"));
            }
        }
        if let Some(is_active_sym) = is_active_symbol {
            if let Some(is_active_type) = types.get(&is_active_sym.id) {
                assert!(is_active_type.contains("boolean"));
            }
        }
    }

    #[test]
    fn test_handle_function_return_types() {
        let code = r#"
        function add(a, b) {
          return a + b;
        }

        async function fetchUser(id) {
          const response = await fetch(`/users/${id}`);
          return response.json();
        }
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let add_symbol = symbols.iter().find(|s| s.name == "add");
        let fetch_user_symbol = symbols.iter().find(|s| s.name == "fetchUser");

        if let Some(add_sym) = add_symbol {
            let add_type = types.get(&add_sym.id);
            assert!(add_type.is_some()); // Type inference may not be perfect for JS
        }
        if let Some(fetch_user_sym) = fetch_user_symbol {
            let fetch_user_type = types.get(&fetch_user_sym.id);
            assert!(fetch_user_type.is_some()); // Type inference may not be perfect for JS
        }
    }

    #[test]
    fn test_track_accurate_symbol_positions() {
        let code = "function test() {\n  return 42;\n}";

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let test_function = symbols.iter().find(|s| s.name == "test");
        assert!(test_function.is_some());
        let test_fn = test_function.unwrap();
        assert_eq!(test_fn.start_line, 1);
        assert_eq!(test_fn.start_column, 9);
        assert_eq!(test_fn.end_line, 1); // Function name spans only one line
    }
}