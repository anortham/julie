//! Legacy JavaScript Patterns Tests
//!
//! Tests for classic JavaScript patterns including:
//! - Constructor functions and prototypes
//! - IIFE (Immediately Invoked Function Expressions)
//! - Traditional function expressions (var, function keyword)
//! - Object literals with methods
//! - Closure-based private variables

use crate::base::SymbolKind;
use crate::javascript::JavaScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Error loading JavaScript grammar");
    parser
}

#[test]
fn test_extract_function_declarations_prototypes_and_iife() {
    let code = r#"
// Function declarations
function Calculator(initialValue) {
  this.value = initialValue || 0;
  this.history = [];
}

// Prototype methods
Calculator.prototype.add = function(num) {
  this.value += num;
  this.history.push(`+${num}`);
  return this;
};

Calculator.prototype.subtract = function(num) {
  this.value -= num;
  this.history.push(`-${num}`);
  return this;
};

// Static method on constructor
Calculator.create = function(initialValue) {
  return new Calculator(initialValue);
};

// IIFE (Immediately Invoked Function Expression)
const MathUtils = (function() {
  const PI = 3.14159;
  let precision = 2;

  function roundToPrecision(num) {
    return Math.round(num * Math.pow(10, precision)) / Math.pow(10, precision);
  }

  return {
    constants: {
      PI: PI,
      E: 2.71828
    },

    area: {
      circle: function(radius) {
        return roundToPrecision(PI * radius * radius);
      },
      rectangle: function(width, height) {
        return roundToPrecision(width * height);
      }
    },

    setPrecision: function(newPrecision) {
      precision = Math.max(0, newPrecision);
    },

    getPrecision: function() {
      return precision;
    }
  };
})();

// Traditional function expressions
var multiply = function(a, b) {
  return a * b;
};

var divide = function divide(a, b) {
  if (b === 0) {
    throw new Error('Division by zero');
  }
  return a / b;
};

// Object literal with methods
const ApiClient = {
  baseUrl: 'https://api.example.com',
  timeout: 5000,

  get: function(endpoint) {
    return this.request('GET', endpoint);
  },

  post: function(endpoint, data) {
    return this.request('POST', endpoint, data);
  },

  request: function(method, endpoint, data) {
    // Implementation
    return Promise.resolve({ method, endpoint, data });
  }
};

// Constructor function with closure
function Counter(initialValue) {
  let count = initialValue || 0;

  this.increment = function() {
    return ++count;
  };

  this.decrement = function() {
    return --count;
  };

  this.getValue = function() {
    return count;
  };

  this.reset = function() {
    count = initialValue || 0;
    return count;
  };
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "legacy.js".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // Constructor function
    let calculator = symbols.iter().find(|s| s.name == "Calculator");
    assert!(calculator.is_some());
    assert_eq!(calculator.unwrap().kind, SymbolKind::Function);
    assert!(
        calculator
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function Calculator(initialValue)")
    );

    // Prototype methods
    let prototype_add = symbols
        .iter()
        .find(|s| s.name == "add" && s.signature.as_ref().unwrap().contains("prototype"));
    assert!(prototype_add.is_some());
    assert_eq!(prototype_add.unwrap().kind, SymbolKind::Method);
    assert!(
        prototype_add
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("Calculator.prototype.add = function(num)")
    );

    let prototype_subtract = symbols
        .iter()
        .find(|s| s.name == "subtract" && s.signature.as_ref().unwrap().contains("prototype"));
    assert!(prototype_subtract.is_some());

    // Static method on constructor
    let calculator_create = symbols.iter().find(|s| {
        s.name == "create" && s.signature.as_ref().unwrap().contains("Calculator.create")
    });
    assert!(calculator_create.is_some());
    assert_eq!(calculator_create.unwrap().kind, SymbolKind::Method);

    // IIFE variable
    let math_utils = symbols.iter().find(|s| s.name == "MathUtils");
    assert!(math_utils.is_some());
    assert_eq!(math_utils.unwrap().kind, SymbolKind::Variable);

    // Functions inside IIFE
    let round_to_precision = symbols.iter().find(|s| s.name == "roundToPrecision");
    assert!(round_to_precision.is_some());
    assert_eq!(round_to_precision.unwrap().kind, SymbolKind::Function);

    // Function expressions
    let multiply_fn = symbols
        .iter()
        .find(|s| s.name == "multiply" && s.signature.as_ref().unwrap().contains("var multiply"));
    assert!(multiply_fn.is_some());
    assert_eq!(multiply_fn.unwrap().kind, SymbolKind::Function);

    let divide_fn = symbols
        .iter()
        .find(|s| s.name == "divide" && s.signature.as_ref().unwrap().contains("function divide"));
    assert!(divide_fn.is_some());

    // Object literal
    let api_client = symbols.iter().find(|s| s.name == "ApiClient");
    assert!(api_client.is_some());
    assert_eq!(api_client.unwrap().kind, SymbolKind::Variable);

    // Object methods
    let get_method = symbols
        .iter()
        .find(|s| s.name == "get" && s.parent_id == Some(api_client.unwrap().id.clone()));
    assert!(get_method.is_some());
    assert_eq!(get_method.unwrap().kind, SymbolKind::Method);

    let post_method = symbols
        .iter()
        .find(|s| s.name == "post" && s.parent_id == Some(api_client.unwrap().id.clone()));
    assert!(post_method.is_some());

    // Constructor with closure
    let counter = symbols.iter().find(|s| s.name == "Counter");
    assert!(counter.is_some());
    assert!(
        counter
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function Counter(initialValue)")
    );

    // Closure methods
    let increment = symbols.iter().find(|s| {
        s.name == "increment" && s.signature.as_ref().unwrap().contains("this.increment")
    });
    assert!(increment.is_some());
    assert_eq!(increment.unwrap().kind, SymbolKind::Method);
}
