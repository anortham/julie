use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_variables_calc_and_modern_css_functions() {
        let css_code = r#"
:root {
  --primary-color: #3498db;
  --secondary-color: #2ecc71;
  --accent-color: #e74c3c;
  --background-color: #ffffff;
  --text-color: #2c3e50;
  --border-color: #bdc3c7;
  --font-family-primary: 'Inter', system-ui, sans-serif;
  --font-family-secondary: 'Fira Code', 'Courier New', monospace;
  --font-size-base: 1rem;
  --font-size-small: 0.875rem;
  --font-size-large: 1.125rem;
  --line-height-base: 1.6;
  --spacing-xs: 0.25rem;
  --spacing-sm: 0.5rem;
  --spacing-md: 1rem;
  --spacing-lg: 1.5rem;
  --spacing-xl: 2rem;
  --container-max-width: 1200px;
  --sidebar-width: 250px;
  --header-height: 80px;
  --border-radius: 8px;
  --transition-fast: 0.15s ease;
  --transition-base: 0.3s ease;
  --transition-slow: 0.5s ease;
  --z-dropdown: 1000;
  --z-sticky: 1020;
  --z-fixed: 1030;
  --z-modal: 1040;
  --z-tooltip: 1050;
}

[data-theme="dark"] {
  --primary-color: #5dade2;
  --background-color: #2c3e50;
  --text-color: #ecf0f1;
  --border-color: #34495e;
}

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

.modern-functions {
  font-size: clamp(1rem, 2.5vw, 2rem);
  width: min(90vw, var(--container-max-width));
  height: max(50vh, 400px);
  background-color: hsl(var(--primary-hue, 200), 70%, 50%);
  color: oklch(0.7 0.15 200);
  min-height: 100svh;
  padding-top: max(env(safe-area-inset-top), var(--spacing-md));
}

.comparison-functions {
  margin: max(var(--spacing-md), 2rem);
  padding: min(5%, var(--spacing-xl));
  width: clamp(300px, 50vw, 800px);
  height: clamp(200px, 30vh, 500px);
}

.advanced-math {
  transform: rotate(calc(sin(var(--angle, 0)) * 45deg));
  opacity: pow(0.8, var(--depth, 1));
  border-radius: calc(sqrt(var(--area, 100)) * 1px);
}

.logical-properties {
  margin-inline: var(--spacing-md);
  padding-block: var(--spacing-lg);
  inset-inline-start: var(--sidebar-width);
  border-start-start-radius: var(--border-radius);
  border-end-end-radius: var(--border-radius);
}

.nested-component {
  & .header {
    background: var(--primary-color);
    padding: var(--spacing-lg);

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

.color-mixing {
  background-color: color-mix(in oklch, var(--primary-color), var(--secondary-color) 30%);
  border-color: color-mix(in srgb, var(--border-color), transparent 50%);
}

.container-units {
  width: calc(50cqw - var(--spacing-md));
  height: calc(30cqh + var(--spacing-lg));
  font-size: calc(2cqi + var(--font-size-base));
}
"#;

        let symbols = extract_symbols(css_code);

        let root_selector = symbols.iter().find(|s| s.name == ":root");
        assert!(root_selector.is_some());
        assert!(
            root_selector
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("--primary-color: #3498db")
        );

        let dark_theme = symbols.iter().find(|s| s.name == "[data-theme=\"dark\"]");
        assert!(dark_theme.is_some());

        let button = symbols.iter().find(|s| s.name == ".button");
        assert!(button.is_some());
        assert!(
            button
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("var(--primary-color)")
        );

        let button_hover = symbols.iter().find(|s| s.name == ".button:hover");
        assert!(button_hover.is_some());

        let layout_calc = symbols.iter().find(|s| s.name == ".layout-calc");
        assert!(layout_calc.is_some());

        let responsive_grid = symbols.iter().find(|s| s.name == ".responsive-grid");
        assert!(responsive_grid.is_some());

        let complex_calc = symbols.iter().find(|s| s.name == ".complex-calc");
        assert!(complex_calc.is_some());

        let modern_functions = symbols.iter().find(|s| s.name == ".modern-functions");
        assert!(modern_functions.is_some());

        let comparison_functions = symbols.iter().find(|s| s.name == ".comparison-functions");
        assert!(comparison_functions.is_some());

        let advanced_math = symbols.iter().find(|s| s.name == ".advanced-math");
        assert!(advanced_math.is_some());

        let logical_properties = symbols.iter().find(|s| s.name == ".logical-properties");
        assert!(logical_properties.is_some());

        let nested_component = symbols.iter().find(|s| s.name == ".nested-component");
        assert!(nested_component.is_some());

        let color_mixing = symbols.iter().find(|s| s.name == ".color-mixing");
        assert!(color_mixing.is_some());

        let container_units = symbols.iter().find(|s| s.name == ".container-units");
        assert!(container_units.is_some());
    }
}
