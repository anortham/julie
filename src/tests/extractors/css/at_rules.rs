use super::extract_symbols;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
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

        let symbols = extract_symbols(css_code);

        // Import statements
        let font_import = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@import url('https://fonts.googleapis.com")
        });
        assert!(font_import.is_some());
        assert_eq!(font_import.unwrap().kind, SymbolKind::Import);

        let normalize_import = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@import url('./normalize.css')")
        });
        assert!(normalize_import.is_some());

        let layered_import = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("layer(components)"));
        assert!(layered_import.is_some());

        // CSS layers
        let layer_declaration = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@layer reset, base, components")
        });
        assert!(layer_declaration.is_some());

        let reset_layer = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("@layer reset"));
        assert!(reset_layer.is_some());

        let base_layer = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("@layer base"));
        assert!(base_layer.is_some());

        // Media queries
        let media_768 = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@media (min-width: 768px)")
        });
        assert!(media_768.is_some());

        let media_1024 = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@media (min-width: 1024px)")
        });
        assert!(media_1024.is_some());

        let media_1200 = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@media (min-width: 1200px)")
        });
        assert!(media_1200.is_some());

        // Feature queries
        let grid_support = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@supports (display: grid)")
        });
        assert!(grid_support.is_some());

        let backdrop_support = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@supports (backdrop-filter: blur(10px))")
        });
        assert!(backdrop_support.is_some());

        let no_grid_support = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@supports not (display: grid)")
        });
        assert!(no_grid_support.is_some());

        // Keyframes
        let fade_in_keyframes = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("@keyframes fadeIn"));
        assert!(fade_in_keyframes.is_some());
        assert_eq!(fade_in_keyframes.unwrap().kind, SymbolKind::Function); // Animations as functions

        let slide_in_keyframes = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@keyframes slideInFromLeft")
        });
        assert!(slide_in_keyframes.is_some());

        let bounce_in_keyframes = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@keyframes bounceIn")
        });
        assert!(bounce_in_keyframes.is_some());

        let pulse_keyframes = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("@keyframes pulse"));
        assert!(pulse_keyframes.is_some());

        // Complex media queries
        let tablet_only = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("(min-width: 768px) and (max-width: 1023px)")
        });
        assert!(tablet_only.is_some());

        let landscape_short = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("(orientation: landscape) and (max-height: 500px)")
        });
        assert!(landscape_short.is_some());

        let dark_scheme = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("(prefers-color-scheme: dark)")
        });
        assert!(dark_scheme.is_some());

        let reduced_motion = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("(prefers-reduced-motion: reduce)")
        });
        assert!(reduced_motion.is_some());

        let hover_pointer = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("(hover: hover) and (pointer: fine)")
        });
        assert!(hover_pointer.is_some());

        // Print styles
        let print_media = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("@media print"));
        assert!(print_media.is_some());

        // Font face
        let font_face_1 = symbols.iter().find(|s| {
            s.signature.as_ref().unwrap().contains("@font-face")
                && s.signature.as_ref().unwrap().contains("font-weight: 400")
        });
        assert!(font_face_1.is_some());

        let font_face_2 = symbols.iter().find(|s| {
            s.signature.as_ref().unwrap().contains("@font-face")
                && s.signature.as_ref().unwrap().contains("font-weight: 700")
        });
        assert!(font_face_2.is_some());

        // Animation classes
        let fade_in_class = symbols.iter().find(|s| s.name == ".fade-in");
        assert!(fade_in_class.is_some());
        assert!(
            fade_in_class
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("animation: fadeIn 0.6s ease-out")
        );

        let slide_in_class = symbols.iter().find(|s| s.name == ".slide-in-left");
        assert!(slide_in_class.is_some());

        let bounce_in_class = symbols.iter().find(|s| s.name == ".bounce-in");
        assert!(bounce_in_class.is_some());

        let pulse_class = symbols.iter().find(|s| s.name == ".pulse-animation");
        assert!(pulse_class.is_some());

        let complex_animation = symbols.iter().find(|s| s.name == ".complex-animation");
        assert!(complex_animation.is_some());
        assert!(
            complex_animation
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("fadeIn 0.5s ease-out")
        );
        assert!(
            complex_animation
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("slideInFromLeft 0.8s 0.2s ease-out both")
        );
    }
}
