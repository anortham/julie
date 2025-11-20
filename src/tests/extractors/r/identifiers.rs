// R Identifiers Tests
// Tests for identifier extraction: function calls, variable references, member access

use super::*;
use crate::extractors::base::{IdentifierKind, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(call_names.contains(&"filter"), "Should extract filter call");
        assert!(call_names.contains(&"select"), "Should extract select call");
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
