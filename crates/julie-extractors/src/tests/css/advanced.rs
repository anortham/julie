use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_complex_selectors_combinators_and_pseudo_elements() {
        let css_code = r#"
/* Complex combinators */
.parent > .direct-child {
  color: red;
}

.ancestor .descendant {
  color: blue;
}

.sibling + .adjacent {
  margin-left: 1rem;
}

.element ~ .general-sibling {
  opacity: 0.8;
}

/* Advanced pseudo-classes */
.item:first-child {
  margin-top: 0;
}

.item:last-child {
  margin-bottom: 0;
}

.item:nth-child(odd) {
  background-color: #f8f9fa;
}

.item:nth-child(even) {
  background-color: #ffffff;
}

.item:nth-child(3n + 1) {
  border-left: 3px solid #3498db;
}

.item:nth-last-child(2) {
  margin-bottom: 2rem;
}

.form-group:has(input:invalid) {
  border-color: #e74c3c;
}

.card:has(> .featured) {
  border: 2px solid gold;
}

.container:not(.disabled):not(.loading) {
  opacity: 1;
}

.input:is(.text, .email, .password) {
  border: 1px solid #ddd;
}

.button:where(.primary, .secondary) {
  padding: 0.75rem 1rem;
}

/* Pseudo-elements */
.element::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 2px;
  background-color: #3498db;
}

.element::after {
  content: attr(data-label);
  position: absolute;
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%);
  background-color: #2c3e50;
  color: white;
  padding: 0.5rem;
  border-radius: 4px;
  font-size: 0.875rem;
  white-space: nowrap;
}

.quote::before {
  content: '"';
  font-size: 2rem;
  color: #3498db;
  line-height: 1;
}

.quote::after {
  content: '"';
  font-size: 2rem;
  color: #3498db;
  line-height: 1;
}

.list-item::marker {
  color: #e74c3c;
  font-weight: bold;
}

.input::placeholder {
  color: #95a5a6;
  font-style: italic;
}

.selection::selection {
  background-color: #3498db;
  color: white;
}

.text::first-line {
  font-weight: bold;
  text-transform: uppercase;
}

.paragraph::first-letter {
  font-size: 3rem;
  float: left;
  line-height: 1;
  margin-right: 0.5rem;
  margin-top: 0.25rem;
}

/* Form pseudo-classes */
input:focus {
  outline: none;
  border-color: #3498db;
  box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
}

input:valid {
  border-color: #27ae60;
}

input:invalid {
  border-color: #e74c3c;
}

input:required {
  background-image: url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><circle cx="4" cy="4" r="2" fill="%23e74c3c"/></svg>');
  background-position: right 0.5rem center;
  background-repeat: no-repeat;
}

input:optional {
  background-image: none;
}

input:checked + label {
  font-weight: bold;
  color: #27ae60;
}

input:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

input:read-only {
  background-color: #f8f9fa;
}

/* Link pseudo-classes */
a:link {
  color: #3498db;
}

a:visited {
  color: #8e44ad;
}

a:hover {
  color: #2980b9;
  text-decoration: underline;
}

a:active {
  color: #e74c3c;
}

a:focus {
  outline: 2px solid #3498db;
  outline-offset: 2px;
}

/* Target pseudo-class */
.section:target {
  background-color: #fff3cd;
  border-left: 4px solid #ffc107;
  padding-left: 1rem;
}

/* Structural pseudo-classes */
.grid-item:nth-of-type(3n) {
  grid-column: span 2;
}

.heading:only-child {
  margin: 0;
}

.list:empty::before {
  content: 'No items to display';
  color: #6c757d;
  font-style: italic;
}

/* Language and direction pseudo-classes */
.content:lang(en) {
  quotes: '"' '"' "'" "'";
}

.content:lang(fr) {
  quotes: '«' '»' '"' '"';
}

.text:dir(rtl) {
  text-align: right;
}

