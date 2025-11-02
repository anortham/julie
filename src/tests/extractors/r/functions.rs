// R Functions Tests
// Tests for function definitions, parameters, closures

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_function_definition() {
        let r_code = r#"
# Simple function
add <- function(a, b) {
  return(a + b)
}

# Function without return statement (implicit return)
multiply <- function(x, y) {
  x * y
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should extract add and multiply functions");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"add"), "Should find add function");
        assert!(func_names.contains(&"multiply"), "Should find multiply function");
    }

    #[test]
    fn test_function_with_default_parameters() {
        let r_code = r#"
# Function with default parameters
greet <- function(name = "World", greeting = "Hello") {
  paste(greeting, name)
}

# Mix of default and required parameters
calculate <- function(x, y = 10, z = 5) {
  x + y + z
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should extract greet and calculate functions");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"greet"), "Should find greet function");
        assert!(func_names.contains(&"calculate"), "Should find calculate function");
    }

    #[test]
    fn test_function_with_ellipsis() {
        let r_code = r#"
# Function with ... (ellipsis) for variable arguments
my_func <- function(...) {
  args <- list(...)
  sum(unlist(args))
}

# Function with named params and ellipsis
wrapper <- function(x, y, ...) {
  other_args <- list(...)
  do.call(some_function, c(list(x, y), other_args))
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should extract functions with ellipsis");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"my_func"), "Should find my_func");
        assert!(func_names.contains(&"wrapper"), "Should find wrapper function");
    }

    #[test]
    fn test_anonymous_functions() {
        let r_code = r#"
# Anonymous function in lapply
result1 <- lapply(1:5, function(x) x^2)

# Anonymous function in sapply
result2 <- sapply(1:10, function(i) {
  if (i %% 2 == 0) "even" else "odd"
})

# Anonymous function in Map
result3 <- Map(function(x, y) x + y, 1:5, 6:10)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 3, "Should extract result variables");

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"result1"), "Should find result1 from lapply");
        assert!(var_names.contains(&"result2"), "Should find result2 from sapply");
        assert!(var_names.contains(&"result3"), "Should find result3 from Map");
    }

    #[test]
    fn test_function_returning_multiple_values() {
        let r_code = r#"
# Function returning multiple values via list
stats <- function(x) {
  list(
    mean = mean(x),
    median = median(x),
    sd = sd(x),
    min = min(x),
    max = max(x)
  )
}

# Function with structured return
analyze_data <- function(data) {
  result <- list(
    summary = summary(data),
    correlation = cor(data),
    plot = plot(data)
  )
  return(result)
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should extract stats and analyze_data functions");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"stats"), "Should find stats function");
        assert!(func_names.contains(&"analyze_data"), "Should find analyze_data function");
    }

    #[test]
    fn test_nested_functions() {
        let r_code = r#"
# Nested function definition (closure)
outer <- function(x) {
  inner <- function(y) {
    x + y  # Accesses x from outer scope
  }
  return(inner)
}

# Function factory
make_adder <- function(n) {
  function(x) {
    x + n
  }
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should extract outer functions at minimum");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"outer"), "Should find outer function");
        assert!(func_names.contains(&"make_adder"), "Should find make_adder function");
    }

    #[test]
    fn test_recursive_function() {
        let r_code = r#"
# Recursive factorial
factorial <- function(n) {
  if (n <= 1) {
    return(1)
  } else {
    return(n * factorial(n - 1))
  }
}

# Recursive fibonacci
fibonacci <- function(n) {
  if (n <= 2) return(1)
  fibonacci(n - 1) + fibonacci(n - 2)
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should extract factorial and fibonacci");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"factorial"), "Should find factorial function");
        assert!(func_names.contains(&"fibonacci"), "Should find fibonacci function");
    }

    #[test]
    fn test_function_with_side_effects() {
        let r_code = r#"
# Function that modifies global state
counter <- 0
increment <- function() {
  counter <<- counter + 1  # Super assignment to modify global
  counter
}

# Function with print side effects
log_message <- function(msg, level = "INFO") {
  timestamp <- Sys.time()
  cat(sprintf("[%s] %s: %s\n", timestamp, level, msg))
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 2, "Should extract increment and log_message");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"increment"), "Should find increment function");
        assert!(func_names.contains(&"log_message"), "Should find log_message function");
    }

    #[test]
    fn test_function_composition() {
        let r_code = r#"
# Function composition
square <- function(x) x^2
double <- function(x) x * 2

# Compose functions
square_then_double <- function(x) {
  double(square(x))
}

# Pipeline style composition
process <- function(x) {
  x |>
    square() |>
    double() |>
    log()
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 4, "Should extract all composed functions");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"square"), "Should find square function");
        assert!(func_names.contains(&"double"), "Should find double function");
        assert!(func_names.contains(&"square_then_double"), "Should find composed function");
    }

    #[test]
    fn test_function_with_complex_parameters() {
        let r_code = r#"
# Function with formula parameter
model_fit <- function(formula, data, method = "lm") {
  if (method == "lm") {
    lm(formula, data = data)
  } else {
    glm(formula, data = data)
  }
}

# Function with function parameter (callback)
apply_operation <- function(data, operation = mean) {
  operation(data)
}

# Function with environment parameter
with_env <- function(expr, envir = parent.frame()) {
  eval(expr, envir = envir)
}
"#;

        let symbols = extract_symbols(r_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(functions.len() >= 3, "Should extract all parameter-complex functions");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(func_names.contains(&"model_fit"), "Should find model_fit function");
        assert!(func_names.contains(&"apply_operation"), "Should find apply_operation function");
        assert!(func_names.contains(&"with_env"), "Should find with_env function");
    }
}
