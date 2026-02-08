// R Identifiers Tests
// Tests for identifier extraction: function calls, variable references, member access

use super::*;
use crate::base::{IdentifierKind, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;
    use super::extract_all;

    #[test]
    fn test_extract_function_call_identifiers() {
        let r_code = r#"
process_data <- function(items) {
  cleaned <- clean_data(items)
  validated <- validate_data(cleaned)
  return(format_result(validated))
}

clean_data <- function(d) { d }
validate_data <- function(d) { d }
format_result <- function(d) { d }
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract identifiers for clean_data, validate_data, format_result calls
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            call_identifiers.len() >= 3,
            "Should extract at least 3 function call identifiers"
        );

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(
            call_names.contains(&"clean_data"),
            "Should extract clean_data call"
        );
        assert!(
            call_names.contains(&"validate_data"),
            "Should extract validate_data call"
        );
        assert!(
            call_names.contains(&"format_result"),
            "Should extract format_result call"
        );
    }

    #[test]
    fn test_extract_library_call_identifiers() {
        let r_code = r#"
library(dplyr)
library(ggplot2)
require(tidyr)

setup_environment <- function() {
  library(readr)
  require(stringr)
}
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract identifiers for library() and require() calls
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            call_identifiers.len() >= 5,
            "Should extract library/require call identifiers"
        );

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(
            call_names.contains(&"library"),
            "Should extract library calls"
        );
        assert!(
            call_names.contains(&"require"),
            "Should extract require calls"
        );
    }

    #[test]
    fn test_extract_variable_reference_identifiers() {
        let r_code = r#"
calculate <- function(x, y) {
  sum_val <- x + y
  diff_val <- x - y
  product <- sum_val * diff_val

  return(product)
}
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract variable references for x, y, sum_val, diff_val, product
        let var_ref_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::VariableRef)
            .collect();

        assert!(
            var_ref_identifiers.len() >= 2,
            "Should extract variable reference identifiers"
        );
    }

    #[test]
    fn test_extract_pipeline_operator_identifiers() {
        let r_code = r#"
process <- function(data) {
  result <- data %>%
    filter(value > 10) %>%
    select(name, value) %>%
    arrange(desc(value))

  return(result)
}
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract identifiers for filter, select, arrange, desc
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            call_identifiers.len() >= 4,
            "Should extract pipeline function call identifiers"
        );

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(
            call_names.contains(&"filter"),
            "Should extract filter call"
        );
        assert!(
            call_names.contains(&"select"),
            "Should extract select call"
        );
        assert!(
            call_names.contains(&"arrange"),
            "Should extract arrange call"
        );
    }

    #[test]
    fn test_extract_dollar_operator_member_access() {
        let r_code = r#"
get_info <- function(person) {
  name <- person$name
  age <- person$age
  city <- person$address$city

  info <- paste(name, age, city)
  return(info)
}
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract member access identifiers for $ operator
        let member_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::MemberAccess)
            .collect();

        assert!(
            member_identifiers.len() >= 3,
            "Should extract member access identifiers for name, age, city"
        );

        let member_names: Vec<&str> = member_identifiers
            .iter()
            .map(|id| id.name.as_str())
            .collect();
        assert!(
            member_names.contains(&"name"),
            "Should extract name member access"
        );
        assert!(
            member_names.contains(&"age"),
            "Should extract age member access"
        );
        assert!(
            member_names.contains(&"city"),
            "Should extract city member access"
        );
    }

    #[test]
    fn test_extract_base_function_identifiers() {
        let r_code = r#"
stats <- function(values) {
  n <- length(values)
  avg <- mean(values)
  median_val <- median(values)
  std <- sd(values)

  summary <- list(
    count = n,
    average = avg,
    median = median_val,
    std_dev = std
  )

  return(summary)
}
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract calls to base R functions
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            call_identifiers.len() >= 5,
            "Should extract calls to length, mean, median, sd, list"
        );

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(call_names.contains(&"length"), "Should extract length call");
        assert!(call_names.contains(&"mean"), "Should extract mean call");
        assert!(call_names.contains(&"median"), "Should extract median call");
        assert!(call_names.contains(&"sd"), "Should extract sd call");
        assert!(call_names.contains(&"list"), "Should extract list call");
    }

    #[test]
    fn test_extract_nested_function_call_identifiers() {
        let r_code = r#"
complex_calc <- function(x, y) {
  result <- sqrt(abs(x - y))
  rounded <- round(result, digits = 2)
  return(rounded)
}
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract sqrt, abs, round calls
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            call_identifiers.len() >= 3,
            "Should extract nested function call identifiers"
        );

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(call_names.contains(&"sqrt"), "Should extract sqrt call");
        assert!(call_names.contains(&"abs"), "Should extract abs call");
        assert!(call_names.contains(&"round"), "Should extract round call");
    }

    #[test]
    fn test_identifier_location_accuracy() {
        let r_code = r#"
test <- function() {
  calculate_sum(10, 20)
}
"#;

        let identifiers = extract_identifiers(r_code);

        let calc_sum_id = identifiers
            .iter()
            .find(|id| id.name == "calculate_sum")
            .expect("Should find calculate_sum identifier");

        // Verify position information is captured
        assert!(calc_sum_id.start_line > 0, "Should have valid start_line");
        assert!(calc_sum_id.end_line > 0, "Should have valid end_line");
        assert!(
            calc_sum_id.start_line <= calc_sum_id.end_line,
            "start_line should be <= end_line"
        );
    }

    // ========================================================================
    // Tests for containing_symbol_id (identifier containment within symbols)
    // ========================================================================

    #[test]
    fn test_identifier_inside_function_has_scope() {
        let code = r#"
my_function <- function(x) {
    result <- process(x)
    print(result)
}
"#;
        let (symbols, _, identifiers) = extract_all(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "my_function")
            .expect("Should find my_function symbol");

        let process_id = identifiers
            .iter()
            .find(|i| i.name == "process" && i.kind == IdentifierKind::Call)
            .expect("Should find process call identifier");

        // process() call should be scoped to my_function
        assert_eq!(
            process_id.containing_symbol_id.as_deref(),
            Some(func.id.as_str()),
            "process() call inside my_function should have containing_symbol_id = my_function's id"
        );
    }

    #[test]
    fn test_identifier_scoping_distinguishes_inside_vs_outside_function() {
        // Identifiers inside a function should be scoped to it;
        // a top-level variable assignment scopes its RHS identifiers to that variable
        let code = r#"
result <- compute(42)

my_function <- function(x) {
    inner_result <- process(x)
}
"#;
        let (symbols, _, identifiers) = extract_all(code);

        // compute() is in a top-level assignment `result <- compute(42)`
        // so it should be scoped to the `result` variable symbol
        let result_var = symbols.iter().find(|s| s.name == "result");
        let compute_id = identifiers
            .iter()
            .find(|i| i.name == "compute" && i.kind == IdentifierKind::Call)
            .expect("Should find compute call identifier");

        if let Some(result_sym) = result_var {
            // If result is extracted as a variable, compute should be scoped to it
            assert_eq!(
                compute_id.containing_symbol_id.as_deref(),
                Some(result_sym.id.as_str()),
                "compute() in `result <- compute(42)` should be scoped to result variable"
            );
        }

        // process() inside my_function should be scoped to my_function (not to inner_result)
        let func = symbols
            .iter()
            .find(|s| s.name == "my_function")
            .expect("Should find my_function");
        let process_id = identifiers
            .iter()
            .find(|i| i.name == "process" && i.kind == IdentifierKind::Call)
            .expect("Should find process call identifier");
        assert_eq!(
            process_id.containing_symbol_id.as_deref(),
            Some(func.id.as_str()),
            "process() inside my_function should be scoped to it (function takes priority over variable)"
        );
    }

    #[test]
    fn test_member_access_identifier_has_scope() {
        let code = r#"
get_name <- function(obj) {
    name <- obj$first_name
    return(name)
}
"#;
        let (symbols, _, identifiers) = extract_all(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "get_name")
            .expect("Should find get_name function");

        let member_id = identifiers
            .iter()
            .find(|i| i.name == "first_name" && i.kind == IdentifierKind::MemberAccess)
            .expect("Should find first_name member access");

        assert_eq!(
            member_id.containing_symbol_id.as_deref(),
            Some(func.id.as_str()),
            "obj$first_name access inside get_name should be scoped to it"
        );
    }

    #[test]
    fn test_variable_ref_identifier_has_scope() {
        let code = r#"
calculate <- function(a, b) {
    total <- a + b
    return(total)
}
"#;
        let (symbols, _, identifiers) = extract_all(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "calculate")
            .expect("Should find calculate function");

        // 'a' used in 'a + b' should be a variable ref scoped to calculate
        let a_refs: Vec<_> = identifiers
            .iter()
            .filter(|i| i.name == "a" && i.kind == IdentifierKind::VariableRef)
            .collect();

        assert!(
            !a_refs.is_empty(),
            "Should find variable reference to 'a'"
        );
        for a_ref in &a_refs {
            assert_eq!(
                a_ref.containing_symbol_id.as_deref(),
                Some(func.id.as_str()),
                "Variable ref 'a' inside calculate should be scoped to it"
            );
        }
    }

    #[test]
    fn test_extract_apply_family_identifiers() {
        let r_code = r#"
process_list <- function(items) {
  transformed <- lapply(items, transform_func)
  summarized <- sapply(transformed, summary_func)
  mapped <- vapply(summarized, format_func, character(1))

  return(mapped)
}
"#;

        let identifiers = extract_identifiers(r_code);

        // Should extract lapply, sapply, vapply calls
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(call_names.contains(&"lapply"), "Should extract lapply call");
        assert!(call_names.contains(&"sapply"), "Should extract sapply call");
        assert!(call_names.contains(&"vapply"), "Should extract vapply call");
    }
}
