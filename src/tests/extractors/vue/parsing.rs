// Inline tests extracted from extractors/vue/parsing.rs
//
// These tests validate Vue SFC (Single File Component) parsing functionality,
// including section detection, language attribute extraction, and handling of unclosed tags.

use crate::extractors::vue::parsing::parse_vue_sfc;

#[test]
fn test_parse_vue_sfc_basic() {
    let content = r#"<template>
  <div>Hello</div>
</template>

<script>
export default {
  name: 'App'
}
</script>

<style>
div { color: blue; }
</style>"#;

    let sections = parse_vue_sfc(content).unwrap();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].section_type, "template");
    assert_eq!(sections[1].section_type, "script");
    assert_eq!(sections[2].section_type, "style");
}

#[test]
fn test_parse_vue_sfc_with_lang_attributes() {
    let content = r#"<template lang="html">
  <div>Content</div>
</template>

<script lang="ts">
export default {}
</script>

<style lang="scss">
$color: blue;
</style>"#;

    let sections = parse_vue_sfc(content).unwrap();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].lang.as_deref(), Some("html"));
    assert_eq!(sections[1].lang.as_deref(), Some("ts"));
    assert_eq!(sections[2].lang.as_deref(), Some("scss"));
}

#[test]
fn test_parse_vue_sfc_without_closing_tags() {
    let content = r#"<template>
  <div>Hello</div>

<script>
export default {
  name: 'App'
}"#;

    let sections = parse_vue_sfc(content).unwrap();
    assert!(sections.len() >= 2);
    assert_eq!(sections[0].section_type, "template");
    assert_eq!(sections[1].section_type, "script");
}
