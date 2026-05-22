// Vue Composition API / <script setup> Tests
//
// These tests validate extraction of Vue 3 Composition API patterns:
// - <script setup> with ref(), reactive(), computed()
// - <script setup> function declarations and arrow functions
// - <script setup lang="ts"> TypeScript support
// - defineProps(), defineEmits(), defineExpose() macros
// - Existing Options API tests still passing

use super::{expected_identifier_id, expected_symbol_id, line_column_for_byte};
use crate::base::{IdentifierKind, SymbolKind};
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
    let props = props.unwrap();
    let props_offset = vue_code.find("props = defineProps").unwrap() as u32;
    let (props_line, props_column) = line_column_for_byte(vue_code, props_offset as usize);
    assert_eq!(props.kind, SymbolKind::Variable);
    assert_eq!(props.start_byte, props_offset);
    assert_eq!(props.start_line, props_line);
    assert_eq!(props.start_column, props_column);
    assert_eq!(props.id, expected_symbol_id(props));

    // defineEmits result should be extracted
    let emit = symbols.iter().find(|s| s.name == "emit");
    assert!(emit.is_some(), "Should extract 'emit' from defineEmits()");
    let emit = emit.unwrap();
    let emit_offset = vue_code.find("emit = defineEmits").unwrap() as u32;
    let (emit_line, emit_column) = line_column_for_byte(vue_code, emit_offset as usize);
    assert_eq!(emit.kind, SymbolKind::Variable);
    assert_eq!(emit.start_byte, emit_offset);
    assert_eq!(emit.start_line, emit_line);
    assert_eq!(emit.start_column, emit_column);
    assert_eq!(emit.id, expected_symbol_id(emit));

    // defineExpose should be extracted as a standalone macro symbol
    let define_expose = symbols.iter().find(|s| s.name == "defineExpose");
    assert!(
        define_expose.is_some(),
        "Should extract 'defineExpose' standalone macro"
    );
    let define_expose = define_expose.unwrap();
    let define_expose_offset = vue_code.find("defineExpose").unwrap() as u32;
    let (define_expose_line, define_expose_column) =
        line_column_for_byte(vue_code, define_expose_offset as usize);
    assert_eq!(define_expose.kind, SymbolKind::Function);
    assert_eq!(define_expose.start_byte, define_expose_offset);
    assert_eq!(define_expose.start_line, define_expose_line);
    assert_eq!(define_expose.start_column, define_expose_column);
    assert_eq!(define_expose.id, expected_symbol_id(define_expose));
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
    let ref_import = ref_import.unwrap();
    let ref_offset = vue_code.find("ref, computed, watch").unwrap() as u32;
    let (ref_line, ref_column) = line_column_for_byte(vue_code, ref_offset as usize);
    assert_eq!(ref_import.start_byte, ref_offset);
    assert_eq!(ref_import.start_line, ref_line);
    assert_eq!(ref_import.start_column, ref_column);
    assert_eq!(ref_import.id, expected_symbol_id(ref_import));

    let computed_import = symbols
        .iter()
        .find(|s| s.name == "computed" && s.kind == SymbolKind::Import);
    assert!(
        computed_import.is_some(),
        "Should extract 'computed' import"
    );

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
    let my_component = my_component.unwrap();
    let my_component_offset = vue_code.find("MyComponent from").unwrap() as u32;
    let (my_component_line, my_component_column) =
        line_column_for_byte(vue_code, my_component_offset as usize);
    assert_eq!(my_component.start_byte, my_component_offset);
    assert_eq!(my_component.start_line, my_component_line);
    assert_eq!(my_component.start_column, my_component_column);
    assert_eq!(my_component.id, expected_symbol_id(my_component));
}

