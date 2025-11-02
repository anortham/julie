// R Tidyverse Tests
// Tests for %>% pipes, dplyr verbs, ggplot2 patterns

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magrittr_pipes() {
        let r_code = r#"
library(dplyr)

# Basic pipe usage
result <- df %>%
  filter(age > 25) %>%
  select(name, age) %>%
  arrange(desc(age))

# Pipe with dot placeholder
data %>%
  lm(y ~ x, data = .) %>%
  summary()
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 1, "Should extract result variable");
    }

    #[test]
    fn test_dplyr_verbs() {
        let r_code = r#"
library(dplyr)

# filter
filtered <- df %>% filter(age > 25, score >= 80)

# select
selected <- df %>% select(name, age, score)

# mutate
mutated <- df %>% mutate(
  grade = ifelse(score >= 90, "A", "B"),
  age_group = cut(age, breaks = c(0, 30, 60, 100))
)

# summarize
summary_stats <- df %>%
  group_by(category) %>%
  summarize(
    avg_score = mean(score),
    total = n()
  )
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 4, "Should extract all dplyr operation results");
    }

    #[test]
    fn test_dplyr_joins() {
        let r_code = r#"
# Inner join
combined <- df1 %>%
  inner_join(df2, by = "id")

# Left join
left_result <- df1 %>%
  left_join(df2, by = c("user_id" = "id"))

# Multiple joins
multi_join <- df1 %>%
  left_join(df2, by = "id") %>%
  left_join(df3, by = "id")
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 3, "Should extract all join results");
    }

    #[test]
    fn test_ggplot2_basics() {
        let r_code = r#"
library(ggplot2)

# Basic plot
p1 <- ggplot(data, aes(x = age, y = score)) +
  geom_point() +
  geom_smooth(method = "lm") +
  labs(title = "Age vs Score")

# Histogram
p2 <- ggplot(data, aes(x = age)) +
  geom_histogram(binwidth = 5, fill = "blue", alpha = 0.7) +
  theme_minimal()
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract p1 and p2 plots");
    }

    #[test]
    fn test_ggplot2_facets() {
        let r_code = r#"
# Facet wrap
plot_wrap <- ggplot(data, aes(x, y)) +
  geom_point() +
  facet_wrap(~category)

# Facet grid
plot_grid <- ggplot(data, aes(x, y)) +
  geom_point() +
  facet_grid(rows = vars(region), cols = vars(year))
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract faceted plots");
    }

    #[test]
    fn test_tidyr_reshape() {
        let r_code = r#"
library(tidyr)

# Pivot longer
long_data <- df %>%
  pivot_longer(
    cols = c(q1, q2, q3, q4),
    names_to = "quarter",
    values_to = "sales"
  )

# Pivot wider
wide_data <- df %>%
  pivot_wider(
    names_from = category,
    values_from = value
  )

# Separate
separated <- df %>%
  separate(full_name, into = c("first", "last"), sep = " ")
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 3, "Should extract tidyr reshape results");
    }

    #[test]
    fn test_purrr_map() {
        let r_code = r#"
library(purrr)

# map - returns list
list_result <- map(1:5, ~ .x * 2)

# map_dbl - returns numeric vector
vec_result <- map_dbl(list(1, 2, 3), ~ .x^2)

# map2 - iterate over two lists
combined <- map2(x_vals, y_vals, ~ .x + .y)

# pmap - iterate over multiple lists
multi_result <- pmap(
  list(a = 1:3, b = 4:6, c = 7:9),
  function(a, b, c) a + b + c
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 4, "Should extract purrr map results");
    }

    #[test]
    fn test_tidyverse_pipeline() {
        let r_code = r#"
# Complex tidyverse pipeline
analysis_result <- raw_data %>%
  filter(!is.na(value)) %>%
  mutate(
    normalized = (value - mean(value)) / sd(value),
    category = cut(normalized, breaks = 3)
  ) %>%
  group_by(category) %>%
  summarize(
    count = n(),
    avg = mean(value),
    sd = sd(value)
  ) %>%
  arrange(desc(avg)) %>%
  mutate(rank = row_number())
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 1, "Should extract analysis_result");
    }

    #[test]
    fn test_stringr_operations() {
        let r_code = r#"
library(stringr)

# String detection
has_pattern <- str_detect(strings, "pattern")

# String extraction
extracted <- str_extract(strings, "\\d+")

# String replacement
cleaned <- str_replace_all(strings, "[^A-Za-z]", "")

# String manipulation
result <- strings %>%
  str_trim() %>%
  str_to_lower() %>%
  str_replace_all("\\s+", "_")
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 4, "Should extract stringr operation results");
    }

    #[test]
    fn test_readr_io() {
        let r_code = r#"
library(readr)

# Read CSV
data <- read_csv("file.csv")

# Read with column specification
typed_data <- read_csv(
  "file.csv",
  col_types = cols(
    id = col_integer(),
    name = col_character(),
    date = col_date(format = "%Y-%m-%d")
  )
)

# Write CSV
data %>% write_csv("output.csv")
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract data and typed_data");
    }

    #[test]
    fn test_forcats_factors() {
        let r_code = r#"
library(forcats)

# Reorder factors
reordered <- fct_reorder(categories, values)

# Lump rare levels
lumped <- fct_lump(categories, n = 5)

# Recode factors
recoded <- fct_recode(
  levels,
  "High" = "h",
  "Medium" = "m",
  "Low" = "l"
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 3, "Should extract forcats operation results");
    }

    #[test]
    fn test_native_pipe() {
        let r_code = r#"
# R 4.1+ native pipe |>
result <- data |>
  filter(age > 25) |>
  select(name, age) |>
  arrange(age)

# Mix of native and magrittr pipes (not recommended but valid)
mixed <- data |>
  filter(x > 0) %>%
  mutate(y = x * 2) |>
  summarize(total = sum(y))
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract result and mixed");
    }
}
