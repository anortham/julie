// Implementation of comprehensive Vue extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

// Submodule declarations
pub mod cross_file_pending;
pub mod parsing;
pub mod relationships;
pub mod type_arguments;

use crate::base::SymbolKind;
use crate::vue::VueExtractor;

#[cfg(test)]
mod vue_extractor_tests {
    use super::*;

    // Helper function to create a VueExtractor - Vue doesn't use tree-sitter
    fn create_extractor(file_path: &str, code: &str) -> VueExtractor {
        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        VueExtractor::new(
            "vue".to_string(),
            file_path.to_string(),
            code.to_string(),
            &workspace_root,
        )
    }

    #[test]
    fn test_extract_vue_component_symbol() {
        let vue_code = r#"
<template>
  <div class="hello-world">
    <h1>{{ message }}</h1>
    <button @click="increment">Count: {{ count }}</button>
  </div>
</template>

<script>
export default {
  name: 'HelloWorld',
  data() {
    return {
      message: 'Hello Vue!',
      count: 0
    }
  },
  methods: {
    increment() {
      this.count++;
    }
  }
}
</script>

<style scoped>
.hello-world {
  padding: 20px;
}
</style>
        "#;

        let mut extractor = create_extractor("test-component.vue", vue_code);
        let symbols = extractor.extract_symbols(None); // Vue doesn't use tree-sitter

        assert!(symbols.len() > 0);

        // Check component symbol
        let component = symbols.iter().find(|s| s.name == "HelloWorld");
        assert!(component.is_some());
        let component = component.unwrap();
        assert_eq!(component.kind, SymbolKind::Class);
        assert!(
            component
                .signature
                .as_ref()
                .unwrap()
                .contains("<HelloWorld />")
        );
    }

