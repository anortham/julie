// R Classes Tests
// Tests for S3, S4, R6 class systems

use super::*;
use crate::base::{Symbol, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_str<'a>(symbol: &'a Symbol, key: &str) -> Option<&'a str> {
        symbol
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(key))
            .and_then(|value| value.as_str())
    }

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
#' Student model for registrar records
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

        let student = symbols
            .iter()
            .find(|s| s.name == "Student" && s.kind == SymbolKind::Class)
            .expect("Student S4 class should be extracted");
        assert_eq!(metadata_str(student, "r_class_system"), Some("S4"));
        assert_eq!(
            student.doc_comment.as_deref(),
            Some("#' Student model for registrar records")
        );
        let slots = metadata_str(student, "slots").expect("Student should record S4 slots");
        assert!(slots.contains("name"));
        assert!(slots.contains("age"));
        assert!(slots.contains("gpa"));

        let grad_student = symbols
            .iter()
            .find(|s| s.name == "GradStudent" && s.kind == SymbolKind::Class)
            .expect("GradStudent S4 class should be extracted");
        assert_eq!(metadata_str(grad_student, "r_class_system"), Some("S4"));
        assert_eq!(metadata_str(grad_student, "contains"), Some("Student"));
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

        let display_generic = symbols
            .iter()
            .find(|s| s.name == "display" && s.kind == SymbolKind::Function)
            .expect("display S4 generic should be extracted");
        assert_eq!(metadata_str(display_generic, "r_class_system"), Some("S4"));
        assert_eq!(metadata_str(display_generic, "s4_role"), Some("generic"));

        let display_student = symbols
            .iter()
            .find(|s| s.name == "display,Student" && s.kind == SymbolKind::Method)
            .expect("display Student S4 method should be extracted");
        assert_eq!(metadata_str(display_student, "r_class_system"), Some("S4"));
        assert_eq!(metadata_str(display_student, "s4_generic"), Some("display"));
        assert_eq!(metadata_str(display_student, "s4_class"), Some("Student"));

        let show_student = symbols
            .iter()
            .find(|s| s.name == "show,Student" && s.kind == SymbolKind::Method)
            .expect("show Student S4 method should be extracted");
        assert_eq!(metadata_str(show_student, "s4_generic"), Some("show"));
        assert_eq!(metadata_str(show_student, "s4_class"), Some("Student"));
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

        let person = symbols
            .iter()
            .find(|s| s.name == "Person" && s.kind == SymbolKind::Class)
            .expect("Person R6 class should be extracted as a class");
        assert_eq!(metadata_str(person, "r_class_system"), Some("R6"));

        let greet = symbols
            .iter()
            .find(|s| s.name == "greet" && s.kind == SymbolKind::Method)
            .expect("R6 public method should be extracted");
        assert_eq!(greet.parent_id.as_deref(), Some(person.id.as_str()));
        assert_eq!(metadata_str(greet, "member_visibility"), Some("public"));

        let john = symbols
            .iter()
            .find(|s| s.name == "john" && s.kind == SymbolKind::Variable)
            .expect("R6 instance assignment should remain a variable");
        assert_ne!(john.id, person.id);
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

        let account = symbols
            .iter()
            .find(|s| s.name == "BankAccount" && s.kind == SymbolKind::Class)
            .expect("BankAccount R6 class should be extracted");
        assert_eq!(metadata_str(account, "r_class_system"), Some("R6"));

        let balance = symbols
            .iter()
            .find(|s| s.name == "balance" && s.kind == SymbolKind::Field)
            .expect("R6 private field should be extracted");
        assert_eq!(balance.parent_id.as_deref(), Some(account.id.as_str()));
        assert_eq!(metadata_str(balance, "member_visibility"), Some("private"));

        let deposit = symbols
            .iter()
            .find(|s| s.name == "deposit" && s.kind == SymbolKind::Method)
            .expect("R6 public method should be extracted");
        assert_eq!(deposit.parent_id.as_deref(), Some(account.id.as_str()));
        assert_eq!(metadata_str(deposit, "member_visibility"), Some("public"));
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

        let person = symbols
            .iter()
            .find(|s| s.name == "Person" && s.kind == SymbolKind::Class)
            .expect("Reference class should be extracted as a class");
        assert_eq!(
            metadata_str(person, "r_class_system"),
            Some("ReferenceClass")
        );

        let greet = symbols
            .iter()
            .find(|s| s.name == "greet" && s.kind == SymbolKind::Method)
            .expect("Reference class method should be extracted");
        assert_eq!(greet.parent_id.as_deref(), Some(person.id.as_str()));

        let person1 = symbols
            .iter()
            .find(|s| s.name == "person1" && s.kind == SymbolKind::Variable)
            .expect("Reference class instance should remain a variable");
        assert_ne!(person1.id, person.id);
    }
}
