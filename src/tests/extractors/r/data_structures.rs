// R Data Structures Tests
// Tests for data.frame, tibble, vector, list, matrix

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Debug test to inspect data structure AST
    fn debug_data_structures_ast() {
        let r_code = r#"
# Vectors
x <- c(1, 2, 3, 4, 5)
names <- c("Alice", "Bob", "Charlie")

# Lists
my_list <- list(name = "John", age = 30, scores = c(85, 90, 95))

# Data frames
df <- data.frame(
  id = 1:3,
  name = c("Alice", "Bob", "Charlie"),
  score = c(85, 90, 95)
)

# Matrices
m <- matrix(1:9, nrow = 3, ncol = 3)
"#;
        let tree = crate::tests::test_utils::init_parser(r_code, "r");
        let root = tree.root_node();

        fn print_ast(node: &tree_sitter::Node, depth: usize, code: &str) {
            let indent = "  ".repeat(depth);
            let node_text = node.utf8_text(code.as_bytes()).unwrap_or("<error>");
            let node_text_truncated = if node_text.len() > 50 {
                format!("{}...", &node_text[..50])
            } else {
                node_text.to_string()
            };

            println!(
                "{}{} [{}:{}] '{}'",
                indent,
                node.kind(),
                node.start_position().row,
                node.end_position().row,
                node_text_truncated.replace('\n', "\\n")
            );

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                print_ast(&child, depth + 1, code);
            }
        }

        print_ast(&root, 0, r_code);
    }

    #[test]
    fn test_vector_creation() {
        let r_code = r#"
# Numeric vector
x <- c(1, 2, 3, 4, 5)

# Character vector
names <- c("Alice", "Bob", "Charlie")

# Logical vector
flags <- c(TRUE, FALSE, TRUE)
"#;

        let symbols = extract_symbols(r_code);

        // Should extract all vector assignments
        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 3,
            "Should extract x, names, flags vectors (found {})",
            variables.len()
        );

        // Check for specific variables
        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"x"), "Should find x vector");
        assert!(var_names.contains(&"names"), "Should find names vector");
        assert!(var_names.contains(&"flags"), "Should find flags vector");
    }

    #[test]
    fn test_list_creation() {
        let r_code = r#"
# Simple list
my_list <- list(name = "John", age = 30, scores = c(85, 90, 95))

# Named list
person <- list(
  first_name = "Alice",
  last_name = "Smith",
  age = 28
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract my_list and person");

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"my_list"), "Should find my_list");
        assert!(var_names.contains(&"person"), "Should find person list");
    }

    #[test]
    fn test_data_frame_creation() {
        let r_code = r#"
# Basic data frame
df <- data.frame(
  id = 1:3,
  name = c("Alice", "Bob", "Charlie"),
  score = c(85, 90, 95)
)

# Data frame with stringsAsFactors
df2 <- data.frame(
  x = 1:5,
  y = letters[1:5],
  stringsAsFactors = FALSE
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract df and df2");

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"df"), "Should find df data frame");
        assert!(var_names.contains(&"df2"), "Should find df2 data frame");
    }

    #[test]
    fn test_matrix_creation() {
        let r_code = r#"
# Basic matrix
m <- matrix(1:9, nrow = 3, ncol = 3)

# Matrix with byrow
m2 <- matrix(c(1, 2, 3, 4, 5, 6), nrow = 2, byrow = TRUE)

# Matrix from vectors
vec1 <- c(1, 2, 3)
vec2 <- c(4, 5, 6)
mat <- rbind(vec1, vec2)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 5,
            "Should extract all matrix and vector variables"
        );

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"m"), "Should find m matrix");
        assert!(var_names.contains(&"m2"), "Should find m2 matrix");
        assert!(
            var_names.contains(&"mat"),
            "Should find mat matrix from rbind"
        );
    }

    #[test]
    fn test_tibble_creation() {
        let r_code = r#"
# Tibble creation (modern tidyverse data structure)
library(tibble)

tbl <- tibble(
  x = 1:5,
  y = letters[1:5],
  z = x * 2
)

# Tribble (row-wise tibble creation)
tbl2 <- tribble(
  ~name, ~age, ~score,
  "Alice", 25, 95,
  "Bob", 30, 87,
  "Charlie", 28, 92
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract tbl and tbl2 tibbles");

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"tbl"), "Should find tbl tibble");
        assert!(var_names.contains(&"tbl2"), "Should find tbl2 tribble");
    }

    #[test]
    fn test_sequence_creation() {
        let r_code = r#"
# Various ways to create sequences
seq1 <- 1:10
seq2 <- seq(1, 10)
seq3 <- seq(0, 1, by = 0.1)
seq4 <- rep(1, times = 5)
seq5 <- rep(c(1, 2, 3), each = 2)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 5,
            "Should extract all sequence variables"
        );

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(
            var_names.contains(&"seq1"),
            "Should find seq1 colon sequence"
        );
        assert!(
            var_names.contains(&"seq2"),
            "Should find seq2 seq() function"
        );
        assert!(
            var_names.contains(&"seq5"),
            "Should find seq5 rep with each"
        );
    }

    #[test]
    fn test_array_creation() {
        let r_code = r#"
# Multi-dimensional arrays
arr <- array(1:24, dim = c(3, 4, 2))

# Array with dimnames
arr2 <- array(
  data = 1:12,
  dim = c(3, 4),
  dimnames = list(
    c("Row1", "Row2", "Row3"),
    c("Col1", "Col2", "Col3", "Col4")
  )
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract arr and arr2 arrays");

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"arr"), "Should find arr array");
        assert!(
            var_names.contains(&"arr2"),
            "Should find arr2 array with dimnames"
        );
    }

    #[test]
    fn test_factor_creation() {
        let r_code = r#"
# Factor (categorical variable)
gender <- factor(c("Male", "Female", "Female", "Male"))

# Ordered factor
education <- factor(
  c("High School", "Bachelor", "Master", "PhD"),
  levels = c("High School", "Bachelor", "Master", "PhD"),
  ordered = TRUE
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 2,
            "Should extract gender and education factors"
        );

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"gender"), "Should find gender factor");
        assert!(
            var_names.contains(&"education"),
            "Should find education ordered factor"
        );
    }

    #[test]
    fn test_environment_creation() {
        let r_code = r#"
# Environment creation
env <- new.env()

# Environment with parent
child_env <- new.env(parent = env)

# List to environment conversion
my_list <- list(a = 1, b = 2, c = 3)
list_env <- list2env(my_list)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 4,
            "Should extract all environment variables"
        );

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"env"), "Should find env environment");
        assert!(
            var_names.contains(&"child_env"),
            "Should find child_env with parent"
        );
        assert!(
            var_names.contains(&"list_env"),
            "Should find list_env from list2env"
        );
    }
}
