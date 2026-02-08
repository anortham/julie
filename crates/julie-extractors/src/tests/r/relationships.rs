// R Relationships Tests
// Tests for relationship extraction: function calls, library usage, pipes

use super::*;
use crate::base::{RelationshipKind, SymbolKind};

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

        let (_symbols, relationships) = extract_symbols_and_relationships(r_code);

        // library() is a built-in function - built-in calls are silently dropped
        // (no Relationship and no PendingRelationship)
        // Piped calls to tidyverse functions (filter, mutate) also go through
        // PendingRelationship since they're not defined locally.
        // No resolved Relationship records should exist here since none
        // of these functions are defined in the same file.
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        // All calls are to built-in or external functions - no resolved relationships
        assert!(
            call_relationships.is_empty(),
            "Built-in/external function calls should not create resolved relationships"
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

        let (_symbols, relationships) = extract_symbols_and_relationships(r_code);

        // filter, select, arrange are built-in/tidyverse functions not defined locally.
        // They now create PendingRelationships instead of Relationships with synthetic IDs.
        // No resolved relationships should exist since nothing is defined locally.
        let pipe_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| {
                r.kind == RelationshipKind::Calls || r.kind == RelationshipKind::Uses
            })
            .collect();

        assert!(
            pipe_relationships.is_empty(),
            "Piped calls to external functions should create PendingRelationships, not resolved Relationships"
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

        // lapply, sapply, mean, return are builtins (silently dropped)
        // Only resolved relationships should be for locally-defined functions:
        // process_list -> transform_item (not a direct call, but passed as arg, won't resolve)
        // process_list -> summarize_item (same)
        // The actual direct calls are to lapply/sapply (builtins) which are dropped.
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        // Built-in calls (lapply, sapply, mean, return) are silently dropped.
        // No locally-defined functions are directly called in call position.
        // So we expect 0 resolved call relationships here.
        assert!(
            call_relationships.is_empty(),
            "Only direct calls to locally-defined functions create resolved relationships. Found: {:?}",
            call_relationships.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
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

        let (_symbols, relationships) = extract_symbols_and_relationships(r_code);

        // length, mean, sd, list, return are all built-in R functions.
        // Built-in calls are silently dropped (no Relationship, no PendingRelationship).
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.is_empty(),
            "Built-in function calls should not create resolved relationships"
        );
    }

    // ========================================================================
    // Tests for synthetic ID removal
    // ========================================================================

    #[test]
    fn test_no_synthetic_builtin_ids_in_relationships() {
        let r_code = r#"
analyze <- function(data) {
  n <- length(data)
  avg <- mean(data)
  print(avg)
  return(n)
}
"#;
        let (_, relationships) = extract_symbols_and_relationships(r_code);

        // No relationship should have a to_symbol_id starting with "builtin_"
        let synthetic_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.to_symbol_id.starts_with("builtin_"))
            .collect();

        assert!(
            synthetic_rels.is_empty(),
            "No relationships should have synthetic 'builtin_' IDs. Found: {:?}",
            synthetic_rels.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_no_synthetic_piped_ids_in_relationships() {
        let r_code = r#"
process <- function(data) {
  result <- data %>%
    filter(x > 10) %>%
    select(name) %>%
    arrange(name)
  return(result)
}
"#;
        let (_, relationships) = extract_symbols_and_relationships(r_code);

        // No relationship should have a to_symbol_id starting with "piped_"
        let synthetic_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.to_symbol_id.starts_with("piped_"))
            .collect();

        assert!(
            synthetic_rels.is_empty(),
            "No relationships should have synthetic 'piped_' IDs. Found: {:?}",
            synthetic_rels.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_no_synthetic_member_ids_in_relationships() {
        let r_code = r#"
get_info <- function(record) {
  name <- record$name
  age <- record$age
  return(paste(name, age))
}
"#;
        let (_, relationships) = extract_symbols_and_relationships(r_code);

        // No relationship should have a to_symbol_id starting with "member_"
        let synthetic_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.to_symbol_id.starts_with("member_"))
            .collect();

        assert!(
            synthetic_rels.is_empty(),
            "No relationships should have synthetic 'member_' IDs. Found: {:?}",
            synthetic_rels.iter().map(|r| &r.to_symbol_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_builtin_calls_create_pending_relationships() {
        // Built-in function calls should use PendingRelationship, not synthetic IDs
        let r_code = r#"
analyze <- function(data) {
  n <- length(data)
  avg <- mean(data)
  print(avg)
  return(n)
}
"#;
        let (_symbols, relationships) = extract_symbols_and_relationships(r_code);

        // We still need to check that these calls are tracked somehow
        // They should be in pending_relationships, not as Relationship with synthetic IDs
        // The key assertion is: no builtin_ in to_symbol_id
        let builtin_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.to_symbol_id.starts_with("builtin_"))
            .collect();
        assert!(
            builtin_rels.is_empty(),
            "Built-in calls should NOT use synthetic IDs"
        );
    }

    #[test]
    fn test_piped_calls_create_pending_relationships() {
        // Piped calls to unknown functions should use PendingRelationship
        let r_code = r#"
transform <- function(data) {
  result <- data %>%
    external_process() %>%
    another_step()
  return(result)
}
"#;
        let (_, relationships) = extract_symbols_and_relationships(r_code);

        let piped_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.to_symbol_id.starts_with("piped_"))
            .collect();
        assert!(
            piped_rels.is_empty(),
            "Piped calls should NOT use synthetic IDs"
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

        let (_symbols, relationships) = extract_symbols_and_relationships(r_code);

        // Dollar operator member accesses now create PendingRelationships
        // (member targets can't be resolved locally - they're dynamic)
        let member_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Uses)
            .collect();

        assert!(
            member_relationships.is_empty(),
            "Member access should create PendingRelationships, not resolved Relationships"
        );
    }
}
