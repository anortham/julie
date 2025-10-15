use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_custom_properties_and_math_functions() {
        let css_code = r#"
/* CSS Custom Properties (Variables) */
:root {
  --primary-color: #007bff;
  --secondary-color: #6c757d;
  --font-size-base: 16px;
  --spacing-unit: 8px;
  --border-radius: 4px;
  --box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

/* Using custom properties */
.button {
  background-color: var(--primary-color);
  color: white;
  padding: calc(var(--spacing-unit) * 2);
  border-radius: var(--border-radius);
  box-shadow: var(--box-shadow);
}

.card {
  border: 1px solid var(--secondary-color);
  border-radius: calc(var(--border-radius) * 2);
  padding: var(--spacing-unit);
}

/* CSS Math functions */
.container {
  width: calc(100% - 2 * var(--spacing-unit));
  height: min(500px, 80vh);
  margin: 0 auto;
}

.grid-item {
  width: clamp(200px, 25%, 300px);
  aspect-ratio: 16 / 9;
}

.responsive-text {
  font-size: clamp(1rem, 2vw, 2rem);
  line-height: max(1.2, 1.5);
}

/* Color functions */
.accent-colors {
  background: hsl(210, 100%, 50%);
  border: 2px solid hsl(210, 100%, 40%);
  color: hsl(210, 100%, 90%);
}

.dark-mode {
  background: hsl(var(--hue, 210), 30%, 10%);
  color: hsl(var(--hue, 210), 20%, 90%);
}

/* Filter functions */
.image-effects {
  filter: blur(2px) brightness(1.2) contrast(1.1);
}

.hover-effect:hover {
  filter: grayscale(50%) sepia(20%);
}

/* Transform functions */
.transform-examples {
  transform: translate(10px, 20px) rotate(45deg) scale(1.2);
}

.advanced-transform {
  transform: perspective(1000px) rotateY(30deg) translateZ(100px);
}

/* Gradient functions */
.gradient-background {
  background: linear-gradient(45deg, #ff6b6b, #4ecdc4, #45b7d1);
}

.radial-gradient {
  background: radial-gradient(circle at center, #ff6b6b, #4ecdc4);
}

.conic-gradient {
  background: conic-gradient(from 0deg, #ff6b6b, #4ecdc4, #45b7d1);
}

/* Custom property fallbacks */
.safe-variables {
  color: var(--text-color, #333);
  font-size: var(--font-size, var(--font-size-base, 16px));
  margin: var(--margin, calc(var(--spacing-unit) * 2), 16px);
}

/* Dynamic custom properties */
.dynamic-theme {
  --theme-color: #007bff;
  --theme-hover: hsl(from var(--theme-color) h s calc(l + 10%));
}

.theme-button {
  background-color: var(--theme-color);
  border-color: var(--theme-hover);
}

.theme-button:hover {
  background-color: var(--theme-hover);
}
"#;

        let symbols = extract_symbols(css_code);

        // Test custom properties
        let root = symbols.iter().find(|s| s.name == ":root");
        assert!(root.is_some());
        assert_eq!(root.unwrap().kind, SymbolKind::Class);

        // Test classes using custom properties
        let button = symbols.iter().find(|s| s.name == ".button");
        assert!(button.is_some());
        assert_eq!(button.unwrap().kind, SymbolKind::Class);

        let card = symbols.iter().find(|s| s.name == ".card");
        assert!(card.is_some());

        // Test math functions
        let container = symbols.iter().find(|s| s.name == ".container");
        assert!(container.is_some());

        let grid_item = symbols.iter().find(|s| s.name == ".grid-item");
        assert!(grid_item.is_some());

        let responsive_text = symbols.iter().find(|s| s.name == ".responsive-text");
        assert!(responsive_text.is_some());

        // Test color functions
        let accent_colors = symbols.iter().find(|s| s.name == ".accent-colors");
        assert!(accent_colors.is_some());

        let dark_mode = symbols.iter().find(|s| s.name == ".dark-mode");
        assert!(dark_mode.is_some());

        // Test filter functions
        let image_effects = symbols.iter().find(|s| s.name == ".image-effects");
        assert!(image_effects.is_some());

        let hover_effect = symbols.iter().find(|s| s.name == ".hover-effect:hover");
        assert!(hover_effect.is_some());

        // Test transform functions
        let transform_examples = symbols.iter().find(|s| s.name == ".transform-examples");
        assert!(transform_examples.is_some());

        let advanced_transform = symbols.iter().find(|s| s.name == ".advanced-transform");
        assert!(advanced_transform.is_some());

        // Test gradient functions
        let gradient_background = symbols.iter().find(|s| s.name == ".gradient-background");
        assert!(gradient_background.is_some());

        let radial_gradient = symbols.iter().find(|s| s.name == ".radial-gradient");
        assert!(radial_gradient.is_some());

        let conic_gradient = symbols.iter().find(|s| s.name == ".conic-gradient");
        assert!(conic_gradient.is_some());

        // Test safe variables
        let safe_variables = symbols.iter().find(|s| s.name == ".safe-variables");
        assert!(safe_variables.is_some());

        // Test dynamic theme
        let dynamic_theme = symbols.iter().find(|s| s.name == ".dynamic-theme");
        assert!(dynamic_theme.is_some());

        let theme_button = symbols.iter().find(|s| s.name == ".theme-button");
        assert!(theme_button.is_some());
    }
}