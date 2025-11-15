// R Relationships Tests
// Tests for relationship extraction: function calls, library usage, pipes

use super::*;
use crate::extractors::base::{RelationshipKind, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_call_relationship() {
        let r_code = r#"
# Define functions
calculate_total <- function(items) {
  sum_values(items)
}

sum_values <- function(arr) {
  sum(arr)
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(r_code);

        // Verify we have both functions
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert!(functions.len() >= 2, "Should extract both functions");

        // Verify call relationship: calculate_total calls sum_values
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.len() >= 1,
            "Should extract at least one call relationship"
        );

        // Find specific call from calculate_total to sum_values
        let calculate_total = functions
            .iter()
            .find(|f| f.name == "calculate_total")
            .expect("Should find calculate_total function");

        let calls_from_calculate = call_relationships
            .iter()
            .filter(|r| r.from_symbol_id == calculate_total.id)
            .count();

        assert!(
            calls_from_calculate >= 1,
            "calculate_total should make at least 1 function call"
        );
    }

    #[test]
    fn test_extract_library_call_relationship() {
        let r_code = r#"
library(dplyr)
library(ggplot2)

process_data <- function(data) {
  data %>%
    filter(value > 10) %>%
    mutate(new_col = value * 2)
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(r_code);

        // Should have call relationships for library() calls
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.len() >= 2,
            "Should extract library() call relationships"
        );
    }

    #[test]
    fn test_extract_pipe_operator_relationships() {
        let r_code = r#"
process_data <- function(data) {
  result <- data %>%
    filter(age > 18) %>%
    select(name, age) %>%
    arrange(age)

  return(result)
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(r_code);

        // Pipe operators create "Uses" or "Calls" relationships
        let pipe_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| {
                r.kind == RelationshipKind::Calls || r.kind == RelationshipKind::Uses
            })
            .collect();

        assert!(
            pipe_relationships.len() >= 3,
            "Should extract relationships for filter, select, arrange calls in pipeline"
        );
    }

    #[test]
    fn test_extract_nested_function_calls() {
        let r_code = r#"
analyze_data <- function(data) {
  cleaned <- clean_data(data)
  validated <- validate_data(cleaned)
  result <- transform_data(validated)
  return(result)
}

clean_data <- function(d) { d }
validate_data <- function(d) { d }
transform_data <- function(d) { d }
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert!(functions.len() >= 4, "Should extract all four functions");

        // analyze_data should call clean_data, validate_data, transform_data
        let analyze_data = functions
            .iter()
            .find(|f| f.name == "analyze_data")
            .expect("Should find analyze_data function");

        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls && r.from_symbol_id == analyze_data.id)
            .collect();

        assert!(
            call_relationships.len() >= 3,
            "analyze_data should make at least 3 function calls"
        );
    }

    #[test]
    fn test_extract_apply_family_relationships() {
        let r_code = r#"
process_list <- function(items) {
  results <- lapply(items, transform_item)
  summary <- sapply(results, summarize_item)
  return(summary)
}

transform_item <- function(item) { item * 2 }
summarize_item <- function(item) { mean(item) }
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(r_code);

        // Should extract calls to lapply, sapply, transform_item, summarize_item, mean
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.len() >= 4,
            "Should extract calls to lapply, sapply, transform_item, summarize_item, mean"
        );
    }

    #[test]
    fn test_extract_base_function_calls() {
        let r_code = r#"
analyze <- function(data) {
  n <- length(data)
  avg <- mean(data)
  std <- sd(data)

  result <- list(count = n, average = avg, std_dev = std)
  return(result)
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(r_code);

        // Should extract calls to length, mean, sd, list
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.len() >= 4,
            "Should extract calls to base R functions: length, mean, sd, list"
        );
    }

    #[test]
    fn test_extract_dollar_operator_member_access() {
        let r_code = r#"
process_record <- function(record) {
  name <- record$name
  age <- record$age
  city <- record$address$city

  return(paste(name, age, city))
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(r_code);

        // Dollar operator creates member access relationships
        let member_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Uses)
            .collect();

        assert!(
            member_relationships.len() >= 3,
            "Should extract member access relationships for $ operator"
        );
    }
}
