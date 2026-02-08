// Vue Composition API / <script setup> Tests
//
// These tests validate extraction of Vue 3 Composition API patterns:
// - <script setup> with ref(), reactive(), computed()
// - <script setup> function declarations and arrow functions
// - <script setup lang="ts"> TypeScript support
// - defineProps(), defineEmits(), defineExpose() macros
// - Existing Options API tests still passing

use crate::base::SymbolKind;
use crate::vue::VueExtractor;
use std::path::PathBuf;

fn create_extractor(file_path: &str, code: &str) -> VueExtractor {
    let workspace_root = PathBuf::from("/tmp/test");
    VueExtractor::new(
        "vue".to_string(),
        file_path.to_string(),
        code.to_string(),
        &workspace_root,
    )
}

#[test]
fn test_script_setup_ref_and_computed() {
    let vue_code = r#"<template>
  <div>{{ count }} - {{ doubled }}</div>
</template>

<script setup>
import { ref, computed } from 'vue'

const count = ref(0)
const doubled = computed(() => count.value * 2)
</script>"#;

    let mut extractor = create_extractor("counter.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    // Should find ref variable
    let count_sym = symbols.iter().find(|s| s.name == "count");
    assert!(count_sym.is_some(), "Should extract 'count' ref variable");
    let count_sym = count_sym.unwrap();
    assert_eq!(count_sym.kind, SymbolKind::Variable);

    // Should find computed variable
    let doubled_sym = symbols.iter().find(|s| s.name == "doubled");
    assert!(
        doubled_sym.is_some(),
        "Should extract 'doubled' computed variable"
    );
    assert_eq!(doubled_sym.unwrap().kind, SymbolKind::Variable);

    // Should find the import
    let import_sym = symbols.iter().find(|s| s.name == "ref");
    assert!(import_sym.is_some(), "Should extract 'ref' import");

    // Component symbol should still exist
    let component = symbols.iter().find(|s| s.kind == SymbolKind::Class);
    assert!(
        component.is_some(),
        "Component-level symbol should still be created"
    );
}

#[test]
fn test_script_setup_function_declarations() {
    let vue_code = r#"<template>
  <button @click="handleClick">Click me</button>
</template>

<script setup>
import { ref } from 'vue'

const count = ref(0)

function handleClick() {
  count.value++
}

function resetCount() {
  count.value = 0
}
</script>"#;

    let mut extractor = create_extractor("buttons.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    let handle_click = symbols.iter().find(|s| s.name == "handleClick");
    assert!(
        handle_click.is_some(),
        "Should extract 'handleClick' function declaration"
    );
    assert_eq!(handle_click.unwrap().kind, SymbolKind::Function);

    let reset_count = symbols.iter().find(|s| s.name == "resetCount");
    assert!(
        reset_count.is_some(),
        "Should extract 'resetCount' function declaration"
    );
    assert_eq!(reset_count.unwrap().kind, SymbolKind::Function);
}

#[test]
fn test_script_setup_arrow_functions() {
    let vue_code = r#"<script setup>
const greet = (name) => {
  return `Hello, ${name}!`
}

const add = (a, b) => a + b
</script>"#;

    let mut extractor = create_extractor("arrows.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    let greet = symbols.iter().find(|s| s.name == "greet");
    assert!(greet.is_some(), "Should extract 'greet' arrow function");
    // Arrow functions assigned to const are extracted as Functions
    assert_eq!(greet.unwrap().kind, SymbolKind::Function);

    let add = symbols.iter().find(|s| s.name == "add");
    assert!(add.is_some(), "Should extract 'add' arrow function");
    assert_eq!(add.unwrap().kind, SymbolKind::Function);
}

#[test]
fn test_script_setup_typescript() {
    let vue_code = r#"<template>
  <div>{{ message }}</div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

interface User {
  name: string
  age: number
}

const message = ref<string>('Hello')
const user = ref<User>({ name: 'Alice', age: 30 })

function greetUser(user: User): string {
  return `Hello, ${user.name}!`
}
</script>"#;

    let mut extractor = create_extractor("typescript.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    // Should extract ref variables even with TypeScript generics
    let message = symbols.iter().find(|s| s.name == "message");
    assert!(
        message.is_some(),
        "Should extract 'message' ref with TypeScript generics"
    );

    let user = symbols
        .iter()
        .find(|s| s.name == "user" && s.kind == SymbolKind::Variable);
    assert!(
        user.is_some(),
        "Should extract 'user' ref with TypeScript generics"
    );

    // Should extract TypeScript function
    let greet_user = symbols.iter().find(|s| s.name == "greetUser");
    assert!(
        greet_user.is_some(),
        "Should extract TypeScript function in <script setup lang=\"ts\">"
    );
    assert_eq!(greet_user.unwrap().kind, SymbolKind::Function);
}

#[test]
fn test_script_setup_define_props_and_emits() {
    let vue_code = r#"<script setup>
const props = defineProps({
  title: String,
  count: {
    type: Number,
    default: 0
  }
})

const emit = defineEmits(['update', 'delete'])

defineExpose({
  reset() {
    // exposed method
  }
})
</script>"#;

    let mut extractor = create_extractor("defines.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    // defineProps result should be extracted
    let props = symbols.iter().find(|s| s.name == "props");
    assert!(props.is_some(), "Should extract 'props' from defineProps()");

    // defineEmits result should be extracted
    let emit = symbols.iter().find(|s| s.name == "emit");
    assert!(emit.is_some(), "Should extract 'emit' from defineEmits()");
}

#[test]
fn test_script_setup_imports() {
    let vue_code = r#"<script setup>
import { ref, computed, watch } from 'vue'
import { useRouter } from 'vue-router'
import MyComponent from './MyComponent.vue'

const count = ref(0)
</script>"#;

    let mut extractor = create_extractor("imports.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    // Should extract named imports
    let ref_import = symbols
        .iter()
        .find(|s| s.name == "ref" && s.kind == SymbolKind::Import);
    assert!(ref_import.is_some(), "Should extract 'ref' import");

    let computed_import = symbols
        .iter()
        .find(|s| s.name == "computed" && s.kind == SymbolKind::Import);
    assert!(computed_import.is_some(), "Should extract 'computed' import");

    let watch_import = symbols
        .iter()
        .find(|s| s.name == "watch" && s.kind == SymbolKind::Import);
    assert!(watch_import.is_some(), "Should extract 'watch' import");

    // Should extract default imports
    let my_component = symbols
        .iter()
        .find(|s| s.name == "MyComponent" && s.kind == SymbolKind::Import);
    assert!(
        my_component.is_some(),
        "Should extract 'MyComponent' default import"
    );
}

#[test]
fn test_script_setup_line_numbers_are_file_relative() {
    // Line numbers must be relative to the .vue file, not the script section
    // Convention: section.start_line = tag line index + 1, first content line
    // gets start_line + tree_sitter_row, matching the Options API behavior
    let vue_code = r#"<template>
  <div>Hello</div>
</template>

<script setup>
const count = ref(0)
</script>"#;

    let mut extractor = create_extractor("lines.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    let count = symbols.iter().find(|s| s.name == "count");
    assert!(count.is_some(), "Should extract 'count'");
    let count = count.unwrap();

    // <template> is at line index 0, <script setup> is at index 4
    // start_line = 4 + 1 = 5, tree-sitter row for first content = 0
    // So start_line = 5 (file-relative, matching Options API convention)
    assert!(
        count.start_line >= 5,
        "Line number should be file-relative (>= 5), got {}",
        count.start_line
    );
    // Should NOT be 0 (section-relative)
    assert!(
        count.start_line > 1,
        "Line number should NOT be section-relative (1), got {}",
        count.start_line
    );
}

#[test]
fn test_script_setup_reactive() {
    let vue_code = r#"<script setup>
import { reactive } from 'vue'

const state = reactive({
  count: 0,
  message: 'Hello'
})
</script>"#;

    let mut extractor = create_extractor("reactive.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    let state = symbols.iter().find(|s| s.name == "state");
    assert!(state.is_some(), "Should extract 'state' reactive variable");
    assert_eq!(state.unwrap().kind, SymbolKind::Variable);
}

#[test]
fn test_options_api_still_works() {
    // Verify existing Options API extraction is unaffected
    let vue_code = r#"<script>
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
    }
  }
}
</script>"#;

    let mut extractor = create_extractor("options.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    assert!(
        symbols.iter().any(|s| s.name == "data"),
        "Options API data() should still work"
    );
    assert!(
        symbols.iter().any(|s| s.name == "computed"),
        "Options API computed should still work"
    );
    assert!(
        symbols.iter().any(|s| s.name == "methods"),
        "Options API methods should still work"
    );
    assert!(
        symbols.iter().any(|s| s.name == "greet"),
        "Options API method functions should still work"
    );
}

#[test]
fn test_parsing_detects_setup_attribute() {
    // Verify the parser correctly sets is_setup
    use crate::vue::parsing::parse_vue_sfc;

    let content = r#"<script setup>
const x = 1
</script>"#;

    let sections = parse_vue_sfc(content).unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].section_type, "script");
    assert!(
        sections[0].is_setup,
        "Parser should detect 'setup' attribute on <script>"
    );

    // Regular script should NOT be setup
    let content2 = r#"<script>
export default {}
</script>"#;

    let sections2 = parse_vue_sfc(content2).unwrap();
    assert_eq!(sections2.len(), 1);
    assert!(
        !sections2[0].is_setup,
        "Regular <script> should not be marked as setup"
    );

    // script setup with lang
    let content3 = r#"<script setup lang="ts">
const x = 1
</script>"#;

    let sections3 = parse_vue_sfc(content3).unwrap();
    assert_eq!(sections3.len(), 1);
    assert!(
        sections3[0].is_setup,
        "<script setup lang=\"ts\"> should be detected as setup"
    );
    assert_eq!(sections3[0].lang.as_deref(), Some("ts"));
}
