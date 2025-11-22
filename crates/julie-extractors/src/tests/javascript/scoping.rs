//! Variable Scoping and Hoisting Tests for JavaScript
//!
//! Tests for JavaScript variable scoping behavior including:
//! - Function hoisting (declarations vs expressions)
//! - Var hoisting and function scope
//! - Let/const block scoping and temporal dead zone
//! - Closures with different scoping behaviors
//! - var vs let/const in loops

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
fn test_handle_var_hoisting_let_const_block_scope_and_function_hoisting() {
    let code = r#"
// Function hoisting - can be called before declaration
console.log(hoistedFunction()); // Works

function hoistedFunction() {
  return 'I am hoisted!';
}

// Var hoisting
console.log(hoistedVar); // undefined, not error
var hoistedVar = 'Now I have a value';

// Let/const temporal dead zone
function scopingExample() {
  // console.log(blockScoped); // Would throw ReferenceError

  let blockScoped = 'let variable';
  const constantValue = 'const variable';

  if (true) {
    let blockScoped = 'different block scoped'; // Shadows outer
    const anotherConstant = 'block constant';
    var functionScoped = 'var in block'; // Hoisted to function scope

    function innerFunction() {
      return blockScoped + ' from inner';
    }

    console.log(innerFunction());
  }

  console.log(functionScoped); // Accessible due to var hoisting
  // console.log(anotherConstant); // Would throw ReferenceError
}

// Function expressions are not hoisted
// console.log(notHoisted()); // Would throw TypeError

var notHoisted = function() {
  return 'Function expression';
};

const alsoNotHoisted = function() {
  return 'Function expression with const';
};

// Arrow functions are not hoisted
const arrowNotHoisted = () => {
  return 'Arrow function';
};

// Different scoping behaviors
function demonstrateScoping() {
  // All these create function-scoped variables
  for (var i = 0; i < 3; i++) {
    setTimeout(function() {
      console.log('var:', i); // Prints 3, 3, 3
    }, 100);
  }

  // Block-scoped variables
  for (let j = 0; j < 3; j++) {
    setTimeout(function() {
      console.log('let:', j); // Prints 0, 1, 2
    }, 200);
  }

  // Const in loops
  for (const k of [0, 1, 2]) {
    setTimeout(function() {
      console.log('const:', k); // Prints 0, 1, 2
    }, 300);
  }
}

// Closure with different scoping
function createClosures() {
  const closures = [];

  // Problematic with var
  for (var m = 0; m < 3; m++) {
    closures.push(function() {
      return m; // All return 3
    });
  }

  // Fixed with let
  for (let n = 0; n < 3; n++) {
    closures.push(function() {
      return n; // Returns 0, 1, 2 respectively
    });
  }

  // IIFE solution for var
  for (var p = 0; p < 3; p++) {
    closures.push((function(index) {
      return function() {
        return index;
      };
    })(p));
  }

  return closures;
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "scoping.js".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // Hoisted function declaration
    let hoisted_function = symbols.iter().find(|s| s.name == "hoistedFunction");
    assert!(hoisted_function.is_some());
    assert_eq!(hoisted_function.unwrap().kind, SymbolKind::Function);
    assert!(
        hoisted_function
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function hoistedFunction()")
    );

    // Var declaration
    let hoisted_var = symbols.iter().find(|s| s.name == "hoistedVar");
    assert!(hoisted_var.is_some());
    assert_eq!(hoisted_var.unwrap().kind, SymbolKind::Variable);

    // Function with block scoping
    let scoping_example = symbols.iter().find(|s| s.name == "scopingExample");
    assert!(scoping_example.is_some());
    assert!(
        scoping_example
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function scopingExample()")
    );

    // Inner function
    let inner_function = symbols.iter().find(|s| s.name == "innerFunction");
    assert!(inner_function.is_some());
    assert_eq!(inner_function.unwrap().kind, SymbolKind::Function);

    // Function expressions
    let not_hoisted = symbols.iter().find(|s| s.name == "notHoisted");
    assert!(not_hoisted.is_some());
    assert!(
        not_hoisted
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("var notHoisted = function()")
    );

    let also_not_hoisted = symbols.iter().find(|s| s.name == "alsoNotHoisted");
    assert!(also_not_hoisted.is_some());
    assert!(
        also_not_hoisted
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const alsoNotHoisted = function()")
    );

    // Arrow function
    let arrow_not_hoisted = symbols.iter().find(|s| s.name == "arrowNotHoisted");
    assert!(arrow_not_hoisted.is_some());
    assert!(
        arrow_not_hoisted
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const arrowNotHoisted = () =>")
    );

    // Scoping demonstration function
    let demonstrate_scoping = symbols.iter().find(|s| s.name == "demonstrateScoping");
    assert!(demonstrate_scoping.is_some());

    // Closure creation function
    let create_closures = symbols.iter().find(|s| s.name == "createClosures");
    assert!(create_closures.is_some());
    assert!(
        create_closures
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function createClosures()")
    );
}
