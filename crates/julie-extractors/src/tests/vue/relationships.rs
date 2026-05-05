use crate::base::RelationshipKind;
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
fn vue_relationships_resolve_script_and_template_refs() {
    let vue_code = r#"
<template>
  <section class="worker">
    <h1>{{ title }}</h1>
  </section>
</template>

<script setup lang="ts">
const title = format("Worker");

function format(value: string): string {
  return value.trim();
}
</script>

<style scoped>
.worker {
  color: #0f766e;
}
</style>
        "#;

    let mut extractor = create_extractor("source.vue", vue_code);
    let symbols = extractor.extract_symbols(None);
    let relationships = extractor.extract_relationships(None, &symbols);

    let title = symbols
        .iter()
        .find(|symbol| symbol.name == "title")
        .expect("script setup const should be extracted");
    let format = symbols
        .iter()
        .find(|symbol| symbol.name == "format")
        .expect("script setup function should be extracted");

    let title_reference = relationships
        .iter()
        .find(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.to_symbol_id == title.id
        })
        .expect("template interpolation should reference the local title symbol");
    assert_eq!(
        title_reference.line_number, 4,
        "template reference line should point at the interpolation line"
    );

    let format_call = relationships
        .iter()
        .find(|relationship| {
            relationship.kind == RelationshipKind::Calls && relationship.to_symbol_id == format.id
        })
        .expect("script setup call should resolve to the local format symbol");
    assert_eq!(
        format_call.line_number, 9,
        "script call line should point at the call expression line, got: {:?}",
        relationships
    );
}

#[test]
fn vue_relationships_record_external_component_pending_refs() {
    let vue_code = r#"
<template>
  <HeaderBar />
</template>

<script setup lang="ts">
const title = "Worker";
</script>
        "#;

    let mut extractor = create_extractor("source.vue", vue_code);
    let symbols = extractor.extract_symbols(None);
    let pending = extractor.extract_structured_pending_relationships(&symbols);

    assert!(
        pending.iter().any(|relationship| {
            relationship.target.display_name == "HeaderBar"
                && relationship.target.terminal_name == "HeaderBar"
        }),
        "external component tag should remain a structured pending reference, got: {:?}",
        pending
    );
}
