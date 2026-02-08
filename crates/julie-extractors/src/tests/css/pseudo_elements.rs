use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pseudo_elements_and_pseudo_classes() {
        let css_code = r#"
/* Pseudo-elements */
.element::before {
  content: "→";
  color: #007bff;
}

.element::after {
  content: "←";
  color: #dc3545;
}

.quote::before {
  content: open-quote;
}

.quote::after {
  content: close-quote;
}

.tooltip::before {
  content: attr(data-tooltip);
  position: absolute;
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%);
}

/* Pseudo-classes */
.link:link {
  color: #007bff;
  text-decoration: none;
}

.link:visited {
  color: #6c757d;
}

.button:hover {
  background-color: #007bff;
  transform: translateY(-2px);
}

.button:active {
  transform: translateY(0);
  box-shadow: inset 0 2px 4px rgba(0,0,0,0.2);
}

.input:focus {
  outline: none;
  border-color: #007bff;
  box-shadow: 0 0 0 2px rgba(0,123,255,0.25);
}

.input:valid {
  border-color: #28a745;
}

.input:invalid {
  border-color: #dc3545;
}

/* Structural pseudo-classes */
.list-item:nth-child(odd) {
  background-color: #f8f9fa;
}

.list-item:nth-child(even) {
  background-color: #ffffff;
}

.list-item:first-child {
  border-top: none;
}

.list-item:last-child {
  border-bottom: none;
}

.article p:nth-of-type(1) {
  font-weight: bold;
  font-size: 1.2em;
}

/* Form pseudo-classes */
.checkbox:checked + .checkmark {
  background-color: #007bff;
}

.radio:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Language pseudo-class */
.quote:lang(fr)::before {
  content: "« ";
}

.quote:lang(fr)::after {
  content: " »";
}

/* Target pseudo-class */
.section:target {
  background-color: #fff3cd;
  border: 1px solid #ffeaa7;
}

/* Not pseudo-class */
.input:not(:focus):not(:valid) {
  border-color: #6c757d;
}
"#;

        let symbols = extract_symbols(css_code);

        // Test pseudo-elements
        let element_before = symbols.iter().find(|s| s.name == ".element::before");
        assert!(element_before.is_some());
        assert_eq!(element_before.unwrap().kind, SymbolKind::Property);

        let quote_before = symbols.iter().find(|s| s.name == ".quote::before");
        assert!(quote_before.is_some());

        let tooltip_before = symbols.iter().find(|s| s.name == ".tooltip::before");
        assert!(tooltip_before.is_some());

        // Test pseudo-classes
        let link = symbols.iter().find(|s| s.name == ".link:link");
        assert!(link.is_some());

        let button_hover = symbols.iter().find(|s| s.name == ".button:hover");
        assert!(button_hover.is_some());

        let input_focus = symbols.iter().find(|s| s.name == ".input:focus");
        assert!(input_focus.is_some());

        let list_item_odd = symbols
            .iter()
            .find(|s| s.name == ".list-item:nth-child(odd)");
        assert!(list_item_odd.is_some());

        let article_nth = symbols
            .iter()
            .find(|s| s.name == ".article p:nth-of-type(1)");
        assert!(article_nth.is_some());

        let checkbox_checked = symbols
            .iter()
            .find(|s| s.name == ".checkbox:checked + .checkmark");
        assert!(checkbox_checked.is_some());

        let radio_disabled = symbols.iter().find(|s| s.name == ".radio:disabled");
        assert!(radio_disabled.is_some());

        let section_target = symbols.iter().find(|s| s.name == ".section:target");
        assert!(section_target.is_some());
    }
}
