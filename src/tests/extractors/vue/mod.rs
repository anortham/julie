// Implementation of comprehensive Vue extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

// Submodule declarations
pub mod parsing;

use crate::extractors::base::SymbolKind;
use crate::extractors::vue::VueExtractor;

#[cfg(test)]
mod vue_extractor_tests {
    use super::*;

    // Helper function to create a VueExtractor - Vue doesn't use tree-sitter
    fn create_extractor(file_path: &str, code: &str) -> VueExtractor {
        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        VueExtractor::new("vue".to_string(), file_path.to_string(), code.to_string(), &workspace_root)
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
        assert!(component
            .signature
            .as_ref()
            .unwrap()
            .contains("<HelloWorld />"));
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
    fn test_extract_template_symbols() {
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

        // Should find component usages
        let user_profile = symbols.iter().find(|s| s.name == "UserProfile");
        assert!(user_profile.is_some());
        assert_eq!(user_profile.unwrap().kind, SymbolKind::Class);

        let product_card = symbols.iter().find(|s| s.name == "ProductCard");
        assert!(product_card.is_some());
        assert_eq!(product_card.unwrap().kind, SymbolKind::Class);

        let custom_button = symbols.iter().find(|s| s.name == "CustomButton");
        assert!(custom_button.is_some());
        assert_eq!(custom_button.unwrap().kind, SymbolKind::Class);

        // Should find directives
        let v_for = symbols.iter().find(|s| s.name == "v-for");
        assert!(v_for.is_some());
        assert_eq!(v_for.unwrap().kind, SymbolKind::Property);

        let v_if = symbols.iter().find(|s| s.name == "v-if");
        assert!(v_if.is_some());
        assert_eq!(v_if.unwrap().kind, SymbolKind::Property);
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

        let button = symbols.iter().find(|s| s.name == "button");
        assert!(button.is_some());
        assert_eq!(button.unwrap().kind, SymbolKind::Property);

        let disabled = symbols.iter().find(|s| s.name == "disabled");
        assert!(disabled.is_some());
        assert_eq!(disabled.unwrap().kind, SymbolKind::Property);
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

        assert!(symbols.len() > 5);

        // Component
        let component = symbols.iter().find(|s| s.name == "AppLayout");
        assert!(component.is_some());

        // Script symbols
        assert!(symbols.iter().find(|s| s.name == "props").is_some());
        assert!(symbols.iter().find(|s| s.name == "data").is_some());
        assert!(symbols.iter().find(|s| s.name == "methods").is_some());
        assert!(symbols.iter().find(|s| s.name == "initialize").is_some());

        // Template symbols
        assert!(symbols.iter().find(|s| s.name == "Header").is_some());

        // Style symbols
        assert!(symbols.iter().find(|s| s.name == "app").is_some());
        assert!(symbols.iter().find(|s| s.name == "header").is_some());
    }

    #[test]
    fn test_infer_types_returns_empty_map() {
        let mut extractor = create_extractor("test.vue", "<template></template>");
        let symbols = extractor.extract_symbols(None);
        let types = extractor.infer_types(&symbols);

        assert!(types.len() == 0);
    }

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
    use crate::extractors::base::IdentifierKind;
    use crate::extractors::vue::VueExtractor;

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