.text:dir(ltr) {
  text-align: left;
}

/* Complex nested selectors */
.header .navigation ul li a:hover::after {
  content: '';
  position: absolute;
  bottom: -5px;
  left: 0;
  width: 100%;
  height: 2px;
  background-color: currentColor;
}

.card:has(.image) .content:not(.minimal) h2:first-of-type::before {
  content: '🖼 ';
  margin-right: 0.5rem;
}

/* Media query with complex selectors */
@media (max-width: 767px) {
  .mobile-nav:target ~ .overlay {
    display: block;
  }

  .menu-toggle:checked + .menu {
    transform: translateX(0);
  }

  .responsive-table th:nth-child(n+3) {
    display: none;
  }
}

/* Custom pseudo-classes (WebKit specific) */
input::-webkit-outer-spin-button,
input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

input[type="search"]::-webkit-search-cancel-button {
  -webkit-appearance: none;
  height: 1em;
  width: 1em;
  background: url('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/></svg>') center/contain no-repeat;
}

::-webkit-scrollbar {
  width: 8px;
}

::-webkit-scrollbar-track {
  background: #f1f1f1;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb {
  background: #c1c1c1;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #a8a8a8;
}
"#;

        let symbols = extract_symbols(css_code);

        // Combinator selectors
        let direct_child = symbols.iter().find(|s| s.name == ".parent > .direct-child");
        assert!(direct_child.is_some());
        assert!(
            direct_child
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("color: red")
        );

        let descendant = symbols.iter().find(|s| s.name == ".ancestor .descendant");
        assert!(descendant.is_some());

        let adjacent = symbols.iter().find(|s| s.name == ".sibling + .adjacent");
        assert!(adjacent.is_some());

        let general_sibling = symbols
            .iter()
            .find(|s| s.name == ".element ~ .general-sibling");
        assert!(general_sibling.is_some());

        // Structural pseudo-classes
        let first_child = symbols.iter().find(|s| s.name == ".item:first-child");
        assert!(first_child.is_some());

        let last_child = symbols.iter().find(|s| s.name == ".item:last-child");
        assert!(last_child.is_some());

        let nth_child_odd = symbols.iter().find(|s| s.name == ".item:nth-child(odd)");
        assert!(nth_child_odd.is_some());

        let nth_child_formula = symbols.iter().find(|s| s.name == ".item:nth-child(3n + 1)");
        assert!(nth_child_formula.is_some());

        let nth_last_child = symbols.iter().find(|s| s.name == ".item:nth-last-child(2)");
        assert!(nth_last_child.is_some());

        // Modern pseudo-classes
        let has_invalid = symbols
            .iter()
            .find(|s| s.name == ".form-group:has(input:invalid)");
        assert!(has_invalid.is_some());

        let has_featured = symbols.iter().find(|s| s.name == ".card:has(> .featured)");
        assert!(has_featured.is_some());

        let not_disabled = symbols
            .iter()
            .find(|s| s.name == ".container:not(.disabled):not(.loading)");
        assert!(not_disabled.is_some());

        let is_inputs = symbols
            .iter()
            .find(|s| s.name == ".input:is(.text, .email, .password)");
        assert!(is_inputs.is_some());

        let where_buttons = symbols
            .iter()
            .find(|s| s.name == ".button:where(.primary, .secondary)");
        assert!(where_buttons.is_some());

        // Pseudo-elements
        let before_element = symbols.iter().find(|s| s.name == ".element::before");
        assert!(before_element.is_some());
        assert!(
            before_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("content: ''")
        );
        assert!(
            before_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("position: absolute")
        );

        let after_element = symbols.iter().find(|s| s.name == ".element::after");
        assert!(after_element.is_some());
        assert!(
            after_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("content: attr(data-label)")
        );

        let quote_before = symbols.iter().find(|s| s.name == ".quote::before");
        assert!(quote_before.is_some());

        let quote_after = symbols.iter().find(|s| s.name == ".quote::after");
        assert!(quote_after.is_some());

        let marker = symbols.iter().find(|s| s.name == ".list-item::marker");
        assert!(marker.is_some());

        let placeholder = symbols.iter().find(|s| s.name == ".input::placeholder");
        assert!(placeholder.is_some());

        let selection = symbols.iter().find(|s| s.name == ".selection::selection");
        assert!(selection.is_some());

        let first_line = symbols.iter().find(|s| s.name == ".text::first-line");
        assert!(first_line.is_some());

        let first_letter = symbols
            .iter()
            .find(|s| s.name == ".paragraph::first-letter");
        assert!(first_letter.is_some());

        // Form pseudo-classes
        let input_focus = symbols.iter().find(|s| s.name == "input:focus");
        assert!(input_focus.is_some());

        let input_valid = symbols.iter().find(|s| s.name == "input:valid");
        assert!(input_valid.is_some());

        let input_invalid = symbols.iter().find(|s| s.name == "input:invalid");
        assert!(input_invalid.is_some());

        let input_required = symbols.iter().find(|s| s.name == "input:required");
        assert!(input_required.is_some());

        let input_checked = symbols.iter().find(|s| s.name == "input:checked + label");
        assert!(input_checked.is_some());

        let input_disabled = symbols.iter().find(|s| s.name == "input:disabled");
        assert!(input_disabled.is_some());

        // Link pseudo-classes
        let link_hover = symbols.iter().find(|s| s.name == "a:hover");
        assert!(link_hover.is_some());

        let link_visited = symbols.iter().find(|s| s.name == "a:visited");
        assert!(link_visited.is_some());

        let link_active = symbols.iter().find(|s| s.name == "a:active");
        assert!(link_active.is_some());

        // Target pseudo-class
        let section_target = symbols.iter().find(|s| s.name == ".section:target");
        assert!(section_target.is_some());

        // Structural selectors
        let nth_of_type = symbols
            .iter()
            .find(|s| s.name == ".grid-item:nth-of-type(3n)");
        assert!(nth_of_type.is_some());

        let only_child = symbols.iter().find(|s| s.name == ".heading:only-child");
        assert!(only_child.is_some());

        let empty_before = symbols.iter().find(|s| s.name == ".list:empty::before");
        assert!(empty_before.is_some());

        // Language pseudo-classes
        let lang_en = symbols.iter().find(|s| s.name == ".content:lang(en)");
        assert!(lang_en.is_some());

        let lang_fr = symbols.iter().find(|s| s.name == ".content:lang(fr)");
        assert!(lang_fr.is_some());

        // Direction pseudo-classes
        let dir_rtl = symbols.iter().find(|s| s.name == ".text:dir(rtl)");
        assert!(dir_rtl.is_some());

        let dir_ltr = symbols.iter().find(|s| s.name == ".text:dir(ltr)");
        assert!(dir_ltr.is_some());

        // Complex nested selectors
        let complex_nested = symbols
            .iter()
            .find(|s| s.name == ".header .navigation ul li a:hover::after");
        assert!(complex_nested.is_some());

        let super_complex = symbols.iter().find(|s| {
            s.name
                .contains(".card:has(.image) .content:not(.minimal) h2:first-of-type::before")
        });
        assert!(super_complex.is_some());

        // WebKit pseudo-elements
        let webkit_scrollbar = symbols.iter().find(|s| s.name == "::-webkit-scrollbar");
        assert!(webkit_scrollbar.is_some());

        let webkit_scrollbar_thumb = symbols
            .iter()
            .find(|s| s.name == "::-webkit-scrollbar-thumb");
        assert!(webkit_scrollbar_thumb.is_some());

        let webkit_scrollbar_thumb_hover = symbols
            .iter()
            .find(|s| s.name == "::-webkit-scrollbar-thumb:hover");
        assert!(webkit_scrollbar_thumb_hover.is_some());
    }
}
