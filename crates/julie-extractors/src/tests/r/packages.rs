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

        assert_eq!(
            sorted_symbol_names(imports),
            vec!["data.table", "dplyr", "ggplot2", "tidyr"]
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

        assert_eq!(
            sorted_symbol_names(imports),
            vec!["dplyr", "ggplot2", "somePackage"]
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

        assert_eq!(
            sorted_symbol_names(variables),
            vec!["internal_result", "plot", "result1"]
        );
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
        assert_eq!(
            symbols_by_kind(&symbols, SymbolKind::Import),
            vec!["tidyverse"]
        );
        assert_eq!(
            symbols_by_kind(&symbols, SymbolKind::Variable),
            vec!["data"]
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
        assert!(
            symbols.is_empty(),
            "detach/unload calls should not create package symbols: {symbols:?}"
        );
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

        assert_eq!(symbols_by_kind(&symbols, SymbolKind::Import), vec!["dplyr"]);
        assert_eq!(
            symbols_by_kind(&symbols, SymbolKind::Variable),
            Vec::<&str>::new()
        );
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

        assert_eq!(sorted_symbol_names(variables), vec!["result"]);
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

        assert_eq!(sorted_symbol_names(functions), vec!["check_pkg"]);
        assert_eq!(symbols_by_kind(&symbols, SymbolKind::Import), vec!["dplyr"]);
    }

    fn symbols_by_kind(symbols: &[Symbol], kind: SymbolKind) -> Vec<&str> {
        let matching = symbols
            .iter()
            .filter(|symbol| symbol.kind == kind)
            .collect::<Vec<_>>();
        sorted_symbol_names(matching)
    }

    fn sorted_symbol_names(symbols: Vec<&Symbol>) -> Vec<&str> {
        let mut names = symbols
            .into_iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }
}