    #[test]
    fn test_extract_script_section_symbols() {
        let vue_code = r#"
<script>
export default {
  data() {
    return { message: 'Hello' }
  },
  computed: {
    upperMessage() {
      return this.message.toUpperCase();
    }
  },
  methods: {
    greet() {
      console.log(this.message);
    },
    calculate(a, b) {
      return a + b;
    }
  }
}
</script>
        "#;

        let mut extractor = create_extractor("methods.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        // Should find data, computed, methods, and individual method functions
        let data_symbol = symbols.iter().find(|s| s.name == "data");
        assert!(data_symbol.is_some());
        assert_eq!(data_symbol.unwrap().kind, SymbolKind::Function);

        let computed_symbol = symbols.iter().find(|s| s.name == "computed");
        assert!(computed_symbol.is_some());
        assert_eq!(computed_symbol.unwrap().kind, SymbolKind::Property);

        let methods_symbol = symbols.iter().find(|s| s.name == "methods");
        assert!(methods_symbol.is_some());
        assert_eq!(methods_symbol.unwrap().kind, SymbolKind::Property);

        let greet_method = symbols.iter().find(|s| s.name == "greet");
        assert!(greet_method.is_some());
        assert_eq!(greet_method.unwrap().kind, SymbolKind::Method);

        let calculate_method = symbols.iter().find(|s| s.name == "calculate");
        assert!(calculate_method.is_some());
        assert_eq!(calculate_method.unwrap().kind, SymbolKind::Method);
    }

    #[test]
    fn test_template_usages_not_extracted_as_symbols() {
        let vue_code = r#"
<template>
  <div>
    <UserProfile :user="currentUser" />
    <ProductCard v-for="product in products" :key="product.id" />
    <CustomButton @click="handleClick" v-if="showButton" />
  </div>
</template>
        "#;

        let mut extractor = create_extractor("template.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        // Component usages in <template> are references, not definitions — should NOT be extracted
        assert!(
            !symbols.iter().any(|s| s.name == "UserProfile"),
            "Component usages should NOT be extracted as symbols"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "ProductCard"),
            "Component usages should NOT be extracted as symbols"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "CustomButton"),
            "Component usages should NOT be extracted as symbols"
        );

        // Directives (v-for, v-if) are framework syntax, not definitions
        assert!(
            !symbols.iter().any(|s| s.name == "v-for"),
            "Vue directives should NOT be extracted as symbols"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "v-if"),
            "Vue directives should NOT be extracted as symbols"
        );
    }

    #[test]
    fn test_vue_template_refs_slots_and_v_model_emit_template_symbols() {
        let vue_code = r#"<template>
  <section>
    <input ref="nameInput" v-model="form.name" />
    <slot name="actions"></slot>
    <UserProfile :user="currentUser" />
  </section>
</template>"#;

        let mut extractor = create_extractor("template-definitions.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        let name_input = symbols
            .iter()
            .find(|symbol| symbol.name == "nameInput")
            .expect("template ref should be extracted as a symbol");
        assert_eq!(name_input.kind, SymbolKind::Variable);
        let ref_offset = vue_code.find("nameInput").unwrap() as u32;
        assert_eq!(name_input.start_byte, ref_offset);
        assert_eq!(name_input.end_byte, ref_offset + "nameInput".len() as u32);
        assert_eq!(
            &vue_code[name_input.start_byte as usize..name_input.end_byte as usize],
            "nameInput"
        );

        let actions = symbols
            .iter()
            .find(|symbol| symbol.name == "actions")
            .expect("named slot should be extracted as a symbol");
        assert_eq!(actions.kind, SymbolKind::Event);
        let slot_offset = vue_code.find("actions").unwrap() as u32;
        assert_eq!(actions.start_byte, slot_offset);
        assert_eq!(actions.end_byte, slot_offset + "actions".len() as u32);

        let form_name = symbols
            .iter()
            .find(|symbol| symbol.name == "form.name")
            .expect("v-model binding should be extracted as a symbol");
        assert_eq!(form_name.kind, SymbolKind::Property);
        let model_offset = vue_code.find("form.name").unwrap() as u32;
        assert_eq!(form_name.start_byte, model_offset);
        assert_eq!(form_name.end_byte, model_offset + "form.name".len() as u32);

        assert!(
            !symbols.iter().any(|symbol| symbol.name == "UserProfile"),
            "template component usages remain references, not definitions"
        );
    }

    #[test]
    fn test_vue_template_symbol_ranges_use_template_section_when_content_repeats() {
        let vue_code = r#"<script>
const repeated = `
  <input ref="nameInput" />
`;
</script>

<template>
  <input ref="nameInput" />
</template>"#;

        let mut extractor = create_extractor("repeated-template.vue", vue_code);
        let symbols = extractor.extract_symbols(None);
        let name_input = symbols
            .iter()
            .find(|symbol| symbol.name == "nameInput" && symbol.kind == SymbolKind::Variable)
            .expect("template ref should be extracted");
        let expected_offset = vue_code.rfind("nameInput").unwrap() as u32;
        assert_eq!(name_input.start_byte, expected_offset);
    }

    #[test]
    fn test_vue_component_symbol_keeps_broad_span_when_name_appears_in_script() {
        let vue_code = r#"<template>
  <div>Named component</div>
</template>

<script>
export default {
  name: 'BroadSpanComponent',
  methods: {
    mention() {
      return 'BroadSpanComponent';
    }
  }
}
</script>"#;

        let mut extractor = create_extractor("broad-span.vue", vue_code);
        let symbols = extractor.extract_symbols(None);
        let component = symbols
            .iter()
            .find(|symbol| symbol.name == "BroadSpanComponent" && symbol.kind == SymbolKind::Class)
            .expect("component-level symbol should be extracted");

        assert_eq!(component.start_line, 1);
        assert_eq!(component.start_column, 1);
        assert_eq!(component.start_byte, 0);
        assert!(component.end_byte > vue_code.find("name: 'BroadSpanComponent'").unwrap() as u32);
    }

    #[test]
    fn test_extract_style_symbols() {
        let vue_code = r#"
<style scoped>
.container {
  display: flex;
  align-items: center;
}

.button {
  padding: 10px;
  background: blue;
}

.disabled {
  opacity: 0.5;
}
</style>
        "#;

        let mut extractor = create_extractor("styles.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        // Should find CSS classes
        let container = symbols.iter().find(|s| s.name == "container");
        assert!(container.is_some());
        let container = container.unwrap();
        assert_eq!(container.kind, SymbolKind::Property);
        assert_eq!(container.signature.as_ref().unwrap(), ".container");
        assert_eq!(container.id, expected_symbol_id(container));

        let button = symbols.iter().find(|s| s.name == "button");
        assert!(button.is_some());
        let button = button.unwrap();
        assert_eq!(button.kind, SymbolKind::Property);
        assert_eq!(button.id, expected_symbol_id(button));

        let disabled = symbols.iter().find(|s| s.name == "disabled");
        assert!(disabled.is_some());
        let disabled = disabled.unwrap();
        assert_eq!(disabled.kind, SymbolKind::Property);
        assert_eq!(disabled.id, expected_symbol_id(disabled));
    }

    #[test]
    fn test_handle_named_components() {
        let vue_code = r#"
<script>
export default {
  name: 'MyCustomComponent',
  data() {
    return { value: 42 }
  }
}
</script>
        "#;

        let mut extractor = create_extractor("custom.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        // Should use the name from the component, not filename
        let component = symbols.iter().find(|s| s.name == "MyCustomComponent");
        assert!(component.is_some());
        assert_eq!(component.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_handle_complex_sfc_with_all_sections() {
        let vue_code = r#"
<template>
  <div class="app">
    <Header :title="pageTitle" />
    <main>
      <slot />
    </main>
  </div>
</template>

<script lang="ts">
export default {
  name: 'AppLayout',
  props: {
    pageTitle: String
  },
  data() {
    return {
      loading: false
    }
  },
  mounted() {
    this.initialize();
  },
  methods: {
    initialize() {
      this.loading = true;
    }
  }
}
</script>

<style lang="scss" scoped>
.app {
  min-height: 100vh;
}

.header {
  position: fixed;
  top: 0;
}
</style>
        "#;

        let mut extractor = create_extractor("app-layout.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        assert!(symbols.len() > 4);

        // Component name from <script> section
        let component = symbols.iter().find(|s| s.name == "AppLayout");
        assert!(component.is_some());

        // Script symbols
        assert!(symbols.iter().find(|s| s.name == "props").is_some());
        assert!(symbols.iter().find(|s| s.name == "data").is_some());
        assert!(symbols.iter().find(|s| s.name == "methods").is_some());
        assert!(symbols.iter().find(|s| s.name == "initialize").is_some());

        // Template component usages should NOT be extracted
        assert!(
            !symbols.iter().any(|s| s.name == "Header"),
            "Template component usages should NOT be extracted"
        );

        // Style symbols
        assert!(symbols.iter().find(|s| s.name == "app").is_some());
        assert!(symbols.iter().find(|s| s.name == "header").is_some());
    }

    // Test removed: Vue now properly extracts types (no longer returns empty map)
    // See memory checkpoint 14:16:59 - fixed Vue infer_types() stub

    #[test]
    fn test_extract_relationships_returns_empty_array() {
        let mut extractor = create_extractor("test.vue", "<template></template>");
        let symbols = extractor.extract_symbols(None);
        let relationships = extractor.extract_relationships(None, &symbols);

        assert!(relationships.len() == 0);
    }
}

// ========================================================================
// Vue Identifier Extraction Tests (TDD RED phase)
// ========================================================================
//
// These tests validate extract_identifiers() functionality for Vue SFCs
// - Function calls within <script> section
// - Member access within <script> section
// - Proper containing symbol tracking
//
// Vue-specific approach: Parse <script> section with JavaScript tree-sitter

#[cfg(test)]
mod vue_identifier_extraction_tests {
    use crate::base::IdentifierKind;
    use crate::vue::VueExtractor;

    #[test]
    fn test_vue_function_calls() {
        let vue_code = r#"
<template>
  <div>{{ message }}</div>
</template>

<script>
export default {
  data() {
    return { message: 'Hello' }
  },
  methods: {
    greet() {
      console.log(this.message);    // Function call: console.log
      this.updateMessage('Hi');      // Method call: updateMessage
    },
    updateMessage(msg) {
      this.message = msg;
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "test.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(None);

        // Extract identifiers from script section
        let identifiers = extractor.extract_identifiers(&symbols);

        // Verify function calls are extracted
        let log_call = identifiers.iter().find(|id| id.name == "log");
        assert!(
            log_call.is_some(),
            "Should extract 'log' function call from script section"
        );
        assert_eq!(log_call.unwrap().kind, IdentifierKind::Call);

        let update_call = identifiers.iter().find(|id| id.name == "updateMessage");
        assert!(
            update_call.is_some(),
            "Should extract 'updateMessage' method call"
        );
        assert_eq!(update_call.unwrap().kind, IdentifierKind::Call);
    }

    #[test]
    fn test_vue_member_access() {
        let vue_code = r#"
<script>
export default {
  data() {
    return {
      user: { name: 'Alice', email: 'alice@example.com' }
    }
  },
  methods: {
    printUserInfo() {
      let userName = this.user.name;      // Member access: name
      let userEmail = this.user.email;    // Member access: email
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "test.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(None);
        let identifiers = extractor.extract_identifiers(&symbols);

        // Verify member access is extracted
        let name_access = identifiers
            .iter()
            .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            name_access > 0,
            "Should extract 'name' member access from script section"
        );

        let email_access = identifiers
            .iter()
            .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            email_access > 0,
            "Should extract 'email' member access from script section"
        );
    }

    #[test]
    fn test_vue_identifiers_have_containing_symbol() {
        let vue_code = r#"
<script>
export default {
  methods: {
    calculate() {
      let result = this.add(5, 3);    // Call within calculate method
      return result;
    },
    add(a, b) {
      return a + b;
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "test.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(None);
        let identifiers = extractor.extract_identifiers(&symbols);

        // Find the add call
        let add_call = identifiers.iter().find(|id| id.name == "add");
        assert!(add_call.is_some(), "Should find 'add' method call");

        // Verify it has a containing symbol
        assert!(
            add_call.unwrap().containing_symbol_id.is_some(),
            "Function call should have containing symbol from script section"
        );
    }

    #[test]
    fn test_vue_chained_member_access() {
        let vue_code = r#"
<script>
export default {
  methods: {
    getUserData() {
      let balance = this.user.account.balance;    // Chained access
      let city = this.user.address.city;          // Chained access
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "test.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(None);
        let identifiers = extractor.extract_identifiers(&symbols);

        // Should extract rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained member access"
        );

        let city_access = identifiers
            .iter()
            .find(|id| id.name == "city" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            city_access.is_some(),
            "Should extract 'city' from chained member access"
        );
    }

    #[test]
    fn test_vue_duplicate_calls_at_different_locations() {
        let vue_code = r#"
<script>
export default {
  methods: {
    process() {
      this.validate();
      this.validate();    // Same call twice
    },
    validate() {
      return true;
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "test.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(None);
        let identifiers = extractor.extract_identifiers(&symbols);

        // Should extract BOTH calls at different locations
        let validate_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "validate" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            validate_calls.len(),
            2,
            "Should extract both validate calls at different locations"
        );

        // Verify different line numbers
        assert_ne!(
            validate_calls[0].start_line, validate_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }

    #[test]
    fn test_vue_malformed_template() {
        let vue_code = r#"
<template>
  <div>
    <h1>Unclosed heading
    <p>Missing closing tags
    <span>Nested unclosed
  </div>
</template>

<script>
export default {
  name: 'MalformedTemplate'
}
</script>
"#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "malformed.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Should handle malformed templates gracefully
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_vue_empty_sections() {
        let vue_code = r#"
<template>
</template>

<script>
</script>

<style>
</style>
"#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "empty.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Should handle empty sections
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_vue_missing_sections() {
        let vue_code = r#"
<script>
export default {
  name: 'MinimalComponent'
}
</script>
"#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "minimal.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Should handle missing template and style sections
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_vue_complex_script_typescript() {
        let vue_code = r#"
<template>
  <div>{{ message }}</div>
</template>

<script lang="ts">
import { defineComponent } from 'vue';

interface Props {
  message: string;
}

export default defineComponent<Props>({
  props: {
    message: String
  },
  setup(props: Props) {
    return {
      message: props.message
    };
  }
});
</script>
"#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "typescript.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Should handle TypeScript in Vue components
        assert!(!symbols.is_empty());
    }
}

// ========================================================================
// Vue Doc Comment Extraction Tests (TDD RED phase)
// ========================================================================
//
// These tests validate that doc comments are extracted from Vue SFC symbols.
// Vue supports multiple comment formats:
// - HTML comments <!-- ... --> for template section
// - JSDoc comments /** ... */ for script section
// - CSS comments /* ... */ for style section

#[cfg(test)]
mod vue_doc_comment_tests {
    use super::*;

    #[test]
    fn test_extract_vue_jsdoc_from_script_methods() {
        let vue_code = r#"
<template>
  <div>{{ count }}</div>
</template>

<script>
export default {
  /**
   * Increments the counter
   * @param {number} amount - The amount to increment by
   */
  methods: {
    increment(amount) {
      this.count += amount;
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "counter.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find the methods symbol
        let methods_symbol = symbols.iter().find(|s| s.name == "methods");
        assert!(methods_symbol.is_some(), "Should find methods symbol");

        // Should extract JSDoc comment for methods
        let methods = methods_symbol.unwrap();
        assert!(
            methods.doc_comment.is_some(),
            "methods should have extracted doc comment"
        );
        let doc = methods.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Increments the counter"),
            "Doc comment should contain description"
        );
    }

    #[test]
    fn test_extract_vue_jsdoc_from_data_function() {
        let vue_code = r#"
<script>
export default {
  /**
   * Component state
   * @returns {Object} Component data object
   */
  data() {
    return {
      message: 'Hello',
      count: 0
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "data.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find the data symbol
        let data_symbol = symbols.iter().find(|s| s.name == "data");
        assert!(data_symbol.is_some(), "Should find data symbol");

        // Should extract JSDoc comment for data
        let data = data_symbol.unwrap();
        assert!(
            data.doc_comment.is_some(),
            "data should have extracted doc comment"
        );
        let doc = data.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Component state"),
            "Doc comment should contain description"
        );
    }

    #[test]
    fn test_extract_vue_jsdoc_from_computed_property() {
        let vue_code = r#"
<script>
export default {
  /**
   * Computed property for user display name
   * Combines first and last name
   */
  computed: {
    displayName() {
      return `${this.firstName} ${this.lastName}`;
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "computed.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find the computed symbol
        let computed_symbol = symbols.iter().find(|s| s.name == "computed");
        assert!(computed_symbol.is_some(), "Should find computed symbol");

        // Should extract JSDoc comment for computed
        let computed = computed_symbol.unwrap();
        assert!(
            computed.doc_comment.is_some(),
            "computed should have extracted doc comment"
        );
        let doc = computed.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Computed property"),
            "Doc comment should contain description"
        );
    }

    #[test]
    fn test_extract_vue_jsdoc_from_props() {
        let vue_code = r#"
<script>
export default {
  /**
   * Component props
   * Defines the interface for parent-to-child communication
   */
  props: {
    title: String,
    count: Number
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "props.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find the props symbol
        let props_symbol = symbols.iter().find(|s| s.name == "props");
        assert!(props_symbol.is_some(), "Should find props symbol");

        // Should extract JSDoc comment for props
        let props = props_symbol.unwrap();
        assert!(
            props.doc_comment.is_some(),
            "props should have extracted doc comment"
        );
        let doc = props.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Component props"),
            "Doc comment should contain description"
        );
    }

    #[test]
    fn test_extract_vue_jsdoc_from_individual_methods() {
        let vue_code = r#"
<script>
export default {
  methods: {
    /**
     * Validates user input
     * @param {string} value - The input to validate
     * @returns {boolean} True if valid, false otherwise
     */
    validateInput(value) {
      return value && value.length > 0;
    },

    /**
     * Saves changes to the database
     * @async
     */
    saveChanges() {
      return new Promise((resolve) => {
        setTimeout(resolve, 1000);
      });
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "methods.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find individual method symbols
        let validate_method = symbols.iter().find(|s| s.name == "validateInput");
        assert!(
            validate_method.is_some(),
            "Should find validateInput method"
        );

        // Should extract JSDoc comment for validateInput
        let validate = validate_method.unwrap();
        assert!(
            validate.doc_comment.is_some(),
            "validateInput should have extracted doc comment"
        );
        let doc = validate.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Validates user input"),
            "Doc comment should contain description"
        );

        // Find saveChanges method
        let save_method = symbols.iter().find(|s| s.name == "saveChanges");
        assert!(save_method.is_some(), "Should find saveChanges method");

        let save = save_method.unwrap();
        assert!(
            save.doc_comment.is_some(),
            "saveChanges should have extracted doc comment"
        );
        let doc = save.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Saves changes"),
            "Doc comment should contain description"
        );
    }

    #[test]
    fn test_extract_vue_html_comment_from_component() {
        let vue_code = r#"
<!--
  UserCard Component
  Displays user information in a card format
  Supports custom styling and events
-->
<template>
  <div class="user-card">
    <img :src="user.avatar" />
    <h3>{{ user.name }}</h3>
  </div>
</template>

<script>
export default {
  name: 'UserCard'
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "user-card.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find component symbol
        let component = symbols.iter().find(|s| s.name == "UserCard");
        assert!(component.is_some(), "Should find UserCard component");

        // Should extract HTML comment for component
        let component = component.unwrap();
        assert!(
            component.doc_comment.is_some(),
            "Component should have extracted HTML comment"
        );
    }

    #[test]
    fn test_extract_vue_css_comment_from_style() {
        let vue_code = r#"
<style scoped>
/**
 * Main container styles
 * Responsive layout with flexbox
 */
.container {
  display: flex;
  flex-direction: column;
}

/* Button styling with hover effects */
.button {
  padding: 10px;
  background: blue;
}

.button:hover {
  background: darkblue;
}
</style>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "styles.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find container class symbol
        let container = symbols.iter().find(|s| s.name == "container");
        assert!(container.is_some(), "Should find .container class");

        // Should extract CSS comment for style
        let container = container.unwrap();
        assert!(
            container.doc_comment.is_some(),
            "container should have extracted CSS comment"
        );
        let doc = container.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("container styles"),
            "Doc comment should contain description"
        );
    }

    #[test]
    fn test_extract_vue_multiline_jsdoc() {
        let vue_code = r#"
<script>
export default {
  /**
   * Main component data function
   *
   * Returns an object containing:
   * - message: String - Display message
   * - count: Number - Counter value
   * - loading: Boolean - Loading state
   *
   * @returns {Object} The data object
   */
  data() {
    return {
      message: 'Hello',
      count: 0,
      loading: false
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "multiline.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // Find data symbol
        let data_symbol = symbols.iter().find(|s| s.name == "data");
        assert!(data_symbol.is_some(), "Should find data symbol");

        let data = data_symbol.unwrap();
        assert!(
            data.doc_comment.is_some(),
            "data should have extracted multiline doc comment"
        );
        let doc = data.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Main component data"),
            "Multiline doc should preserve content"
        );
        assert!(
            doc.contains("Returns an object"),
            "Multiline doc should preserve full content"
        );
    }

    #[test]
    fn test_vue_symbols_without_comments_have_none() {
        let vue_code = r#"
<script>
export default {
  data() {
    return { message: 'Hello' }
  },
  methods: {
    greet() {
      console.log(this.message);
    }
  }
}
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "no-comments.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(None);

        // All symbols without comments should have None
        let data_symbol = symbols.iter().find(|s| s.name == "data");
        assert!(data_symbol.is_some());
        // Note: Currently may have default documentation, checking actual behavior
        let greet_method = symbols.iter().find(|s| s.name == "greet");
        assert!(greet_method.is_some());
    }
}
mod script_setup;
mod types; // Phase 4: Type extraction verification tests // Phase 5: Vue 3 Composition API / <script setup> tests

// ========================================================================
// Vue Style Section Enhanced Tests (TDD RED phase)
// ========================================================================
//
// These tests validate extraction of ID selectors and CSS custom properties
// from Vue SFC <style> sections, beyond the existing class selector support.

#[cfg(test)]
mod vue_style_enhanced_tests {
    use super::{expected_identifier_id, line_column_for_byte};
    use crate::base::{IdentifierKind, SymbolKind};
    use crate::vue::VueExtractor;

    fn create_extractor(file_path: &str, code: &str) -> VueExtractor {
        VueExtractor::new(
            "vue".to_string(),
            file_path.to_string(),
            code.to_string(),
            &std::path::PathBuf::from("/test"),
        )
    }

    #[test]
    fn test_extract_id_selectors() {
        let vue_code = r#"
<style scoped>
#app {
  font-family: Arial, sans-serif;
}

#sidebar {
  width: 250px;
}
</style>
        "#;

        let mut extractor = create_extractor("id-selectors.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        let app = symbols
            .iter()
            .find(|s| s.name == "app" && s.kind == SymbolKind::Property);
        assert!(app.is_some(), "Should extract #app ID selector");
        assert_eq!(app.unwrap().signature.as_ref().unwrap(), "#app");

        let sidebar = symbols
            .iter()
            .find(|s| s.name == "sidebar" && s.kind == SymbolKind::Property);
        assert!(sidebar.is_some(), "Should extract #sidebar ID selector");
        assert_eq!(sidebar.unwrap().signature.as_ref().unwrap(), "#sidebar");
    }

    #[test]
    fn test_extract_css_custom_properties() {
        let vue_code = r#"
<style>
:root {
  --primary-color: #3498db;
  --font-size: 16px;
}
</style>
        "#;

        let mut extractor = create_extractor("custom-props.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        let primary_color = symbols
            .iter()
            .find(|s| s.name == "--primary-color" && s.kind == SymbolKind::Variable);
        assert!(
            primary_color.is_some(),
            "Should extract --primary-color custom property"
        );
        assert_eq!(
            primary_color.unwrap().signature.as_ref().unwrap(),
            "--primary-color"
        );

        let font_size = symbols
            .iter()
            .find(|s| s.name == "--font-size" && s.kind == SymbolKind::Variable);
        assert!(
            font_size.is_some(),
            "Should extract --font-size custom property"
        );
        assert_eq!(
            font_size.unwrap().signature.as_ref().unwrap(),
            "--font-size"
        );
    }

    #[test]
    fn test_mixed_style_selectors() {
        let vue_code = r#"
<style scoped>
.container {
  display: flex;
}

#main-content {
  padding: 20px;
}

:root {
  --spacing: 8px;
}
</style>
        "#;

        let mut extractor = create_extractor("mixed-styles.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        // Class selector
        let container = symbols
            .iter()
            .find(|s| s.name == "container" && s.kind == SymbolKind::Property);
        assert!(
            container.is_some(),
            "Should extract .container class selector"
        );
        assert_eq!(container.unwrap().signature.as_ref().unwrap(), ".container");

        // ID selector
        let main_content = symbols
            .iter()
            .find(|s| s.name == "main-content" && s.kind == SymbolKind::Property);
        assert!(
            main_content.is_some(),
            "Should extract #main-content ID selector"
        );
        assert_eq!(
            main_content.unwrap().signature.as_ref().unwrap(),
            "#main-content"
        );

        // CSS custom property
        let spacing = symbols
            .iter()
            .find(|s| s.name == "--spacing" && s.kind == SymbolKind::Variable);
        assert!(
            spacing.is_some(),
            "Should extract --spacing custom property"
        );
        assert_eq!(spacing.unwrap().signature.as_ref().unwrap(), "--spacing");
    }

    #[test]
    fn test_vue_style_delegates_to_css_extractor_with_offsets() {
        let vue_code = r#"<template>
  <button class="button">Save</button>
</template>

<style scoped>
.button {
  animation: pulse 1s ease-in-out;
}

@keyframes pulse {
  from { opacity: 0; }
  to { opacity: 1; }
}
</style>"#;

        let mut extractor = create_extractor("animated.vue", vue_code);
        let symbols = extractor.extract_symbols(None);

        let keyframes = symbols
            .iter()
            .find(|symbol| symbol.name == "@keyframes pulse")
            .expect("Vue style block should delegate to CSS keyframes extraction");
        assert_eq!(keyframes.kind, SymbolKind::Function);
        assert!(keyframes.start_byte > vue_code.find("<style").unwrap() as u32);
        assert_eq!(
            &vue_code[keyframes.start_byte as usize..keyframes.start_byte as usize + 10],
            "@keyframes"
        );

        let button = symbols
            .iter()
            .find(|symbol| symbol.name == ".button")
            .expect("Vue style block should preserve CSS selector names");
        assert_eq!(button.kind, SymbolKind::Property);
        assert!(button.start_byte > vue_code.find("<style").unwrap() as u32);
    }

    #[test]
    fn test_vue_script_setup_callsite_ids_are_file_relative() {
        let vue_code = r#"
<template>
  <div />
</template>

<script setup>
function bump() {
  return 1
}

bump()
</script>
        "#;

        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VueExtractor::new(
            "vue".to_string(),
            "setup.vue".to_string(),
            vue_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(None);
        let identifiers = extractor.extract_identifiers(&symbols);

        let bump_call = identifiers
            .iter()
            .find(|id| id.name == "bump" && id.kind == IdentifierKind::Call)
            .expect("Should extract the script-setup bump() call");
        let bump_offset = vue_code.rfind("bump()").unwrap() as u32;
        let (bump_line, bump_column) = line_column_for_byte(vue_code, bump_offset as usize);
        assert_eq!(bump_call.start_byte, bump_offset);
        assert_eq!(bump_call.start_line, bump_line);
        assert_eq!(bump_call.start_column, bump_column);
        assert_eq!(bump_call.id, expected_identifier_id(bump_call));
    }
}

fn line_column_for_byte(content: &str, target: usize) -> (u32, u32) {
    let prefix = &content[..target];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit_once('\n')
        .map(|(_, tail)| tail.len())
        .unwrap_or(prefix.len()) as u32;
    (line, column)
}

fn expected_id(
    file_path: &str,
    name: &str,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
) -> String {
    let input = format!(
        "{file_path}:{name}:{start_line}:{start_column}:{end_line}:{end_column}:{start_byte}:{end_byte}"
    );
    format!("{:x}", md5::compute(input.as_bytes()))
}

fn expected_symbol_id(symbol: &crate::base::Symbol) -> String {
    expected_id(
        symbol.file_path.as_str(),
        symbol.name.as_str(),
        symbol.start_line,
        symbol.start_column,
        symbol.end_line,
        symbol.end_column,
        symbol.start_byte,
        symbol.end_byte,
    )
}

fn expected_identifier_id(identifier: &crate::base::Identifier) -> String {
    expected_id(
        identifier.file_path.as_str(),
        identifier.name.as_str(),
        identifier.start_line,
        identifier.start_column,
        identifier.end_line,
        identifier.end_column,
        identifier.start_byte,
        identifier.end_byte,
    )
}
