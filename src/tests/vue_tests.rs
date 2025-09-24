// Port of Miller's comprehensive Vue extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

use crate::extractors::base::SymbolKind;
use crate::extractors::vue::VueExtractor;

#[cfg(test)]
mod vue_extractor_tests {
    use super::*;

    // Helper function to create a VueExtractor - Vue doesn't use tree-sitter
    fn create_extractor(file_path: &str, code: &str) -> VueExtractor {
        VueExtractor::new("vue".to_string(), file_path.to_string(), code.to_string())
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
        assert!(component.signature.as_ref().unwrap().contains("<HelloWorld />"));
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