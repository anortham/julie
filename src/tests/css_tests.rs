// CSS Extractor Tests
//
// Direct port of Miller's CSS extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/css-extractor.test.ts
//
// Test suites:
// 1. Basic CSS Selectors and Rules
// 2. Modern CSS Features and Layout Systems
// 3. CSS Custom Properties and Functions
// 4. CSS At-Rules and Media Queries
// 5. Advanced CSS Selectors and Pseudo-elements

use crate::extractors::base::{Symbol, SymbolKind};
use crate::extractors::css::CSSExtractor;
use tree_sitter::Parser;

/// Initialize CSS parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_css::LANGUAGE.into()).expect("Error loading CSS grammar");
    parser
}

#[cfg(test)]
mod css_extractor_tests {
    use super::*;

    mod basic_css_selectors_and_rules {
        use super::*;

        #[test]
        fn test_extract_basic_selectors_properties_and_values() {
            let css_code = r#"
/* Global styles */
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

/* Element selectors */
body {
  font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
  line-height: 1.6;
  color: #333;
  background-color: #ffffff;
}

h1, h2, h3, h4, h5, h6 {
  font-weight: bold;
  margin-bottom: 1rem;
  color: #2c3e50;
}

a {
  color: #3498db;
  text-decoration: none;
  transition: color 0.3s ease;
}

a:hover {
  color: #2980b9;
  text-decoration: underline;
}

a:visited {
  color: #8e44ad;
}

a:active {
  color: #e74c3c;
}

/* Class selectors */
.container {
  max-width: 1200px;
  margin: 0 auto;
  padding: 0 2rem;
}

.header {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  padding: 2rem 0;
  text-align: center;
}

.navigation {
  background-color: rgba(255, 255, 255, 0.95);
  backdrop-filter: blur(10px);
  position: sticky;
  top: 0;
  z-index: 1000;
}

.btn {
  display: inline-block;
  padding: 0.75rem 1.5rem;
  border: none;
  border-radius: 4px;
  font-size: 1rem;
  cursor: pointer;
  transition: all 0.3s ease;
}

.btn-primary {
  background-color: #3498db;
  color: white;
}

.btn-primary:hover {
  background-color: #2980b9;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(52, 152, 219, 0.3);
}

.btn-secondary {
  background-color: #95a5a6;
  color: white;
}

/* ID selectors */
#main-content {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

#footer {
  background-color: #34495e;
  color: #ecf0f1;
  text-align: center;
  padding: 2rem 0;
  margin-top: auto;
}

/* Attribute selectors */
[data-theme="dark"] {
  background-color: #2c3e50;
  color: #ecf0f1;
}

[data-component="modal"] {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background-color: rgba(0, 0, 0, 0.8);
  display: flex;
  align-items: center;
  justify-content: center;
}

input[type="text"],
input[type="email"],
input[type="password"] {
  width: 100%;
  padding: 0.75rem;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 1rem;
}

input[type="text"]:focus,
input[type="email"]:focus,
input[type="password"]:focus {
  outline: none;
  border-color: #3498db;
  box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.1);
}

button[disabled] {
  opacity: 0.6;
  cursor: not-allowed;
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(css_code, None).unwrap();

            let mut extractor = CSSExtractor::new(
                "css".to_string(),
                "basic.css".to_string(),
                css_code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Universal selector
            let universal_selector = symbols.iter().find(|s| s.name == "*");
            assert!(universal_selector.is_some());
            assert_eq!(universal_selector.unwrap().kind, SymbolKind::Variable); // CSS rules as variables

            // Element selectors
            let body_selector = symbols.iter().find(|s| s.name == "body");
            assert!(body_selector.is_some());
            assert!(body_selector.unwrap().signature.as_ref().unwrap().contains("font-family"));

            let heading_selectors = symbols.iter().find(|s| s.name == "h1, h2, h3, h4, h5, h6");
            assert!(heading_selectors.is_some());
            assert!(heading_selectors.unwrap().signature.as_ref().unwrap().contains("font-weight: bold"));

            // Anchor pseudo-classes
            let anchor_hover = symbols.iter().find(|s| s.name == "a:hover");
            assert!(anchor_hover.is_some());
            assert!(anchor_hover.unwrap().signature.as_ref().unwrap().contains("text-decoration: underline"));

            let anchor_visited = symbols.iter().find(|s| s.name == "a:visited");
            assert!(anchor_visited.is_some());

            let anchor_active = symbols.iter().find(|s| s.name == "a:active");
            assert!(anchor_active.is_some());

            // Class selectors
            let container_class = symbols.iter().find(|s| s.name == ".container");
            assert!(container_class.is_some());
            assert!(container_class.unwrap().signature.as_ref().unwrap().contains("max-width: 1200px"));

            let header_class = symbols.iter().find(|s| s.name == ".header");
            assert!(header_class.is_some());
            assert!(header_class.unwrap().signature.as_ref().unwrap().contains("linear-gradient"));

            let navigation_class = symbols.iter().find(|s| s.name == ".navigation");
            assert!(navigation_class.is_some());
            assert!(navigation_class.unwrap().signature.as_ref().unwrap().contains("backdrop-filter: blur(10px)"));

            // Button classes
            let btn_class = symbols.iter().find(|s| s.name == ".btn");
            assert!(btn_class.is_some());
            assert!(btn_class.unwrap().signature.as_ref().unwrap().contains("display: inline-block"));

            let btn_primary = symbols.iter().find(|s| s.name == ".btn-primary");
            assert!(btn_primary.is_some());

            let btn_primary_hover = symbols.iter().find(|s| s.name == ".btn-primary:hover");
            assert!(btn_primary_hover.is_some());
            assert!(btn_primary_hover.unwrap().signature.as_ref().unwrap().contains("transform: translateY(-2px)"));

            // ID selectors
            let main_content = symbols.iter().find(|s| s.name == "#main-content");
            assert!(main_content.is_some());
            assert!(main_content.unwrap().signature.as_ref().unwrap().contains("display: flex"));

            let footer = symbols.iter().find(|s| s.name == "#footer");
            assert!(footer.is_some());
            assert!(footer.unwrap().signature.as_ref().unwrap().contains("margin-top: auto"));

            // Attribute selectors
            let dark_theme = symbols.iter().find(|s| s.name == "[data-theme=\"dark\"]");
            assert!(dark_theme.is_some());
            assert!(dark_theme.unwrap().signature.as_ref().unwrap().contains("background-color: #2c3e50"));

            let modal_component = symbols.iter().find(|s| s.name == "[data-component=\"modal\"]");
            assert!(modal_component.is_some());
            assert!(modal_component.unwrap().signature.as_ref().unwrap().contains("position: fixed"));

            // Input type selectors
            let text_inputs = symbols.iter().find(|s| s.name.contains("input[type=\"text\"]"));
            assert!(text_inputs.is_some());
            assert!(text_inputs.unwrap().signature.as_ref().unwrap().contains("width: 100%"));

            let input_focus = symbols.iter().find(|s| s.name.contains("input[type=\"text\"]:focus"));
            assert!(input_focus.is_some());
            assert!(input_focus.unwrap().signature.as_ref().unwrap().contains("box-shadow"));

            let disabled_button = symbols.iter().find(|s| s.name == "button[disabled]");
            assert!(disabled_button.is_some());
            assert!(disabled_button.unwrap().signature.as_ref().unwrap().contains("cursor: not-allowed"));
        }
    }

