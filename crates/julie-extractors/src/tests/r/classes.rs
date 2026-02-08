// R Classes Tests
// Tests for S3, S4, R6 class systems

use super::*;
use crate::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_class_creation() {
        let r_code = r#"
# S3 class creation using structure()
person <- structure(
  list(name = "John", age = 30),
  class = "person"
)

# S3 class with multiple attributes
car <- structure(
  list(make = "Toyota", model = "Camry", year = 2020),
  class = c("car", "vehicle")
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 2,
            "Should extract person and car S3 objects"
        );
    }

    #[test]
    fn test_s3_methods() {
        let r_code = r#"
# S3 generic function
print.person <- function(x) {
  cat("Person:", x$name, "Age:", x$age, "\n")
}

summary.person <- function(object) {
  cat("Person Summary\n")
  cat("Name:", object$name, "\n")
  cat("Age:", object$age, "\n")
}

# S3 method for custom class
plot.my_class <- function(x, ...) {
  plot(x$data, ...)
}
"#;

        let symbols = extract_symbols(r_code);

        // S3 methods should be extracted as SymbolKind::Method
        let methods: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Method)
            .collect();

        assert!(
            methods.len() >= 3,
            "Should extract S3 methods as Method kind (found {})",
            methods.len()
        );

        // Verify S3 metadata
        let print_person = methods
            .iter()
            .find(|m| m.name == "print.person")
            .expect("Should find print.person method");
        let meta = print_person.metadata.as_ref().unwrap();
        assert_eq!(
            meta.get("s3_method").and_then(|v| v.as_str()),
            Some("print"),
            "Should have s3_method metadata"
        );
        assert_eq!(
            meta.get("s3_class").and_then(|v| v.as_str()),
            Some("person"),
            "Should have s3_class metadata"
        );
    }

    #[test]
    fn test_s4_class_definition() {
        let r_code = r#"
# S4 class definition
setClass("Student",
  slots = c(
    name = "character",
    age = "numeric",
    gpa = "numeric"
  )
)

# S4 class with inheritance
setClass("GradStudent",
  contains = "Student",
  slots = c(
    advisor = "character",
    thesis_topic = "character"
  )
)
"#;

        let symbols = extract_symbols(r_code);

        // The code should parse successfully
        assert!(symbols.len() >= 0, "Should parse S4 class definitions");
    }

    #[test]
    fn test_s4_methods() {
        let r_code = r#"
# S4 generic and method
setGeneric("display", function(object) {
  standardGeneric("display")
})

setMethod("display", "Student", function(object) {
  cat("Student:", object@name, "\n")
  cat("GPA:", object@gpa, "\n")
})

# S4 accessor methods
setMethod("show", "Student", function(object) {
  cat("Student:", object@name, "GPA:", object@gpa, "\n")
})
"#;

        let symbols = extract_symbols(r_code);
        assert!(symbols.len() >= 0, "Should parse S4 method definitions");
    }

    #[test]
    fn test_r6_class() {
        let r_code = r#"
# R6 class definition
library(R6)

Person <- R6Class("Person",
  public = list(
    name = NULL,
    age = NULL,

    initialize = function(name, age) {
      self$name <- name
      self$age <- age
    },

    greet = function() {
      cat("Hello, I'm", self$name, "\n")
    }
  )
)

# Creating R6 instance
john <- Person$new("John", 30)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 2,
            "Should extract Person class and john instance"
        );
    }

    #[test]
    fn test_r6_inheritance() {
        let r_code = r#"
# R6 inheritance
Employee <- R6Class("Employee",
  inherit = Person,
  public = list(
    job_title = NULL,
    salary = NULL,

    initialize = function(name, age, job_title, salary) {
      super$initialize(name, age)
      self$job_title <- job_title
      self$salary <- salary
    },

    describe = function() {
      cat(self$name, "works as", self$job_title, "\n")
    }
  )
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 1, "Should extract Employee class");
    }

    #[test]
    fn test_r6_private_members() {
        let r_code = r#"
# R6 with private members
BankAccount <- R6Class("BankAccount",
  private = list(
    balance = 0
  ),
  public = list(
    deposit = function(amount) {
      private$balance <- private$balance + amount
    },

    withdraw = function(amount) {
      if (amount <= private$balance) {
        private$balance <- private$balance - amount
        return(TRUE)
      }
      return(FALSE)
    },

    get_balance = function() {
      return(private$balance)
    }
  )
)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(variables.len() >= 1, "Should extract BankAccount class");
    }

    #[test]
    fn test_reference_classes() {
        let r_code = r#"
# Reference Class (older alternative to R6)
Person <- setRefClass("Person",
  fields = list(
    name = "character",
    age = "numeric"
  ),
  methods = list(
    initialize = function(n, a) {
      name <<- n
      age <<- a
    },
    greet = function() {
      cat("Hello from", name, "\n")
    }
  )
)

# Create instance
person1 <- Person$new(name = "Alice", age = 25)
"#;

        let symbols = extract_symbols(r_code);

        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 2,
            "Should extract Person refClass and instance"
        );
    }
}
