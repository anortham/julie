#[cfg(test)]
mod elixir_tests {
    use crate::base::{SymbolKind, Visibility};
    use crate::elixir::ElixirExtractor;
    use std::path::PathBuf;
    use tree_sitter::{Parser, Tree};

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_elixir::LANGUAGE.into())
            .expect("Error loading Elixir grammar");
        parser
    }

    fn create_extractor_and_parse(code: &str) -> (ElixirExtractor, Tree) {
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let extractor = ElixirExtractor::new(
            "elixir".to_string(),
            "test.ex".to_string(),
            code.to_string(),
            &workspace_root,
        );
        (extractor, tree)
    }

    // ========================================================================
    // AST Debug Test (run with --nocapture to see output)
    // ========================================================================

    #[test]
    fn test_debug_elixir_tree() {
        let code = r#"defmodule MyApp.Calculator do
  @moduledoc "A simple calculator"

  @doc "Adds two numbers"
  def add(a, b), do: a + b

  defp validate(n) when is_number(n), do: n

  def multiply(a, b) do
    validate(a)
    a * b
  end
end

defprotocol Printable do
  def to_string(data)
end

defimpl Printable, for: Integer do
  def to_string(n), do: Integer.to_string(n)
end

defmodule MyApp.User do
  defstruct [:name, :email, :age]

  defmacro validate_field(field) do
    quote do: is_binary(unquote(field))
  end
end

defmodule MyApp.Service do
  use GenServer
  import Enum, only: [map: 2, filter: 2]
  alias MyApp.{User, Calculator}
  require Logger

  @type state :: %{count: integer()}
  @callback init(args :: term()) :: {:ok, state()}
  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(opts), do: GenServer.start_link(__MODULE__, opts)
end"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        fn debug_print_tree(node: tree_sitter::Node, source: &str, depth: usize) {
            let indent = "  ".repeat(depth);
            let text = node.utf8_text(source.as_bytes()).unwrap_or("<err>");
            let short = if text.len() > 80 {
                &text[..77]
            } else {
                text
            };
            let field_name = node
                .parent()
                .and_then(|p| {
                    let mut cursor = p.walk();
                    for (i, child) in p.children(&mut cursor).enumerate() {
                        if child.id() == node.id() {
                            return p.field_name_for_child(i as u32);
                        }
                    }
                    None
                })
                .unwrap_or("");
            let field_prefix = if field_name.is_empty() {
                String::new()
            } else {
                format!("{}:", field_name)
            };
            println!(
                "{}{}({}) {:?}",
                indent,
                field_prefix,
                node.kind(),
                short.replace('\n', "\\n")
            );
            if depth < 5 {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    debug_print_tree(child, source, depth + 1);
                }
            }
        }

        debug_print_tree(tree.root_node(), code, 0);
    }

    // ========================================================================
    // Symbol Extraction Tests
    // ========================================================================

    #[test]
    fn test_elixir_defmodule_extraction() {
        let code = r#"defmodule MyApp.Calculator do
  def add(a, b), do: a + b
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        // Should find: module + function
        let modules: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1, "Expected 1 module, got: {:?}", modules);
        assert_eq!(modules[0].name, "MyApp.Calculator");

        let functions: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(
            functions.len(),
            1,
            "Expected 1 function, got: {:?}",
            functions
        );
        assert_eq!(functions[0].name, "add");
        assert_eq!(functions[0].parent_id, Some(modules[0].id.clone()));
    }

    #[test]
    fn test_elixir_def_defp_visibility() {
        let code = r#"defmodule Foo do
  def public_fn(x), do: x
  defp private_fn(y), do: y
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let public_fn = symbols.iter().find(|s| s.name == "public_fn").unwrap();
        assert_eq!(public_fn.visibility, Some(Visibility::Public));

        let private_fn = symbols.iter().find(|s| s.name == "private_fn").unwrap();
        assert_eq!(private_fn.visibility, Some(Visibility::Private));
    }

    #[test]
    fn test_elixir_defprotocol() {
        let code = r#"defprotocol Printable do
  def to_string(data)
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let protocols: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(protocols.len(), 1);
        assert_eq!(protocols[0].name, "Printable");

        let fns: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "to_string");
        assert_eq!(fns[0].parent_id, Some(protocols[0].id.clone()));
    }

    #[test]
    fn test_elixir_defimpl() {
        let code = r#"defprotocol Printable do
  def to_string(data)
end

defimpl Printable, for: Integer do
  def to_string(n), do: Integer.to_string(n)
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let impls: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(impls.len(), 1, "Expected 1 impl, got: {:?}", impls);
        assert_eq!(impls[0].name, "Printable.Integer");

        // Check metadata
        let meta = impls[0].metadata.as_ref().unwrap();
        assert_eq!(meta.get("protocol_impl").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn test_elixir_defstruct() {
        let code = r#"defmodule MyApp.User do
  defstruct [:name, :email, :age]
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let structs: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1, "Expected 1 struct, got: {:?}", structs);

        let fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field)
            .collect();
        let field_names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"name"), "Expected 'name' field, got: {:?}", field_names);
        assert!(field_names.contains(&"email"), "Expected 'email' field, got: {:?}", field_names);
        assert!(field_names.contains(&"age"), "Expected 'age' field, got: {:?}", field_names);
    }

    #[test]
    fn test_elixir_defmacro() {
        let code = r#"defmodule MyApp.User do
  defmacro validate_field(field) do
    quote do: is_binary(unquote(field))
  end
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let macros: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Function
                    && s.metadata
                        .as_ref()
                        .and_then(|m| m.get("macro"))
                        .and_then(|v| v.as_bool())
                        == Some(true)
            })
            .collect();
        assert_eq!(macros.len(), 1, "Expected 1 macro, got: {:?}", macros);
        assert_eq!(macros[0].name, "validate_field");
    }

    #[test]
    fn test_elixir_import_directives() {
        let code = r#"defmodule MyApp.Service do
  use GenServer
  import Enum, only: [map: 2, filter: 2]
  alias MyApp.User
  require Logger
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let imports: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Import)
            .collect();

        let import_names: Vec<_> = imports.iter().map(|i| i.name.as_str()).collect();
        assert!(
            import_names.contains(&"GenServer"),
            "Expected GenServer import, got: {:?}",
            import_names
        );
        assert!(
            import_names.contains(&"Enum"),
            "Expected Enum import, got: {:?}",
            import_names
        );
        assert!(
            import_names.contains(&"Logger"),
            "Expected Logger import, got: {:?}",
            import_names
        );
    }

    #[test]
    fn test_elixir_type_and_callback_attributes() {
        let code = r#"defmodule MyApp.Service do
  @type state :: %{count: integer()}
  @callback init(args :: term()) :: {:ok, state()}
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let types: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Type)
            .collect();
        assert!(
            types.iter().any(|t| t.name == "state"),
            "Expected 'state' type, got: {:?}",
            types
        );

        let callbacks: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.metadata
                    .as_ref()
                    .and_then(|m| m.get("callback"))
                    .and_then(|v| v.as_bool())
                    == Some(true)
            })
            .collect();
        assert!(
            callbacks.iter().any(|c| c.name == "init"),
            "Expected 'init' callback, got: {:?}",
            callbacks
        );
    }

    #[test]
    fn test_elixir_spec_type_inference() {
        let code = r#"defmodule MyApp.Service do
  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(opts), do: GenServer.start_link(__MODULE__, opts)
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        // Should find a type for start_link
        let start_link_sym = symbols.iter().find(|s| s.name == "start_link").unwrap();
        assert!(
            types.contains_key(&start_link_sym.id),
            "Expected type inference for start_link, types: {:?}",
            types
        );
    }

    // ========================================================================
    // Relationship Tests
    // ========================================================================

    #[test]
    fn test_elixir_impl_relationship() {
        let code = r#"defprotocol Printable do
  def to_string(data)
end

defimpl Printable, for: Integer do
  def to_string(n), do: Integer.to_string(n)
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // defimpl Printable, for: Integer should create an Implements relationship
        let impl_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == crate::base::RelationshipKind::Implements)
            .collect();
        assert!(
            !impl_rels.is_empty(),
            "Expected Implements relationship for defimpl, got: {:?}",
            relationships
        );
    }

    #[test]
    fn test_elixir_call_relationship() {
        let code = r#"defmodule Foo do
  def add(a, b), do: a + b
  def multiply(a, b) do
    add(a, 0)
    a * b
  end
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // multiply should have a Calls relationship to add
        let call_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == crate::base::RelationshipKind::Calls)
            .collect();
        assert!(
            !call_rels.is_empty(),
            "Expected Calls relationship (multiply -> add), got: {:?}",
            relationships
        );
    }

    #[test]
    fn test_elixir_cross_file_use_produces_pending_relationship() {
        // When a module uses another module not defined in the same file,
        // we should get a pending relationship for cross-file resolution.
        let code = r#"defmodule MyApp.Web do
  use Phoenix.Router
  use Plug.Builder
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        // Phoenix.Router and Plug.Builder are not defined in this file,
        // so they should appear as pending relationships
        let pending_names: Vec<_> = pending.iter().map(|p| p.callee_name.as_str()).collect();
        assert!(
            pending_names.contains(&"Phoenix.Router"),
            "Expected pending relationship for 'Phoenix.Router', got: {:?}",
            pending_names
        );
        assert!(
            pending_names.contains(&"Plug.Builder"),
            "Expected pending relationship for 'Plug.Builder', got: {:?}",
            pending_names
        );
        // All should be Uses kind
        assert!(
            pending
                .iter()
                .all(|p| p.kind == crate::base::RelationshipKind::Uses),
            "Expected all pending relationships to be Uses kind"
        );
    }

    #[test]
    fn test_elixir_cross_file_call_produces_pending_relationship() {
        // When a function calls another unqualified function not defined in the same file,
        // we should get a pending relationship for cross-file resolution.
        // Note: qualified dot-calls (e.g. Logger.info) are not currently tracked.
        let code = r#"defmodule MyApp.Worker do
  def run do
    start_server()
    init_state()
  end
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);
        let _relationships = extractor.extract_relationships(&tree, &symbols);
        let pending = extractor.get_pending_relationships();

        // start_server and init_state are not defined in this file
        let pending_names: Vec<_> = pending.iter().map(|p| p.callee_name.as_str()).collect();
        assert!(
            pending_names.contains(&"start_server"),
            "Expected pending Calls relationship for 'start_server', got: {:?}",
            pending_names
        );
        assert!(
            pending
                .iter()
                .all(|p| p.kind == crate::base::RelationshipKind::Calls),
            "Expected all pending relationships to be Calls kind"
        );
    }

    // ========================================================================
    // Identifier Tests
    // ========================================================================

    #[test]
    fn test_elixir_identifier_extraction() {
        let code = r#"defmodule Foo do
  def add(a, b), do: a + b
  def multiply(a, b) do
    add(a, 0)
    a * b
  end
end"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should find function call identifiers
        let call_ids: Vec<_> = identifiers
            .iter()
            .filter(|i| i.kind == crate::base::IdentifierKind::Call)
            .collect();
        // At minimum, `add` should appear as a Call identifier (from multiply calling add)
        let call_names: Vec<_> = call_ids.iter().map(|i| i.name.as_str()).collect();
        assert!(
            call_names.contains(&"add"),
            "Expected 'add' call identifier, got: {:?}",
            call_names
        );
    }

    // ========================================================================
    // Full Fixture Tests
    // ========================================================================

    #[test]
    fn test_elixir_full_fixture() {
        let code = include_str!("../../../../../fixtures/elixir/basic.ex");
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        // Verify we extracted a reasonable number of symbols
        assert!(
            symbols.len() >= 15,
            "Expected at least 15 symbols from full fixture, got {}",
            symbols.len()
        );

        // Check key symbols exist
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"MyApp.Calculator"), "Missing MyApp.Calculator");
        assert!(names.contains(&"add"), "Missing add");
        assert!(names.contains(&"multiply"), "Missing multiply");
        assert!(names.contains(&"Printable"), "Missing Printable");
        assert!(names.contains(&"MyApp.User"), "Missing MyApp.User");
        assert!(names.contains(&"MyApp.Service"), "Missing MyApp.Service");
    }
}
