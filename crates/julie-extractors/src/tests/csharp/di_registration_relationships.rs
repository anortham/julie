//! DI Registration Relationship Tests for C#
//!
//! Tests that DI container registration calls (`services.AddScoped<IFoo, Foo>()`)
//! are extracted as `Instantiates` relationships. In C#/.NET, classes registered
//! via DI containers have zero graph centrality because no source code references
//! them directly — the container resolves them at runtime. By extracting
//! `Instantiates` relationships from these registrations, we give DI-registered
//! types the centrality they deserve in search rankings.

use crate::ExtractionResults;
use crate::base::{RelationshipKind, SymbolKind};
use crate::factory::extract_symbols_and_relationships;
use std::path::PathBuf;
use tree_sitter::Parser;

#[cfg(test)]
mod tests {
    use super::*;

    fn init_csharp_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .expect("Error loading C# grammar");
        parser
    }

    fn extract_full(filename: &str, code: &str) -> ExtractionResults {
        let mut parser = init_csharp_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let workspace_root = PathBuf::from("/test/workspace");

        extract_symbols_and_relationships(&tree, filename, code, "csharp", &workspace_root)
            .expect("Failed to extract")
    }

    // ========================================================================
    // STEP 0: AST Diagnostic — verify tree-sitter node structure
    // ========================================================================

    #[test]
    fn test_di_ast_structure() {
        // Verify that tree-sitter parses DI registration as:
        //   invocation_expression
        //     member_access_expression
        //       identifier ("services")
        //       generic_name
        //         identifier ("AddScoped")
        //         type_argument_list
        //           ...type args...
        //     argument_list
        let code = r#"
public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddScoped<IFoo, Foo>();
    }
}
"#;
        let mut parser = init_csharp_parser();
        let tree = parser.parse(code, None).expect("Failed to parse");
        let root = tree.root_node();

        // Walk to find the invocation_expression
        fn find_node<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
            if node.kind() == kind {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_node(child, kind) {
                    return Some(found);
                }
            }
            None
        }

        let invocation = find_node(root, "invocation_expression")
            .expect("Should find invocation_expression");

        // First child should be member_access_expression
        let member_access = invocation.child(0).expect("invocation should have children");
        assert_eq!(
            member_access.kind(),
            "member_access_expression",
            "First child of invocation should be member_access_expression"
        );

        // member_access_expression should contain a generic_name
        let mut cursor = member_access.walk();
        let generic_name = member_access
            .children(&mut cursor)
            .find(|c| c.kind() == "generic_name")
            .expect("member_access_expression should contain generic_name");

        // generic_name should have identifier ("AddScoped") and type_argument_list
        let mut gc = generic_name.walk();
        let children: Vec<_> = generic_name.children(&mut gc).collect();
        let ident = children.iter().find(|c| c.kind() == "identifier");
        let type_args = children.iter().find(|c| c.kind() == "type_argument_list");

        assert!(ident.is_some(), "generic_name should have identifier child");
        assert!(
            type_args.is_some(),
            "generic_name should have type_argument_list child"
        );

        let method_name = ident.unwrap().utf8_text(code.as_bytes()).unwrap();
        assert_eq!(method_name, "AddScoped");
    }

    // ========================================================================
    // TEST 1: Interface-to-implementation registration
    // ========================================================================

    #[test]
    fn test_di_interface_to_implementation() {
        let code = r#"
public interface IFoo { }
public class Foo { }

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddScoped<IFoo, Foo>();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let startup = results
            .symbols
            .iter()
            .find(|s| s.name == "Startup" && s.kind == SymbolKind::Class)
            .expect("Should find Startup class");

        // Should have Instantiates relationships to both IFoo and Foo
        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| {
                r.from_symbol_id == startup.id && r.kind == RelationshipKind::Instantiates
            })
            .collect();

        let ifoo = results
            .symbols
            .iter()
            .find(|s| s.name == "IFoo")
            .expect("Should find IFoo");
        let foo = results
            .symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("Should find Foo");

        assert!(
            instantiates.iter().any(|r| r.to_symbol_id == ifoo.id),
            "Should have Instantiates relationship to IFoo.\nFound: {:?}",
            instantiates.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
        assert!(
            instantiates.iter().any(|r| r.to_symbol_id == foo.id),
            "Should have Instantiates relationship to Foo.\nFound: {:?}",
            instantiates.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST 2: Concrete-only registration (single generic arg)
    // ========================================================================

    #[test]
    fn test_di_concrete_only() {
        let code = r#"
public class MyService { }

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddSingleton<MyService>();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let startup = results
            .symbols
            .iter()
            .find(|s| s.name == "Startup" && s.kind == SymbolKind::Class)
            .expect("Should find Startup class");

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService")
            .expect("Should find MyService");

        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| {
                r.from_symbol_id == startup.id && r.kind == RelationshipKind::Instantiates
            })
            .collect();

        assert!(
            instantiates.iter().any(|r| r.to_symbol_id == my_service.id),
            "Should have Instantiates relationship to MyService.\nFound: {:?}",
            instantiates.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST 3: Hosted service registration
    // ========================================================================

    #[test]
    fn test_di_hosted_service() {
        let code = r#"
public class BackgroundWorker { }

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddHostedService<BackgroundWorker>();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let startup = results
            .symbols
            .iter()
            .find(|s| s.name == "Startup" && s.kind == SymbolKind::Class)
            .expect("Should find Startup class");

        let worker = results
            .symbols
            .iter()
            .find(|s| s.name == "BackgroundWorker")
            .expect("Should find BackgroundWorker");

        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| {
                r.from_symbol_id == startup.id && r.kind == RelationshipKind::Instantiates
            })
            .collect();

        assert!(
            instantiates.iter().any(|r| r.to_symbol_id == worker.id),
            "Should have Instantiates relationship to BackgroundWorker.\nFound: {:?}",
            instantiates.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST 4: Chained member access (builder.Services.AddScoped<T>())
    // ========================================================================

    #[test]
    fn test_di_chained_member_access() {
        let code = r#"
public class MyService { }

public class Program {
    public static void Main() {
        var builder = WebApplication.CreateBuilder();
        builder.Services.AddScoped<MyService>();
    }
}
"#;
        let results = extract_full("src/Program.cs", code);

        let program = results
            .symbols
            .iter()
            .find(|s| s.name == "Program" && s.kind == SymbolKind::Class)
            .expect("Should find Program class");

        let my_service = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService")
            .expect("Should find MyService");

        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| {
                r.from_symbol_id == program.id && r.kind == RelationshipKind::Instantiates
            })
            .collect();

        assert!(
            instantiates.iter().any(|r| r.to_symbol_id == my_service.id),
            "Chained access (builder.Services.AddScoped) should produce Instantiates.\nFound: {:?}",
            instantiates.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST 5: Cross-file types create PendingRelationship
    // ========================================================================

    #[test]
    fn test_di_cross_file_creates_pending() {
        // Types not defined in this file should become PendingRelationships
        let code = r#"
using MyApp.Services;

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddScoped<IOrderService, OrderService>();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let startup = results
            .symbols
            .iter()
            .find(|s| s.name == "Startup" && s.kind == SymbolKind::Class)
            .expect("Should find Startup class");

        let pending_instantiates: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| {
                p.from_symbol_id == startup.id && p.kind == RelationshipKind::Instantiates
            })
            .collect();

        assert!(
            pending_instantiates
                .iter()
                .any(|p| p.callee_name == "IOrderService"),
            "Should have pending Instantiates for IOrderService.\nFound: {:?}",
            pending_instantiates
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
        assert!(
            pending_instantiates
                .iter()
                .any(|p| p.callee_name == "OrderService"),
            "Should have pending Instantiates for OrderService.\nFound: {:?}",
            pending_instantiates
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST 6: All lifetime methods recognized
    // ========================================================================

    #[test]
    fn test_di_all_lifetime_methods() {
        let code = r#"
public class A { }
public class B { }
public class C { }

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddSingleton<A>();
        services.AddScoped<B>();
        services.AddTransient<C>();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let startup = results
            .symbols
            .iter()
            .find(|s| s.name == "Startup" && s.kind == SymbolKind::Class)
            .expect("Should find Startup class");

        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| {
                r.from_symbol_id == startup.id && r.kind == RelationshipKind::Instantiates
            })
            .collect();

        for type_name in &["A", "B", "C"] {
            let sym = results
                .symbols
                .iter()
                .find(|s| s.name == *type_name)
                .unwrap_or_else(|| panic!("Should find {}", type_name));
            assert!(
                instantiates.iter().any(|r| r.to_symbol_id == sym.id),
                "{} should have Instantiates relationship (registered via DI).\nFound: {:?}",
                type_name,
                instantiates
                    .iter()
                    .map(|r| &r.to_symbol_id)
                    .collect::<Vec<_>>()
            );
        }
    }

    // ========================================================================
    // TEST 7: Non-registration generics are NOT extracted
    // ========================================================================

    #[test]
    fn test_di_non_registration_not_extracted() {
        let code = r#"
public class Foo { }

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.Configure<AppSettings>();
        var list = new List<Foo>();
        services.AddCors();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let startup = results
            .symbols
            .iter()
            .find(|s| s.name == "Startup" && s.kind == SymbolKind::Class)
            .expect("Should find Startup class");

        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| {
                r.from_symbol_id == startup.id && r.kind == RelationshipKind::Instantiates
            })
            .collect();

        assert!(
            instantiates.is_empty(),
            "Configure<T>(), new List<T>(), AddCors() should NOT produce Instantiates.\nFound: {:?}",
            instantiates
                .iter()
                .map(|r| &r.to_symbol_id)
                .collect::<Vec<_>>()
        );

        // Also check pending — Configure<AppSettings> should NOT be pending Instantiates
        let pending_instantiates: Vec<_> = results
            .pending_relationships
            .iter()
            .filter(|p| {
                p.from_symbol_id == startup.id && p.kind == RelationshipKind::Instantiates
            })
            .collect();

        assert!(
            pending_instantiates.is_empty(),
            "Non-registration calls should NOT produce pending Instantiates.\nFound: {:?}",
            pending_instantiates
                .iter()
                .map(|p| &p.callee_name)
                .collect::<Vec<_>>()
        );
    }

    // ========================================================================
    // TEST 8: Source of relationship is the containing class, not method
    // ========================================================================

    #[test]
    fn test_di_source_is_containing_class() {
        let code = r#"
public class MyService { }

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddScoped<MyService>();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let startup = results
            .symbols
            .iter()
            .find(|s| s.name == "Startup" && s.kind == SymbolKind::Class)
            .expect("Should find Startup class");

        // The from_symbol_id should be Startup (the class), not ConfigureServices (the method)
        let configure_method = results
            .symbols
            .iter()
            .find(|s| s.name == "ConfigureServices");

        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Instantiates)
            .collect();

        assert!(
            !instantiates.is_empty(),
            "Should have at least one Instantiates relationship"
        );

        for rel in &instantiates {
            assert_eq!(
                rel.from_symbol_id, startup.id,
                "Instantiates source should be the containing class (Startup), not the method"
            );
            if let Some(method) = configure_method {
                assert_ne!(
                    rel.from_symbol_id, method.id,
                    "Instantiates source should NOT be ConfigureServices method"
                );
            }
        }
    }

    // ========================================================================
    // TEST 9: Target is the class, not the same-named constructor
    // (Dogfood bug: in real C# codebases, class and constructor share the
    //  same name. symbols.iter().find() could hit the constructor first.)
    // ========================================================================

    #[test]
    fn test_di_target_is_class_not_constructor() {
        let code = r#"
public class MyService {
    public MyService(ILogger logger) {
    }
}

public class Startup {
    public void ConfigureServices(IServiceCollection services) {
        services.AddScoped<MyService>();
    }
}
"#;
        let results = extract_full("src/Startup.cs", code);

        let my_service_class = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class)
            .expect("Should find MyService class");

        let my_service_ctor = results
            .symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Constructor);

        let instantiates: Vec<_> = results
            .relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Instantiates)
            .collect();

        assert!(
            !instantiates.is_empty(),
            "Should have Instantiates relationship"
        );

        // Relationship should target the CLASS, not the constructor
        assert!(
            instantiates
                .iter()
                .any(|r| r.to_symbol_id == my_service_class.id),
            "Instantiates should target the class symbol, not the constructor"
        );

        if let Some(ctor) = my_service_ctor {
            assert!(
                !instantiates.iter().any(|r| r.to_symbol_id == ctor.id),
                "Instantiates should NOT target the constructor symbol"
            );
        }
    }
}