    mod modern_css_features_and_layout_systems {
        use super::*;

        #[test]
        fn test_extract_css_grid_flexbox_and_modern_layout_properties() {
            let css_code = r#"
/* CSS Grid Layout */
.grid-container {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  grid-template-rows: auto 1fr auto;
  grid-template-areas:
    "header header header"
    "sidebar main aside"
    "footer footer footer";
  gap: 2rem;
  min-height: 100vh;
}

.grid-header {
  grid-area: header;
  background-color: #3498db;
  padding: 2rem;
}

.grid-sidebar {
  grid-area: sidebar;
  background-color: #ecf0f1;
  padding: 1rem;
}

.grid-main {
  grid-area: main;
  padding: 2rem;
}

.grid-aside {
  grid-area: aside;
  background-color: #f8f9fa;
  padding: 1rem;
}

.grid-footer {
  grid-area: footer;
  background-color: #2c3e50;
  color: white;
  padding: 2rem;
  text-align: center;
}

/* Advanced Grid */
.photo-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  grid-auto-rows: 200px;
  grid-auto-flow: dense;
  gap: 1rem;
}

.photo-item:nth-child(3n) {
  grid-column: span 2;
  grid-row: span 2;
}

.photo-item:nth-child(5n) {
  grid-column: span 1;
  grid-row: span 3;
}

/* CSS Flexbox */
.flex-container {
  display: flex;
  flex-direction: row;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  align-content: flex-start;
  gap: 1rem;
}

.flex-item {
  flex: 1 1 auto;
  min-width: 0;
}

.flex-item-grow {
  flex-grow: 2;
  flex-shrink: 1;
  flex-basis: 200px;
}

.flex-item-fixed {
  flex: 0 0 150px;
}

/* Flexbox Navigation */
.nav-flex {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem 2rem;
}

.nav-flex .logo {
  flex: 0 0 auto;
}

.nav-flex .menu {
  display: flex;
  list-style: none;
  margin: 0;
  padding: 0;
  gap: 2rem;
}

.nav-flex .actions {
  display: flex;
  gap: 1rem;
  margin-left: auto;
}

/* Modern Positioning */
.sticky-header {
  position: sticky;
  top: 0;
  z-index: 100;
  background-color: rgba(255, 255, 255, 0.95);
  backdrop-filter: blur(10px);
}

.fixed-sidebar {
  position: fixed;
  top: 80px;
  left: 0;
  width: 250px;
  height: calc(100vh - 80px);
  overflow-y: auto;
  background-color: #f8f9fa;
  border-right: 1px solid #dee2e6;
}

.absolute-overlay {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background-color: white;
  padding: 2rem;
  border-radius: 8px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
}

/* Subgrid (newer CSS feature) */
.subgrid-container {
  display: grid;
  grid-template-columns: 1fr 2fr 1fr;
  gap: 1rem;
}

.subgrid-item {
  display: grid;
  grid-template-columns: subgrid;
  grid-column: 1 / -1;
  gap: inherit;
}

/* Container Queries */
.card-container {
  container-type: inline-size;
  container-name: card;
}

@container card (min-width: 400px) {
  .card-content {
    display: flex;
    gap: 1rem;
  }

  .card-image {
    flex: 0 0 150px;
  }
}

@container card (min-width: 600px) {
  .card-content {
    flex-direction: column;
  }

  .card-image {
    flex: none;
    width: 100%;
    height: 200px;
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(css_code, None).unwrap();

            let mut extractor = CSSExtractor::new(
                "css".to_string(),
                "layout.css".to_string(),
                css_code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // CSS Grid container
            let grid_container = symbols.iter().find(|s| s.name == ".grid-container");
            assert!(grid_container.is_some());
            assert!(grid_container.unwrap().signature.as_ref().unwrap().contains("display: grid"));
            assert!(grid_container.unwrap().signature.as_ref().unwrap().contains("grid-template-columns: repeat(auto-fit, minmax(300px, 1fr))"));
            assert!(grid_container.unwrap().signature.as_ref().unwrap().contains("grid-template-areas"));

            // Grid areas
            let grid_header = symbols.iter().find(|s| s.name == ".grid-header");
            assert!(grid_header.is_some());
            assert!(grid_header.unwrap().signature.as_ref().unwrap().contains("grid-area: header"));

            let grid_sidebar = symbols.iter().find(|s| s.name == ".grid-sidebar");
            assert!(grid_sidebar.is_some());
            assert!(grid_sidebar.unwrap().signature.as_ref().unwrap().contains("grid-area: sidebar"));

            let grid_main = symbols.iter().find(|s| s.name == ".grid-main");
            assert!(grid_main.is_some());
            assert!(grid_main.unwrap().signature.as_ref().unwrap().contains("grid-area: main"));

            // Advanced grid
            let photo_grid = symbols.iter().find(|s| s.name == ".photo-grid");
            assert!(photo_grid.is_some());
            assert!(photo_grid.unwrap().signature.as_ref().unwrap().contains("grid-auto-flow: dense"));

            let photo_item_nth = symbols.iter().find(|s| s.name == ".photo-item:nth-child(3n)");
            assert!(photo_item_nth.is_some());
            assert!(photo_item_nth.unwrap().signature.as_ref().unwrap().contains("grid-column: span 2"));

            // Flexbox container
            let flex_container = symbols.iter().find(|s| s.name == ".flex-container");
            assert!(flex_container.is_some());
            assert!(flex_container.unwrap().signature.as_ref().unwrap().contains("display: flex"));
            assert!(flex_container.unwrap().signature.as_ref().unwrap().contains("justify-content: space-between"));
            assert!(flex_container.unwrap().signature.as_ref().unwrap().contains("align-items: center"));

            // Flex items
            let flex_item = symbols.iter().find(|s| s.name == ".flex-item");
            assert!(flex_item.is_some());
            assert!(flex_item.unwrap().signature.as_ref().unwrap().contains("flex: 1 1 auto"));

            let flex_item_grow = symbols.iter().find(|s| s.name == ".flex-item-grow");
            assert!(flex_item_grow.is_some());
            assert!(flex_item_grow.unwrap().signature.as_ref().unwrap().contains("flex-grow: 2"));

            let flex_item_fixed = symbols.iter().find(|s| s.name == ".flex-item-fixed");
            assert!(flex_item_fixed.is_some());
            assert!(flex_item_fixed.unwrap().signature.as_ref().unwrap().contains("flex: 0 0 150px"));

            // Navigation flex
            let nav_flex = symbols.iter().find(|s| s.name == ".nav-flex");
            assert!(nav_flex.is_some());

            let nav_flex_logo = symbols.iter().find(|s| s.name == ".nav-flex .logo");
            assert!(nav_flex_logo.is_some());

            let nav_flex_menu = symbols.iter().find(|s| s.name == ".nav-flex .menu");
            assert!(nav_flex_menu.is_some());

            // Modern positioning
            let sticky_header = symbols.iter().find(|s| s.name == ".sticky-header");
            assert!(sticky_header.is_some());
            assert!(sticky_header.unwrap().signature.as_ref().unwrap().contains("position: sticky"));
            assert!(sticky_header.unwrap().signature.as_ref().unwrap().contains("backdrop-filter: blur(10px)"));

            let fixed_sidebar = symbols.iter().find(|s| s.name == ".fixed-sidebar");
            assert!(fixed_sidebar.is_some());
            assert!(fixed_sidebar.unwrap().signature.as_ref().unwrap().contains("position: fixed"));
            assert!(fixed_sidebar.unwrap().signature.as_ref().unwrap().contains("height: calc(100vh - 80px)"));

            let absolute_overlay = symbols.iter().find(|s| s.name == ".absolute-overlay");
            assert!(absolute_overlay.is_some());
            assert!(absolute_overlay.unwrap().signature.as_ref().unwrap().contains("transform: translate(-50%, -50%)"));

            // Subgrid
            let subgrid_container = symbols.iter().find(|s| s.name == ".subgrid-container");
            assert!(subgrid_container.is_some());

            let subgrid_item = symbols.iter().find(|s| s.name == ".subgrid-item");
            assert!(subgrid_item.is_some());
            assert!(subgrid_item.unwrap().signature.as_ref().unwrap().contains("grid-template-columns: subgrid"));

            // Container queries
            let card_container = symbols.iter().find(|s| s.name == ".card-container");
            assert!(card_container.is_some());
            assert!(card_container.unwrap().signature.as_ref().unwrap().contains("container-type: inline-size"));

            // Container query rules
            let container_query_400 = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@container card (min-width: 400px)"));
            assert!(container_query_400.is_some());

            let container_query_600 = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@container card (min-width: 600px)"));
            assert!(container_query_600.is_some());
        }
    }

    mod css_custom_properties_and_functions {
        use super::*;

        #[test]
        fn test_extract_css_variables_calc_and_modern_css_functions() {
            let css_code = r#"
/* CSS Custom Properties (Variables) */
:root {
  /* Color palette */
  --primary-color: #3498db;
  --secondary-color: #2ecc71;
  --accent-color: #e74c3c;
  --background-color: #ffffff;
  --text-color: #2c3e50;
  --border-color: #bdc3c7;

  /* Typography */
  --font-family-primary: 'Inter', system-ui, sans-serif;
  --font-family-secondary: 'Fira Code', 'Courier New', monospace;
  --font-size-base: 1rem;
  --font-size-small: 0.875rem;
  --font-size-large: 1.125rem;
  --line-height-base: 1.6;

  /* Spacing */
  --spacing-xs: 0.25rem;
  --spacing-sm: 0.5rem;
  --spacing-md: 1rem;
  --spacing-lg: 1.5rem;
  --spacing-xl: 2rem;
  --spacing-2xl: 3rem;

  /* Layout */
  --container-max-width: 1200px;
  --sidebar-width: 250px;
  --header-height: 80px;
  --border-radius: 8px;

  /* Animations */
  --transition-fast: 0.15s ease;
  --transition-base: 0.3s ease;
  --transition-slow: 0.5s ease;

  /* Z-index scale */
  --z-dropdown: 1000;
  --z-sticky: 1020;
  --z-fixed: 1030;
  --z-modal: 1040;
  --z-tooltip: 1050;
}

/* Dark theme variables */
[data-theme="dark"] {
  --primary-color: #5dade2;
  --background-color: #2c3e50;
  --text-color: #ecf0f1;
  --border-color: #34495e;
}

/* Component using custom properties */
.button {
  background-color: var(--primary-color);
  color: var(--background-color);
  font-family: var(--font-family-primary);
  font-size: var(--font-size-base);
  padding: var(--spacing-sm) var(--spacing-md);
  border: 1px solid var(--border-color, transparent);
  border-radius: var(--border-radius);
  transition: var(--transition-base);
  cursor: pointer;
}

.button:hover {
  background-color: var(--primary-color, #3498db);
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(var(--primary-color), 0.3);
}

/* CSS calc() and mathematical functions */
.layout-calc {
  width: calc(100% - var(--sidebar-width));
  height: calc(100vh - var(--header-height));
  margin-left: calc(var(--spacing-md) * 2);
  padding: calc(var(--spacing-base) + 10px);
}

.responsive-grid {
  grid-template-columns: repeat(auto-fit, minmax(calc(300px + 2rem), 1fr));
  gap: calc(var(--spacing-md) + var(--spacing-sm));
}

.complex-calc {
  width: calc((100vw - var(--sidebar-width)) / 3 - var(--spacing-lg));
  height: calc(100vh - var(--header-height) - var(--spacing-xl) * 2);
  font-size: calc(var(--font-size-base) + 0.5vw);
}

/* Modern CSS functions */
.modern-functions {
  /* Clamp for fluid typography */
  font-size: clamp(1rem, 2.5vw, 2rem);

  /* Min/Max functions */
  width: min(90vw, var(--container-max-width));
  height: max(50vh, 400px);

  /* Color functions */
  background-color: hsl(var(--primary-hue, 200), 70%, 50%);
  color: oklch(0.7 0.15 200);

  /* New viewport units */
  min-height: 100svh; /* Small viewport height */
  padding-top: max(env(safe-area-inset-top), var(--spacing-md));
}

/* CSS comparison functions */
.comparison-functions {
  margin: max(var(--spacing-md), 2rem);
  padding: min(5%, var(--spacing-xl));
  width: clamp(300px, 50vw, 800px);
  height: clamp(200px, 30vh, 500px);
}

/* Trigonometric and exponential functions */
.advanced-math {
  /* CSS sin/cos/tan (experimental) */
  transform: rotate(calc(sin(var(--angle, 0)) * 45deg));

  /* Exponential functions */
  opacity: pow(0.8, var(--depth, 1));

  /* Square root */
  border-radius: calc(sqrt(var(--area, 100)) * 1px);
}

/* CSS logical properties with custom properties */
.logical-properties {
  margin-inline: var(--spacing-md);
  margin-block: var(--spacing-sm);
  padding-inline-start: var(--spacing-lg);
  border-inline-end: 1px solid var(--border-color);
  inset-inline-start: var(--sidebar-width);
}

/* CSS nesting with custom properties */
.nested-component {
  background-color: var(--background-color);
  border: 1px solid var(--border-color);

  & .header {
    background-color: var(--primary-color);
    padding: var(--spacing-sm);

    & h2 {
      color: var(--background-color);
      font-size: var(--font-size-large);
    }
  }

  & .content {
    padding: var(--spacing-md);

    & p {
      color: var(--text-color);
      line-height: var(--line-height-base);
    }
  }
}

/* CSS color-mix function */
.color-mixing {
  background-color: color-mix(in oklch, var(--primary-color), var(--secondary-color) 30%);
  border-color: color-mix(in srgb, var(--border-color), transparent 50%);
}

/* CSS container query units with custom properties */
.container-units {
  width: calc(50cqw - var(--spacing-md));
  height: calc(30cqh + var(--spacing-lg));
  font-size: calc(2cqi + var(--font-size-base));
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(css_code, None).unwrap();

            let mut extractor = CSSExtractor::new(
                "css".to_string(),
                "variables.css".to_string(),
                css_code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Root variables
            let root_selector = symbols.iter().find(|s| s.name == ":root");
            assert!(root_selector.is_some());
            assert!(root_selector.unwrap().signature.as_ref().unwrap().contains("--primary-color: #3498db"));
            assert!(root_selector.unwrap().signature.as_ref().unwrap().contains("--font-family-primary"));
            assert!(root_selector.unwrap().signature.as_ref().unwrap().contains("--spacing-xs: 0.25rem"));

            // Theme variables
            let dark_theme = symbols.iter().find(|s| s.name == "[data-theme=\"dark\"]");
            assert!(dark_theme.is_some());
            assert!(dark_theme.unwrap().signature.as_ref().unwrap().contains("--primary-color: #5dade2"));

            // Components using variables
            let button = symbols.iter().find(|s| s.name == ".button");
            assert!(button.is_some());
            assert!(button.unwrap().signature.as_ref().unwrap().contains("background-color: var(--primary-color)"));
            assert!(button.unwrap().signature.as_ref().unwrap().contains("padding: var(--spacing-sm) var(--spacing-md)"));

            let button_hover = symbols.iter().find(|s| s.name == ".button:hover");
            assert!(button_hover.is_some());
            assert!(button_hover.unwrap().signature.as_ref().unwrap().contains("var(--primary-color, #3498db)"));

            // Calc functions
            let layout_calc = symbols.iter().find(|s| s.name == ".layout-calc");
            assert!(layout_calc.is_some());
            assert!(layout_calc.unwrap().signature.as_ref().unwrap().contains("width: calc(100% - var(--sidebar-width))"));
            assert!(layout_calc.unwrap().signature.as_ref().unwrap().contains("height: calc(100vh - var(--header-height))"));

            let responsive_grid = symbols.iter().find(|s| s.name == ".responsive-grid");
            assert!(responsive_grid.is_some());
            assert!(responsive_grid.unwrap().signature.as_ref().unwrap().contains("calc(300px + 2rem)"));

            let complex_calc = symbols.iter().find(|s| s.name == ".complex-calc");
            assert!(complex_calc.is_some());
            assert!(complex_calc.unwrap().signature.as_ref().unwrap().contains("calc((100vw - var(--sidebar-width)) / 3"));

            // Modern CSS functions
            let modern_functions = symbols.iter().find(|s| s.name == ".modern-functions");
            assert!(modern_functions.is_some());
            assert!(modern_functions.unwrap().signature.as_ref().unwrap().contains("clamp(1rem, 2.5vw, 2rem)"));
            assert!(modern_functions.unwrap().signature.as_ref().unwrap().contains("min(90vw, var(--container-max-width))"));
            assert!(modern_functions.unwrap().signature.as_ref().unwrap().contains("max(50vh, 400px)"));
            assert!(modern_functions.unwrap().signature.as_ref().unwrap().contains("oklch(0.7 0.15 200)"));
            assert!(modern_functions.unwrap().signature.as_ref().unwrap().contains("100svh"));

            // Comparison functions
            let comparison_functions = symbols.iter().find(|s| s.name == ".comparison-functions");
            assert!(comparison_functions.is_some());
            assert!(comparison_functions.unwrap().signature.as_ref().unwrap().contains("max(var(--spacing-md), 2rem)"));
            assert!(comparison_functions.unwrap().signature.as_ref().unwrap().contains("clamp(300px, 50vw, 800px)"));

            // Advanced math functions
            let advanced_math = symbols.iter().find(|s| s.name == ".advanced-math");
            assert!(advanced_math.is_some());
            assert!(advanced_math.unwrap().signature.as_ref().unwrap().contains("sin(var(--angle, 0))"));
            assert!(advanced_math.unwrap().signature.as_ref().unwrap().contains("pow(0.8, var(--depth, 1))"));
            assert!(advanced_math.unwrap().signature.as_ref().unwrap().contains("sqrt(var(--area, 100))"));

            // Logical properties
            let logical_properties = symbols.iter().find(|s| s.name == ".logical-properties");
            assert!(logical_properties.is_some());
            assert!(logical_properties.unwrap().signature.as_ref().unwrap().contains("margin-inline: var(--spacing-md)"));
            assert!(logical_properties.unwrap().signature.as_ref().unwrap().contains("inset-inline-start: var(--sidebar-width)"));

            // CSS nesting
            let nested_component = symbols.iter().find(|s| s.name == ".nested-component");
            assert!(nested_component.is_some());

            let nested_header = symbols.iter().find(|s| s.name == "& .header" || s.name.contains(".nested-component .header"));
            assert!(nested_header.is_some());

            // Color mixing
            let color_mixing = symbols.iter().find(|s| s.name == ".color-mixing");
            assert!(color_mixing.is_some());
            assert!(color_mixing.unwrap().signature.as_ref().unwrap().contains("color-mix(in oklch"));

            // Container units
            let container_units = symbols.iter().find(|s| s.name == ".container-units");
            assert!(container_units.is_some());
            assert!(container_units.unwrap().signature.as_ref().unwrap().contains("50cqw"));
            assert!(container_units.unwrap().signature.as_ref().unwrap().contains("30cqh"));
            assert!(container_units.unwrap().signature.as_ref().unwrap().contains("2cqi"));
        }
    }

    mod css_at_rules_and_media_queries {
        use super::*;

        #[test]
        fn test_extract_media_keyframes_import_and_other_at_rules() {
            let css_code = r#"
/* CSS imports */
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap');
@import url('./normalize.css');
@import url('./components.css') layer(components);

/* CSS layers */
@layer reset, base, components, utilities;

@layer reset {
  * {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }
}

@layer base {
  body {
    font-family: 'Inter', system-ui, sans-serif;
    line-height: 1.6;
  }
}

/* Media queries */
@media (min-width: 768px) {
  .container {
    max-width: 750px;
  }

  .grid {
    grid-template-columns: repeat(2, 1fr);
  }

  .navigation {
    flex-direction: row;
  }
}

@media (min-width: 1024px) {
  .container {
    max-width: 980px;
  }

  .grid {
    grid-template-columns: repeat(3, 1fr);
  }

  .sidebar {
    display: block;
  }
}

@media (min-width: 1200px) {
  .container {
    max-width: 1140px;
  }

  .grid {
    grid-template-columns: repeat(4, 1fr);
  }
}

/* Feature queries */
@supports (display: grid) {
  .layout {
    display: grid;
    grid-template-columns: 1fr 3fr 1fr;
  }
}

@supports (backdrop-filter: blur(10px)) {
  .modal-backdrop {
    backdrop-filter: blur(10px);
    background-color: rgba(0, 0, 0, 0.5);
  }
}

@supports not (display: grid) {
  .layout {
    display: flex;
    flex-wrap: wrap;
  }

  .layout > * {
    flex: 1 1 300px;
  }
}

/* Keyframe animations */
@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes slideInFromLeft {
  0% {
    transform: translateX(-100%);
    opacity: 0;
  }
  50% {
    opacity: 0.5;
  }
  100% {
    transform: translateX(0);
    opacity: 1;
  }
}

@keyframes bounceIn {
  0% {
    transform: scale(0.3);
    opacity: 0;
  }
  50% {
    transform: scale(1.05);
    opacity: 0.8;
  }
  70% {
    transform: scale(0.9);
    opacity: 0.9;
  }
  100% {
    transform: scale(1);
    opacity: 1;
  }
}

@keyframes pulse {
  0%, 100% {
    transform: scale(1);
    opacity: 1;
  }
  50% {
    transform: scale(1.1);
    opacity: 0.7;
  }
}

/* Complex media queries */
@media screen and (min-width: 768px) and (max-width: 1023px) {
  .tablet-only {
    display: block;
  }
}

@media (orientation: landscape) and (max-height: 500px) {
  .landscape-short {
    height: 100vh;
    overflow-y: auto;
  }
}

@media (prefers-color-scheme: dark) {
  :root {
    --background-color: #1a1a1a;
    --text-color: #ffffff;
  }
}

@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}

@media (hover: hover) and (pointer: fine) {
  .hover-effects:hover {
    transform: scale(1.05);
    box-shadow: 0 10px 20px rgba(0, 0, 0, 0.2);
  }
}

/* Print styles */
@media print {
  .no-print {
    display: none !important;
  }

  .container {
    max-width: none;
    margin: 0;
    padding: 0;
  }

  body {
    font-size: 12pt;
    line-height: 1.4;
  }

  h1, h2, h3 {
    page-break-after: avoid;
  }

  img {
    max-width: 100% !important;
    height: auto !important;
  }
}

/* Font face declarations */
@font-face {
  font-family: 'CustomFont';
  src: url('./fonts/custom-font.woff2') format('woff2'),
       url('./fonts/custom-font.woff') format('woff');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}

@font-face {
  font-family: 'CustomFont';
  src: url('./fonts/custom-font-bold.woff2') format('woff2');
  font-weight: 700;
  font-style: normal;
  font-display: swap;
}

/* CSS custom properties in media queries */
@media (min-width: 768px) {
  :root {
    --container-padding: 2rem;
    --font-size-base: 1.125rem;
    --grid-columns: 2;
  }
}

@media (min-width: 1024px) {
  :root {
    --container-padding: 3rem;
    --font-size-base: 1.25rem;
    --grid-columns: 3;
  }
}

/* Animation classes using keyframes */
.fade-in {
  animation: fadeIn 0.6s ease-out;
}

.slide-in-left {
  animation: slideInFromLeft 0.8s cubic-bezier(0.25, 0.46, 0.45, 0.94);
}

.bounce-in {
  animation: bounceIn 1s ease-out;
}

.pulse-animation {
  animation: pulse 2s ease-in-out infinite;
}

.complex-animation {
  animation:
    fadeIn 0.5s ease-out,
    slideInFromLeft 0.8s 0.2s ease-out both,
    pulse 2s 1s ease-in-out infinite;
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(css_code, None).unwrap();

            let mut extractor = CSSExtractor::new(
                "css".to_string(),
                "at-rules.css".to_string(),
                css_code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Import statements
            let font_import = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@import url('https://fonts.googleapis.com"));
            assert!(font_import.is_some());
            assert_eq!(font_import.unwrap().kind, SymbolKind::Import);

            let normalize_import = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@import url('./normalize.css')"));
            assert!(normalize_import.is_some());

            let layered_import = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("layer(components)"));
            assert!(layered_import.is_some());

            // CSS layers
            let layer_declaration = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@layer reset, base, components"));
            assert!(layer_declaration.is_some());

            let reset_layer = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@layer reset"));
            assert!(reset_layer.is_some());

            let base_layer = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@layer base"));
            assert!(base_layer.is_some());

            // Media queries
            let media_768 = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@media (min-width: 768px)"));
            assert!(media_768.is_some());

            let media_1024 = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@media (min-width: 1024px)"));
            assert!(media_1024.is_some());

            let media_1200 = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@media (min-width: 1200px)"));
            assert!(media_1200.is_some());

            // Feature queries
            let grid_support = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@supports (display: grid)"));
            assert!(grid_support.is_some());

            let backdrop_support = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@supports (backdrop-filter: blur(10px))"));
            assert!(backdrop_support.is_some());

            let no_grid_support = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@supports not (display: grid)"));
            assert!(no_grid_support.is_some());

            // Keyframes
            let fade_in_keyframes = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@keyframes fadeIn"));
            assert!(fade_in_keyframes.is_some());
            assert_eq!(fade_in_keyframes.unwrap().kind, SymbolKind::Function); // Animations as functions

            let slide_in_keyframes = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@keyframes slideInFromLeft"));
            assert!(slide_in_keyframes.is_some());

            let bounce_in_keyframes = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@keyframes bounceIn"));
            assert!(bounce_in_keyframes.is_some());

            let pulse_keyframes = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@keyframes pulse"));
            assert!(pulse_keyframes.is_some());

            // Complex media queries
            let tablet_only = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("(min-width: 768px) and (max-width: 1023px)"));
            assert!(tablet_only.is_some());

            let landscape_short = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("(orientation: landscape) and (max-height: 500px)"));
            assert!(landscape_short.is_some());

            let dark_scheme = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("(prefers-color-scheme: dark)"));
            assert!(dark_scheme.is_some());

            let reduced_motion = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("(prefers-reduced-motion: reduce)"));
            assert!(reduced_motion.is_some());

            let hover_pointer = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("(hover: hover) and (pointer: fine)"));
            assert!(hover_pointer.is_some());

            // Print styles
            let print_media = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@media print"));
            assert!(print_media.is_some());

            // Font face
            let font_face_1 = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@font-face") && s.signature.as_ref().unwrap().contains("font-weight: 400"));
            assert!(font_face_1.is_some());

            let font_face_2 = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("@font-face") && s.signature.as_ref().unwrap().contains("font-weight: 700"));
            assert!(font_face_2.is_some());

            // Animation classes
            let fade_in_class = symbols.iter().find(|s| s.name == ".fade-in");
            assert!(fade_in_class.is_some());
            assert!(fade_in_class.unwrap().signature.as_ref().unwrap().contains("animation: fadeIn 0.6s ease-out"));

            let slide_in_class = symbols.iter().find(|s| s.name == ".slide-in-left");
            assert!(slide_in_class.is_some());

            let bounce_in_class = symbols.iter().find(|s| s.name == ".bounce-in");
            assert!(bounce_in_class.is_some());

            let pulse_class = symbols.iter().find(|s| s.name == ".pulse-animation");
            assert!(pulse_class.is_some());

            let complex_animation = symbols.iter().find(|s| s.name == ".complex-animation");
            assert!(complex_animation.is_some());
            assert!(complex_animation.unwrap().signature.as_ref().unwrap().contains("fadeIn 0.5s ease-out"));
            assert!(complex_animation.unwrap().signature.as_ref().unwrap().contains("slideInFromLeft 0.8s 0.2s ease-out both"));
        }
    }

