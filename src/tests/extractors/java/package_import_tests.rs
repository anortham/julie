// Package and Import Extraction Tests
//
// Direct Implementation of Java extractor tests (TDD RED phase)

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::java::JavaExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod package_import_tests {
    use super::*;

    #[test]
    fn test_extract_package_declarations() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
package com.example.app;

package com.acme.utils;
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let app_package = symbols.iter().find(|s| s.name == "com.example.app");
        assert!(app_package.is_some());
        assert_eq!(app_package.unwrap().kind, SymbolKind::Namespace);
        assert!(app_package
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("package com.example.app"));
        assert_eq!(
            app_package.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Public
        );
    }

    #[test]
    fn test_extract_import_statements() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
package com.example;

import java.util.List;
import java.util.ArrayList;
import java.util.Map;
import static java.lang.Math.PI;
import static java.util.Collections.*;
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let list_import = symbols.iter().find(|s| s.name == "List");
        assert!(list_import.is_some());
        assert_eq!(list_import.unwrap().kind, SymbolKind::Import);
        assert!(list_import
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("import java.util.List"));

        let pi_import = symbols.iter().find(|s| s.name == "PI");
        assert!(pi_import.is_some());
        assert!(pi_import
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("import static java.lang.Math.PI"));

        let collections_import = symbols.iter().find(|s| s.name == "Collections");
        assert!(collections_import.is_some());
        assert!(collections_import
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("import static java.util.Collections.*"));
    }
}
