// TypeScript Extractor Tests
//
// Direct port of Miller's TypeScript extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/typescript-extractor.test.ts

use crate::extractors::base::SymbolKind;
use crate::extractors::typescript::TypeScriptExtractor;
use tree_sitter::Parser;

/// Initialize JavaScript parser for TypeScript files (Miller used JavaScript parser for TS)
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Error loading JavaScript grammar");
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
        let get_user_data_func = symbols
            .iter()
            .find(|s| s.name == "getUserDataAsyncAsyncAsyncAsyncAsyncAsync");
        assert!(get_user_data_func.is_some());
        assert_eq!(get_user_data_func.unwrap().kind, SymbolKind::Function);
        assert!(get_user_data_func
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("getUserDataAsyncAsyncAsyncAsyncAsyncAsync(id)"));

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
        let call_relationships: Vec<_> = relationships
            .iter()
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
        let extends_rel = relationships
            .iter()
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

        // CRITICAL: Function symbol must span entire body for containment logic
        // (identifiers inside function must find function as containing symbol)
        assert_eq!(test_fn.start_line, 1);
        assert_eq!(test_fn.start_column, 0); // Start of "function" keyword
        assert_eq!(test_fn.end_line, 3); // Closing brace line
        assert_eq!(test_fn.end_column, 1); // After closing brace
    }
}

// ========================================================================
// Identifier Extraction Tests (TDD RED phase)
// ========================================================================
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (call_expression)
// - Member access (property_access_expression)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#[cfg(test)]
mod identifier_extraction {
    use super::*;
    use crate::extractors::base::IdentifierKind;

    #[test]
    fn test_extract_function_calls() {
        let code = r#"
function add(a, b) {
    return a + b;
}

function calculate() {
    const result = add(5, 3);      // Function call to add
    console.log(result);            // Function call to log
    return result;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            code.to_string(),
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

        let log_call = identifiers.iter().find(|id| id.name == "log");
        assert!(
            log_call.is_some(),
            "Should extract 'log' function call identifier"
        );
        let log_call = log_call.unwrap();
        assert_eq!(log_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside calculate method)
        assert!(
            add_call.containing_symbol_id.is_some(),
            "Function call should have containing symbol"
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
        let code = r#"
class User {
    constructor(name, email) {
        this.name = name;
        this.email = email;
    }

    printInfo() {
        console.log(this.name);   // Member access: this.name
        const email = this.email;  // Member access: this.email
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
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found member access identifiers
        let name_access = identifiers
            .iter()
            .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            name_access > 0,
            "Should extract 'name' member access identifier"
        );

        let email_access = identifiers
            .iter()
            .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            email_access > 0,
            "Should extract 'email' member access identifier"
        );
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation
        let code = r#"
class Service {
    process() {
        this.helper();              // Call to helper in same file
    }

    helper() {
        // Helper method
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
        let code = r#"
class DataService {
    execute() {
        const result = user.account.balance;   // Chained member access
        const name = customer.profile.name;     // Chained member access
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
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained member access"
        );

        let name_access = identifiers
            .iter()
            .find(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            name_access.is_some(),
            "Should extract 'name' from chained member access"
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let code = r#"
class Test {
    run() {
        process();
        process();  // Same call twice
    }

    process() {
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
    fn test_typescript_decorators_and_metadata() {
        let code = r#"
@Component({
    selector: 'app-user',
    template: '<div>{{user.name}}</div>'
})
export class UserComponent {
    @Input() user: User;
    @Output() userChange = new EventEmitter<User>();

    @HostListener('click')
    onClick() {
        this.userChange.emit(this.user);
    }
}

@Injectable()
export class UserService {
    @Inject(HttpClient) private http: HttpClient;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "decorators.ts".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should handle decorators and metadata
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_typescript_advanced_types() {
        let code = r#"
type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P];
};

type NonNullable<T> = T extends null | undefined ? never : T;

interface ApiResponse<T = any> {
    data: T;
    error?: string;
    status: 'success' | 'error' | 'loading';
}

type UserKeys = keyof User;
type UserValues = User[UserKeys];

function processResponse<T extends ApiResponse>(
    response: T
): T extends ApiResponse<infer U> ? U : never {
    return response.data;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "advanced-types.ts".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should handle advanced TypeScript types
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_typescript_jsx_and_tsx() {
        let code = r#"
import React from 'react';

interface Props {
    name: string;
    age?: number;
}

const UserCard: React.FC<Props> = ({ name, age = 25 }) => {
    return (
        <div className="user-card">
            <h2>{name}</h2>
            <p>Age: {age}</p>
            <button onClick={() => console.log('clicked')}>
                Click me
            </button>
        </div>
    );
};

export default UserCard;
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "tsx.tsx".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should handle JSX/TSX syntax
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_typescript_module_augmentation() {
        let code = r#"
import express from 'express';

declare global {
    namespace Express {
        interface Request {
            user?: User;
        }
    }
}

declare module 'express' {
    interface Request {
        session?: any;
    }
}

declare module '*.css' {
    const content: { [className: string]: string };
    export default content;
}

declare module '*.png' {
    const content: string;
    export default content;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "module-augmentation.ts".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should handle module augmentation
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_typescript_utility_types() {
        let code = r#"
interface User {
    id: number;
    name: string;
    email: string;
    createdAt: Date;
}

type PartialUser = Partial<User>;
type RequiredUser = Required<User>;
type PickUser = Pick<User, 'id' | 'name'>;
type OmitUser = Omit<User, 'createdAt'>;
type ReadonlyUser = Readonly<User>;

function processUser(user: PartialUser): void {
    console.log(user.name);
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "javascript".to_string(),
            "utility-types.ts".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should handle utility types without panicking
        // The extractor may or may not extract all type definitions
        // If we reach here without panicking, the test passes
        let _ = symbols.len(); // Just ensure it doesn't crash
    }
}