#[test]
fn test_script_setup_line_numbers_are_file_relative() {
    // Line numbers must be relative to the .vue file, not the script section.
    // `section.start_line` is the section tag line, so content rows add one more line.
    let vue_code = r#"<template>
  <div>Hello</div>
</template>

<script setup>
import { ref } from 'vue'
const count = ref(0)
defineExpose({ count })
</script>"#;

    let mut extractor = create_extractor("lines.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    let ref_import = symbols.iter().find(|s| s.name == "ref");
    assert!(ref_import.is_some(), "Should extract 'ref' import");
    let ref_import = ref_import.unwrap();
    assert_eq!(
        ref_import.start_line, 6,
        "Import line should point at the script content line, got {}",
        ref_import.start_line
    );

    let count = symbols.iter().find(|s| s.name == "count");
    assert!(count.is_some(), "Should extract 'count'");
    let count = count.unwrap();
    assert_eq!(
        count.start_line, 7,
        "Variable line should point at the script content line, got {}",
        count.start_line
    );

    let define_expose = symbols.iter().find(|s| s.name == "defineExpose");
    assert!(
        define_expose.is_some(),
        "Should extract 'defineExpose' macro call"
    );
    let define_expose = define_expose.unwrap();
    assert_eq!(
        define_expose.start_line, 8,
        "Macro line should point at the script content line, got {}",
        define_expose.start_line
    );

    assert!(
        count.start_line > 1,
        "Line number should not be section-relative (1), got {}",
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
fn test_vue_options_api_methods_computed_data_props_emit_member_symbols() {
    let vue_code = r#"<script lang="ts">
export default {
  props: {
    title: String,
    count: Number
  },
  emits: ['save', 'cancel'],
  data() {
    return {
      total: 0,
      enabled: true
    }
  },
  computed: {
    doubled() {
      return this.total * 2
    }
  },
  methods: {
    increment() {
      this.total++
    },
    decrement() {
      this.total--
    }
  }
}
</script>"#;

    let mut extractor = create_extractor("counter.vue", vue_code);
    let symbols = extractor.extract_symbols(None);

    for (name, kind) in [
        ("title", SymbolKind::Property),
        ("count", SymbolKind::Property),
        ("save", SymbolKind::Event),
        ("cancel", SymbolKind::Event),
        ("total", SymbolKind::Property),
        ("enabled", SymbolKind::Property),
        ("doubled", SymbolKind::Method),
        ("increment", SymbolKind::Method),
        ("decrement", SymbolKind::Method),
    ] {
        let symbol = symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("missing Options API symbol {name}; got {symbols:#?}"));
        assert_eq!(symbol.kind, kind, "symbol {name} has wrong kind");
        assert!(
            symbol.start_byte > 0,
            "symbol {name} should keep full-file start byte"
        );
        assert!(
            symbol.end_byte > symbol.start_byte,
            "symbol {name} should keep byte range"
        );
    }
}

#[test]
fn test_vue_script_symbols_have_full_file_byte_ranges() {
    let vue_code = r#"<template>
  <div>{{ total }}</div>
</template>

<script>
export default {
  methods: {
    increment() {
      this.total++
    }
  }
}
</script>"#;

    let mut extractor = create_extractor("ranges.vue", vue_code);
    let symbols = extractor.extract_symbols(None);
    let increment = symbols
        .iter()
        .find(|symbol| symbol.name == "increment")
        .expect("increment method should be extracted");

    assert!(increment.start_byte > vue_code.find("<script>").unwrap() as u32);
    assert_eq!(
        &vue_code[increment.start_byte as usize..increment.end_byte as usize],
        "increment"
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

#[test]
fn test_vue_identifier_extraction_parses_script_section_once() {
    // Regression guard: the Vue identifier extractor must walk each <script>
    // section exactly once per extract_identifiers() call. The historical bug
    // shape was re-parsing the script section on every extraction pass
    // (identifiers, symbols, relationships), producing duplicate output.
    //
    // Behavioral assertion (Form C): a single function call in the script
    // section must produce exactly 1 Call identifier. A count of 2 means
    // the section was walked twice; 0 means the extractor is broken.
    let vue_code = r#"<template>
  <div>Hello</div>
</template>

<script>
function run() {
  doOnce()
}
</script>"#;

    let mut extractor = create_extractor("parses-once.vue", vue_code);
    let symbols = extractor.extract_symbols(None);
    let identifiers = extractor.extract_identifiers(&symbols);

    let do_once_calls: Vec<_> = identifiers
        .iter()
        .filter(|id| id.name == "doOnce" && id.kind == IdentifierKind::Call)
        .collect();

    assert_eq!(
        do_once_calls.len(),
        1,
        "Script section must be walked exactly once: expected 1 'doOnce' Call identifier, \
         got {}. A count of 2 indicates double-walking; 0 means the call was not captured.",
        do_once_calls.len()
    );
}

#[test]
fn test_vue_identifier_offsets_use_the_current_script_section() {
    let vue_code = r#"<template>
  <div />
</template>

<script>
function boot() {
  bootCall()
}
</script>

<script setup>
function bump() {
  helper()
}
</script>"#;

    let mut extractor = create_extractor("multi-script.vue", vue_code);
    let symbols = extractor.extract_symbols(None);
    let identifiers = extractor.extract_identifiers(&symbols);

    let bump = symbols
        .iter()
        .find(|symbol| symbol.name == "bump" && symbol.kind == SymbolKind::Function)
        .expect("script setup function should be extracted");
    let helper = identifiers
        .iter()
        .find(|identifier| identifier.name == "helper" && identifier.kind == IdentifierKind::Call)
        .expect("script setup helper() call should be extracted");

    let helper_offset = vue_code.find("helper()").unwrap() as u32;
    let (helper_line, helper_column) = line_column_for_byte(vue_code, helper_offset as usize);
    assert_eq!(helper.start_byte, helper_offset);
    assert_eq!(helper.start_line, helper_line);
    assert_eq!(helper.start_column, helper_column);
    assert_eq!(helper.id, expected_identifier_id(helper));
    assert_eq!(
        helper.containing_symbol_id.as_deref(),
        Some(bump.id.as_str()),
        "identifier containment must be computed after section offsets are applied"
    );
}
