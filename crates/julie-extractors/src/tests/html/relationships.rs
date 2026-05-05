use super::extract_symbols_and_relationships;
use crate::base::RelationshipKind;

#[test]
fn test_html_relationships_walk_past_doctype() {
    let code = r#"<!doctype html>
<html>
  <body>
    <a href="/workers">Workers</a>
  </body>
</html>"#;

    let (symbols, relationships) = extract_symbols_and_relationships(code);

    assert!(
        symbols.iter().any(|symbol| symbol.name == "a"),
        "anchor symbol should be extracted"
    );
    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.to_symbol_id == "url:/workers"
        }),
        "href relationship should be extracted even when the document starts with a doctype"
    );
}
