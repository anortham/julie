use super::extract_symbols;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
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
}
"#;

        let symbols = extract_symbols(css_code);

        let universal_selector = symbols.iter().find(|s| s.name == "*");
        assert!(universal_selector.is_some());
        assert_eq!(universal_selector.unwrap().kind, SymbolKind::Variable);

        let body = symbols.iter().find(|s| s.name == "body");
        assert!(body.is_some());

        let link_hover = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("a:hover"));
        assert!(link_hover.is_some());

        let container = symbols.iter().find(|s| s.name == ".container");
        assert!(container.is_some());

        let btn_primary = symbols.iter().find(|s| s.name == ".btn-primary");
        assert!(btn_primary.is_some());

        let footer = symbols.iter().find(|s| s.name == "#footer");
        assert!(footer.is_some());

        let data_theme = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("[data-theme=\"dark\"]")
        });
        assert!(data_theme.is_some());

        let input_type = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("input[type=\"text\"]")
        });
        assert!(input_type.is_some());
    }
}
