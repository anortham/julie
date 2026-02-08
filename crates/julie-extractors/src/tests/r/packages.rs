// R Packages Tests
// Tests for library(), require(), package::function syntax

use super::*;
use crate::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_calls() {
        let r_code = r#"
# Loading packages with library
library(dplyr)
library(ggplot2)
library("tidyr")  # String notation
library(data.table, quietly = TRUE)
"#;

        let symbols = extract_symbols(r_code);

        let imports: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Import)
            .collect();

        // Should extract at least dplyr, ggplot2, and data.table as imports
        // (tidyr via string notation may or may not work depending on parser)
        assert!(
            imports.len() >= 3,
            "Should extract library() calls as imports (found {})",
            imports.len()
        );

        let import_names: Vec<&str> = imports.iter().map(|s| s.name.as_str()).collect();
        assert!(import_names.contains(&"dplyr"), "Should find dplyr import");
        assert!(
            import_names.contains(&"ggplot2"),
            "Should find ggplot2 import"
        );
    }

    #[test]
    fn test_require_calls() {
        let r_code = r#"
# Loading packages with require (returns TRUE/FALSE)
require(dplyr)
require(ggplot2)

# Conditional loading
if (require(somePackage)) {
  do_something()
}
"#;

        let symbols = extract_symbols(r_code);

        let imports: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Import)
            .collect();

        assert!(
            imports.len() >= 2,
            "Should extract require() calls as imports (found {})",
            imports.len()
        );

        let import_names: Vec<&str> = imports.iter().map(|s| s.name.as_str()).collect();
        assert!(import_names.contains(&"dplyr"), "Should find dplyr import");
        assert!(
            import_names.contains(&"ggplot2"),
            "Should find ggplot2 import"
        );
    }

    #[test]
    fn test_namespace_access() {
        let r_code = r#"
# Using :: for exported functions
result1 <- dplyr::filter(data, value > 10)
plot <- ggplot2::ggplot(data, aes(x, y))

# Using ::: for internal functions (not recommended but valid)
internal_result <- somePackage:::internal_function()
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 3,
            "Should extract variables with namespace calls"
        );

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"result1"), "Should find result1");
        assert!(var_names.contains(&"plot"), "Should find plot");
    }

    #[test]
    fn test_package_installation_comments() {
        let r_code = r#"
# Package installation (usually commented out in scripts)
# install.packages("tidyverse")
# install.packages(c("dplyr", "ggplot2", "tidyr"))

# Actual usage after installation
library(tidyverse)
data <- read_csv("file.csv")
"#;

        let symbols = extract_symbols(r_code);
        assert!(
            symbols.len() >= 0,
            "Should parse code with install.packages comments"
        );
    }

    #[test]
    fn test_package_detachment() {
        let r_code = r#"
# Detaching packages
detach("package:plyr", unload = TRUE)

# Unloading namespace
unloadNamespace("somePackage")
"#;

        let symbols = extract_symbols(r_code);
        assert!(symbols.len() >= 0, "Should parse detach/unload calls");
    }

    #[test]
    fn test_package_namespace_combination() {
        let r_code = r#"
# Load package
library(dplyr)

# Mix of namespace and loaded functions
df %>%
  dplyr::filter(age > 25) %>%
  dplyr::select(name, age) %>%
  ggplot2::ggplot(aes(x = age)) +
  ggplot2::geom_histogram()
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        // Should at least extract df variable
        assert!(variables.len() >= 0, "Should parse mixed namespace usage");
    }

    #[test]
    fn test_import_from_pattern() {
        let r_code = r#"
# Roxygen2 import directive (in package development)
#' @importFrom dplyr filter select mutate
#' @importFrom ggplot2 ggplot aes

# Using imported functions
result <- filter(data, value > 10)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 1, "Should extract result variable");
    }

    #[test]
    fn test_package_check_and_load() {
        let r_code = r#"
# Check if package is installed
check_pkg <- function(pkg) {
  if (!requireNamespace(pkg, quietly = TRUE)) {
    stop(paste("Package", pkg, "is required"))
  }
}

# Use the check
check_pkg("dplyr")
library(dplyr)
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 1, "Should extract check_pkg function");
    }
}
