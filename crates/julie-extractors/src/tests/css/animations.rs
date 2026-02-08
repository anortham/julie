use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_animations_keyframes_and_transitions() {
        let css_code = r#"
/* Keyframe animations */
@keyframes slideIn {
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

@keyframes fadeOut {
  from {
    opacity: 1;
  }
  to {
    opacity: 0;
  }
}

@keyframes bounce {
  0%, 20%, 50%, 80%, 100% {
    transform: translateY(0);
  }
  40% {
    transform: translateY(-30px);
  }
  60% {
    transform: translateY(-15px);
  }
}

/* Animation properties */
.animated-element {
  animation-name: slideIn, fadeOut;
  animation-duration: 2s, 1s;
  animation-timing-function: ease-in-out, linear;
  animation-delay: 0s, 2s;
  animation-iteration-count: 1, infinite;
  animation-direction: normal, alternate;
  animation-fill-mode: forwards, backwards;
  animation-play-state: running, paused;
}

/* Shorthand animation */
.quick-animation {
  animation: slideIn 2s ease-in-out 0s 1 normal forwards running;
}

/* Transitions */
.transition-element {
  transition-property: background-color, transform;
  transition-duration: 0.3s, 0.5s;
  transition-timing-function: ease, cubic-bezier(0.4, 0, 0.2, 1);
  transition-delay: 0s, 0.1s;
}

/* Shorthand transition */
.smooth-transition {
  transition: all 0.3s ease 0s;
}

/* Hover effects with transitions */
.button {
  background-color: #007bff;
  transition: background-color 0.3s ease;
}

.button:hover {
  background-color: #0056b3;
}

/* Transform animations */
.rotate-element {
  animation: spin 2s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
"#;

        let symbols = extract_symbols(css_code);

        // Test keyframe animations
        let slide_in = symbols.iter().find(|s| s.name == "slideIn");
        assert!(slide_in.is_some());
        assert_eq!(slide_in.unwrap().kind, SymbolKind::Function); // Keyframes are treated as functions

        let fade_out = symbols.iter().find(|s| s.name == "fadeOut");
        assert!(fade_out.is_some());

        let bounce = symbols.iter().find(|s| s.name == "bounce");
        assert!(bounce.is_some());

        let spin = symbols.iter().find(|s| s.name == "spin");
        assert!(spin.is_some());

        // Test animation properties
        let animated_element = symbols.iter().find(|s| s.name == ".animated-element");
        assert!(animated_element.is_some());
        assert_eq!(animated_element.unwrap().kind, SymbolKind::Class);

        let quick_animation = symbols.iter().find(|s| s.name == ".quick-animation");
        assert!(quick_animation.is_some());

        let transition_element = symbols.iter().find(|s| s.name == ".transition-element");
        assert!(transition_element.is_some());

        let smooth_transition = symbols.iter().find(|s| s.name == ".smooth-transition");
        assert!(smooth_transition.is_some());

        let button = symbols.iter().find(|s| s.name == ".button");
        assert!(button.is_some());

        let button_hover = symbols.iter().find(|s| s.name == ".button:hover");
        assert!(button_hover.is_some());

        let rotate_element = symbols.iter().find(|s| s.name == ".rotate-element");
        assert!(rotate_element.is_some());
    }

    #[test]
    fn test_keyframe_percentages_not_extracted() {
        let css_code = r#"
@keyframes slideIn {
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

@keyframes fadeOut {
  from {
    opacity: 1;
  }
  to {
    opacity: 0;
  }
}
"#;

        let symbols = extract_symbols(css_code);

        // The @keyframes rules and animation names SHOULD be extracted
        assert!(
            symbols.iter().any(|s| s.name == "@keyframes slideIn"),
            "Expected @keyframes slideIn rule to be extracted"
        );
        assert!(
            symbols.iter().any(|s| s.name == "slideIn"),
            "Expected slideIn animation name to be extracted"
        );
        assert!(
            symbols.iter().any(|s| s.name == "@keyframes fadeOut"),
            "Expected @keyframes fadeOut rule to be extracted"
        );
        assert!(
            symbols.iter().any(|s| s.name == "fadeOut"),
            "Expected fadeOut animation name to be extracted"
        );

        // Individual keyframe blocks MUST NOT be extracted — they are noise
        let noise_names = ["from", "to", "0%", "50%", "100%"];
        for noise in &noise_names {
            assert!(
                !symbols.iter().any(|s| s.name == *noise),
                "Keyframe block '{}' should NOT be extracted as a symbol, but was found",
                noise
            );
        }

        // Also verify no Variable-kind symbols exist at all (keyframe blocks were the only ones)
        let variable_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();
        assert!(
            variable_symbols.is_empty(),
            "No Variable-kind symbols should exist for keyframe blocks, but found: {:?}",
            variable_symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }
}
