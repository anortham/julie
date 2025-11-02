// R Control Flow Tests
// Tests for if/else, loops, vectorized operations

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_if_else_statements() {
        let r_code = r#"
# Simple if/else
check_value <- function(x) {
  if (x > 10) {
    print("Large")
  } else if (x > 5) {
    print("Medium")
  } else {
    print("Small")
  }
}

# Inline if/else (ifelse function)
result <- ifelse(x > 0, "positive", "negative")
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 1, "Should extract check_value function");
    }

    #[test]
    fn test_for_loops() {
        let r_code = r#"
# Basic for loop
for (i in 1:10) {
  print(i^2)
}

# For loop with vector
names <- c("Alice", "Bob", "Charlie")
for (name in names) {
  print(paste("Hello", name))
}

# Nested for loops
matrix_result <- matrix(0, nrow = 3, ncol = 3)
for (i in 1:3) {
  for (j in 1:3) {
    matrix_result[i, j] <- i * j
  }
}
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract names and matrix_result");
    }

    #[test]
    fn test_while_loops() {
        let r_code = r#"
# While loop
x <- 1
while (x < 100) {
  x <- x * 2
}

# While with condition check
count <- 0
while (TRUE) {
  count <- count + 1
  if (count > 10) break
}
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 2, "Should extract x and count");
    }

    #[test]
    fn test_repeat_and_break() {
        let r_code = r#"
# Repeat loop with break
counter <- 0
repeat {
  counter <- counter + 1
  if (counter > 10) break
}

# Using next (continue)
for (i in 1:10) {
  if (i %% 2 == 0) next
  print(i)  # Only odd numbers
}
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 1, "Should extract counter variable");
    }

    #[test]
    fn test_vectorized_operations() {
        let r_code = r#"
# Vectorized operations (R's strength)
x <- 1:10
y <- x^2       # Vectorized squaring
z <- x * 2     # Vectorized multiplication
w <- x + y     # Vectorized addition

# Element-wise comparison
flags <- x > 5  # Returns logical vector

# Vectorized functions
sums <- cumsum(x)
means <- cummean(x)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 7, "Should extract all vectorized operation results");
    }

    #[test]
    fn test_apply_family() {
        let r_code = r#"
# lapply - returns list
list_result <- lapply(1:5, function(x) x^2)

# sapply - returns vector
vec_result <- sapply(1:10, function(i) {
  if (i %% 2 == 0) "even" else "odd"
})

# apply - for matrices
mat <- matrix(1:12, nrow = 3)
row_sums <- apply(mat, 1, sum)
col_sums <- apply(mat, 2, sum)

# mapply - multivariate apply
combined <- mapply(function(x, y) x + y, 1:5, 6:10)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 6, "Should extract all apply results");
    }

    #[test]
    fn test_switch_statement() {
        let r_code = r#"
# Switch statement
get_greeting <- function(language) {
  switch(language,
    english = "Hello",
    spanish = "Hola",
    french = "Bonjour",
    german = "Guten Tag",
    "Unknown language"
  )
}

# Numeric switch
value <- switch(2,
  "first",
  "second",
  "third"
)
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 1, "Should extract get_greeting function");
    }

    #[test]
    fn test_tryCatch_error_handling() {
        let r_code = r#"
# tryCatch for error handling
safe_divide <- function(a, b) {
  tryCatch(
    {
      result <- a / b
      return(result)
    },
    error = function(e) {
      message("Error in division: ", e$message)
      return(NA)
    },
    warning = function(w) {
      message("Warning: ", w$message)
    }
  )
}

# withCallingHandlers
logged_operation <- withCallingHandlers(
  {
    x <- risky_function()
  },
  error = function(e) {
    log_error(e)
  }
)
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 1, "Should extract safe_divide function");
    }
}
