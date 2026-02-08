use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_media_queries_and_responsive_design() {
        let css_code = r#"
/* Basic media queries */
@media screen and (max-width: 768px) {
  .container {
    width: 100%;
    padding: 0 15px;
  }
}

@media print {
  .no-print {
    display: none;
  }
}

/* Complex media queries */
@media (min-width: 768px) and (max-width: 1024px) {
  .sidebar {
    display: block;
    width: 250px;
  }
}

@media (orientation: landscape) and (min-width: 1200px) {
  .hero {
    height: 80vh;
    background-size: cover;
  }
}

/* Mobile-first responsive design */
@media (min-width: 576px) {
  .card {
    display: flex;
  }
}

@media (min-width: 768px) {
  .navbar {
    flex-direction: row;
  }

  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
  }
}

@media (min-width: 992px) {
  .container {
    max-width: 960px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (min-width: 1200px) {
  .container {
    max-width: 1140px;
  }

  .hero-text {
    font-size: 3rem;
  }
}

/* Dark mode media query */
@media (prefers-color-scheme: dark) {
  body {
    background-color: #1a1a1a;
    color: #ffffff;
  }

  .card {
    background-color: #2d2d2d;
    border-color: #404040;
  }
}

@media (prefers-color-scheme: light) {
  body {
    background-color: #ffffff;
    color: #000000;
  }
}

/* High contrast mode */
@media (prefers-contrast: high) {
  .button {
    border: 2px solid currentColor;
  }
}

/* Reduced motion */
@media (prefers-reduced-motion: reduce) {
  .animated-element {
    animation: none;
    transition: none;
  }
}
"#;

        let symbols = extract_symbols(css_code);

        // Test media query selectors
        let container = symbols.iter().find(|s| s.name == ".container");
        assert!(container.is_some());
        assert_eq!(container.unwrap().kind, SymbolKind::Property);

        let sidebar = symbols.iter().find(|s| s.name == ".sidebar");
        assert!(sidebar.is_some());

        let hero = symbols.iter().find(|s| s.name == ".hero");
        assert!(hero.is_some());

        let card = symbols.iter().find(|s| s.name == ".card");
        assert!(card.is_some());

        let navbar = symbols.iter().find(|s| s.name == ".navbar");
        assert!(navbar.is_some());

        let grid = symbols.iter().find(|s| s.name == ".grid");
        assert!(grid.is_some());

        let hero_text = symbols.iter().find(|s| s.name == ".hero-text");
        assert!(hero_text.is_some());

        let body = symbols.iter().find(|s| s.name == "body");
        assert!(body.is_some());

        let button = symbols.iter().find(|s| s.name == ".button");
        assert!(button.is_some());

        let animated_element = symbols.iter().find(|s| s.name == ".animated-element");
        assert!(animated_element.is_some());
    }
}