    mod advanced_css_selectors_and_pseudo_elements {
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

            let mut parser = init_parser();
            let tree = parser.parse(css_code, None).unwrap();

            let mut extractor = CSSExtractor::new(
                "css".to_string(),
                "selectors.css".to_string(),
                css_code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Combinator selectors
            let direct_child = symbols.iter().find(|s| s.name == ".parent > .direct-child");
            assert!(direct_child.is_some());
            assert!(direct_child.unwrap().signature.as_ref().unwrap().contains("color: red"));

            let descendant = symbols.iter().find(|s| s.name == ".ancestor .descendant");
            assert!(descendant.is_some());

            let adjacent = symbols.iter().find(|s| s.name == ".sibling + .adjacent");
            assert!(adjacent.is_some());

            let general_sibling = symbols.iter().find(|s| s.name == ".element ~ .general-sibling");
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
            let has_invalid = symbols.iter().find(|s| s.name == ".form-group:has(input:invalid)");
            assert!(has_invalid.is_some());

            let has_featured = symbols.iter().find(|s| s.name == ".card:has(> .featured)");
            assert!(has_featured.is_some());

            let not_disabled = symbols.iter().find(|s| s.name == ".container:not(.disabled):not(.loading)");
            assert!(not_disabled.is_some());

            let is_inputs = symbols.iter().find(|s| s.name == ".input:is(.text, .email, .password)");
            assert!(is_inputs.is_some());

            let where_buttons = symbols.iter().find(|s| s.name == ".button:where(.primary, .secondary)");
            assert!(where_buttons.is_some());

            // Pseudo-elements
            let before_element = symbols.iter().find(|s| s.name == ".element::before");
            assert!(before_element.is_some());
            assert!(before_element.unwrap().signature.as_ref().unwrap().contains("content: ''"));
            assert!(before_element.unwrap().signature.as_ref().unwrap().contains("position: absolute"));

            let after_element = symbols.iter().find(|s| s.name == ".element::after");
            assert!(after_element.is_some());
            assert!(after_element.unwrap().signature.as_ref().unwrap().contains("content: attr(data-label)"));

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

            let first_letter = symbols.iter().find(|s| s.name == ".paragraph::first-letter");
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
            let nth_of_type = symbols.iter().find(|s| s.name == ".grid-item:nth-of-type(3n)");
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
            let complex_nested = symbols.iter().find(|s| s.name == ".header .navigation ul li a:hover::after");
            assert!(complex_nested.is_some());

            let super_complex = symbols.iter().find(|s| s.name.contains(".card:has(.image) .content:not(.minimal) h2:first-of-type::before"));
            assert!(super_complex.is_some());

            // WebKit pseudo-elements
            let webkit_scrollbar = symbols.iter().find(|s| s.name == "::-webkit-scrollbar");
            assert!(webkit_scrollbar.is_some());

            let webkit_scrollbar_thumb = symbols.iter().find(|s| s.name == "::-webkit-scrollbar-thumb");
            assert!(webkit_scrollbar_thumb.is_some());

            let webkit_scrollbar_thumb_hover = symbols.iter().find(|s| s.name == "::-webkit-scrollbar-thumb:hover");
            assert!(webkit_scrollbar_thumb_hover.is_some());
        }
    }
}