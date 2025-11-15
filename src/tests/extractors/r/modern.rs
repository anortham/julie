// R Modern Patterns Tests
// Tests for modern R patterns (tidyverse, data.table)

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_table_basics() {
        let r_code = r#"
library(data.table)

# Create data.table
dt <- data.table(x = 1:5, y = letters[1:5])

# data.table operations
result <- dt[x > 2, .(mean_x = mean(x))]

# Reference semantics (modify in place)
dt[, z := x * 2]
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract dt and result");
    }

    #[test]
    fn test_data_table_chaining() {
        let r_code = r#"
# data.table chaining
result <- dt[
  age > 25
][,
  .(avg_score = mean(score), count = .N),
  by = category
][
  order(-avg_score)
]
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 1, "Should extract result");
    }

    #[test]
    fn test_formula_syntax() {
        let r_code = r#"
# Linear model with formula
model1 <- lm(mpg ~ wt + hp, data = mtcars)

# Formula with interaction
model2 <- lm(y ~ x1 * x2, data = df)

# Formula with transformations
model3 <- lm(log(y) ~ poly(x, 2) + factor(group), data = df)

# GLM with formula
logit_model <- glm(
  outcome ~ age + gender + treatment,
  data = df,
  family = binomial(link = "logit")
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 4, "Should extract all model variables");
    }

    #[test]
    fn test_non_standard_evaluation() {
        let r_code = r#"
# NSE with rlang
library(rlang)

my_function <- function(data, col) {
  col_sym <- ensym(col)
  data %>% select(!!col_sym)
}

# Quasiquotation
filter_by <- function(data, condition) {
  condition_expr <- enexpr(condition)
  data %>% filter(!!condition_expr)
}

# Tidy evaluation
summarize_col <- function(data, col) {
  data %>% summarize(mean = mean({{ col }}))
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 3, "Should extract all NSE functions");
    }

    #[test]
    fn test_functional_programming() {
        let r_code = r#"
library(purrr)

# Reduce
total <- reduce(1:10, `+`)

# Accumulate
cumulative <- accumulate(1:5, `*`)

# Compose functions
composed <- compose(sqrt, mean, abs)

# Partial application
add_five <- partial(`+`, 5)

# Keep/discard
evens <- keep(1:10, ~ .x %% 2 == 0)
odds <- discard(1:10, ~ .x %% 2 == 0)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 6,
            "Should extract functional programming results"
        );
    }

    #[test]
    fn test_async_parallel() {
        let r_code = r#"
library(future)
library(furrr)

# Setup parallel processing
plan(multisession, workers = 4)

# Parallel map
parallel_result <- future_map(1:100, ~ expensive_function(.x))

# Parallel processing with progress
result_with_progress <- future_map(
  data_list,
  ~ process_item(.x),
  .progress = TRUE
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 2,
            "Should extract parallel processing results"
        );
    }

    #[test]
    fn test_list_columns() {
        let r_code = r#"
# Nested data with list columns
nested_df <- df %>%
  group_by(category) %>%
  nest()

# Operate on list columns
results <- nested_df %>%
  mutate(
    model = map(data, ~ lm(y ~ x, data = .x)),
    predictions = map2(model, data, predict)
  )

# Unnest results
unnested <- results %>%
  select(category, predictions) %>%
  unnest(predictions)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 3,
            "Should extract nested data structures"
        );
    }

    #[test]
    fn test_package_development() {
        let r_code = r#"
#' Calculate mean with NA handling
#'
#' @param x Numeric vector
#' @param na_rm Remove NA values?
#' @return Numeric mean value
#' @export
#' @examples
#' safe_mean(c(1, 2, NA), na_rm = TRUE)
safe_mean <- function(x, na_rm = TRUE) {
  if (na_rm) {
    mean(x, na.rm = TRUE)
  } else {
    mean(x)
  }
}

#' @importFrom dplyr filter select
#' @importFrom ggplot2 ggplot aes
NULL
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 1, "Should extract documented function");
    }

    #[test]
    fn test_quosure_operations() {
        let r_code = r#"
library(rlang)

# Create quosures
my_quo <- quo(x + y)
my_quos <- quos(a = x, b = y + 2)

# Evaluate quosures
eval_result <- eval_tidy(my_quo, data = df)

# Quosure manipulation
new_quo <- quo_set_expr(my_quo, expr(x * 2))
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 4, "Should extract quosure operations");
    }

    #[test]
    fn test_advanced_dplyr() {
        let r_code = r#"
# Scoped verbs
summary_all <- df %>% summarise_all(mean)
summary_if <- df %>% summarise_if(is.numeric, mean)
summary_at <- df %>% summarise_at(vars(x, y), list(mean, sd))

# Window functions
ranked <- df %>%
  group_by(category) %>%
  mutate(
    rank = row_number(desc(value)),
    percentile = percent_rank(value),
    cumsum = cumsum(value)
  )

# Cross joins and set operations
cross <- crossing(x = 1:3, y = letters[1:3])
combined <- bind_rows(df1, df2)
unique_rows <- distinct(df, category, .keep_all = TRUE)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 6,
            "Should extract advanced dplyr operations"
        );
    }
}
