use super::{parse_cpp, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_namespace_declarations_and_include_statements() {
        let cpp_code = r#"
    #include <iostream>
    #include <vector>
    #include "custom_header.h"

    using namespace std;
    using std::string;

    namespace MyCompany {
        namespace Utils {
            // Nested namespace content
        }
    }

    namespace MyProject = MyCompany::Utils;  // Namespace alias
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        let std_namespace = symbols.iter().find(|s| s.name == "std");
        assert!(std_namespace.is_some());
        assert_eq!(std_namespace.unwrap().kind, SymbolKind::Import);

        let my_company = symbols.iter().find(|s| s.name == "MyCompany");
        assert!(my_company.is_some());
        assert_eq!(my_company.unwrap().kind, SymbolKind::Namespace);

        let utils = symbols.iter().find(|s| s.name == "Utils");
        assert!(utils.is_some());
        assert_eq!(utils.unwrap().kind, SymbolKind::Namespace);

        let alias = symbols.iter().find(|s| s.name == "MyProject");
        assert!(alias.is_some());
        assert!(alias
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("MyCompany::Utils"));
    }
}
