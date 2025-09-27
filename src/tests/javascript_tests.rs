// JavaScript Extractor Tests
//
// Direct port of Miller's JavaScript extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/javascript-extractor.test.ts

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::javascript::JavaScriptExtractor;
use tree_sitter::Parser;

/// Initialize JavaScript parser for JavaScript files
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Error loading JavaScript grammar");
    parser
}

#[cfg(test)]
mod javascript_extractor_tests {
    use super::*;

    mod modern_javascript_features {
        use super::*;

        #[test]
        fn test_extract_es6_plus_features() {
            let code = r#"
// ES6 Imports/Exports
import { debounce, throttle } from 'lodash';
import React, { useState, useEffect } from 'react';
export { default as Component } from './Component';
export const API_URL = 'https://api.example.com';

// Arrow functions
const add = (a, b) => a + b;
const multiply = (x, y) => {
  return x * y;
};

// Async/await functions
async function fetchData(url) {
  try {
    const response = await fetch(url);
    return await response.json();
  } catch (error) {
    console.error('Fetch failed:', error);
    throw error;
  }
}

const asyncArrow = async (id) => {
  const data = await fetchData(`/api/users/${id}`);
  return data;
};

// Generator functions
function* fibonacci() {
  let [a, b] = [0, 1];
  while (true) {
    yield a;
    [a, b] = [b, a + b];
  }
}

const generatorArrow = function* (items) {
  for (const item of items) {
    yield item.toUpperCase();
  }
};

// Classes with modern features
class EventEmitter {
  #listeners = new Map(); // Private field

  constructor(options = {}) {
    this.maxListeners = options.maxListeners || 10;
  }

  // Static method
  static create(options) {
    return new EventEmitter(options);
  }

  // Getter/setter
  get listenerCount() {
    return this.#listeners.size;
  }

  set maxListeners(value) {
    this._maxListeners = Math.max(0, value);
  }

  // Async method
  async emit(event, ...args) {
    const handlers = this.#listeners.get(event) || [];
    await Promise.all(handlers.map(handler => handler(...args)));
  }

  // Private method
  #validateEvent(event) {
    if (typeof event !== 'string') {
      throw new TypeError('Event must be a string');
    }
  }
}

// Class inheritance
class AsyncEventEmitter extends EventEmitter {
  constructor(options) {
    super(options);
    this.queue = [];
  }

  async processQueue() {
    while (this.queue.length > 0) {
      const event = this.queue.shift();
      await super.emit(event.name, ...event.args);
    }
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let mut extractor = JavaScriptExtractor::new(
                "javascript".to_string(),
                "test.js".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // ES6 Imports
            let lodash_import = symbols
                .iter()
                .find(|s| s.name == "debounce" && s.kind == SymbolKind::Import);
            assert!(lodash_import.is_some());
            assert!(lodash_import
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("import { debounce, throttle } from 'lodash'"));

            let react_import = symbols
                .iter()
                .find(|s| s.name == "React" && s.kind == SymbolKind::Import);
            assert!(react_import.is_some());
            assert!(react_import
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("import React, { useState, useEffect } from 'react'"));

            // ES6 Exports
            let component_export = symbols
                .iter()
                .find(|s| s.name == "Component" && s.kind == SymbolKind::Export);
            assert!(component_export.is_some());
            assert!(component_export
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("export { default as Component }"));

            let api_url_export = symbols
                .iter()
                .find(|s| s.name == "API_URL" && s.kind == SymbolKind::Export);
            assert!(api_url_export.is_some());
            assert!(api_url_export
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("export const API_URL"));

            // Arrow functions
            let add_arrow = symbols.iter().find(|s| s.name == "add");
            assert!(add_arrow.is_some());
            assert_eq!(add_arrow.unwrap().kind, SymbolKind::Function);
            assert!(add_arrow
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const add = (a, b) => a + b"));

            let multiply_arrow = symbols.iter().find(|s| s.name == "multiply");
            assert!(multiply_arrow.is_some());
            assert!(multiply_arrow
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const multiply = (x, y) =>"));

            // Async functions
            let fetch_data = symbols.iter().find(|s| s.name == "fetchData");
            assert!(fetch_data.is_some());
            assert_eq!(fetch_data.unwrap().kind, SymbolKind::Function);
            assert!(fetch_data
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("async function fetchData(url)"));

            let async_arrow = symbols.iter().find(|s| s.name == "asyncArrow");
            assert!(async_arrow.is_some());
            assert!(async_arrow
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const asyncArrow = async (id) =>"));

            // Generator functions
            let fibonacci = symbols.iter().find(|s| s.name == "fibonacci");
            assert!(fibonacci.is_some());
            assert_eq!(fibonacci.unwrap().kind, SymbolKind::Function);
            assert!(fibonacci
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function* fibonacci()"));

            let generator_arrow = symbols.iter().find(|s| s.name == "generatorArrow");
            assert!(generator_arrow.is_some());
            assert!(generator_arrow
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const generatorArrow = function* (items)"));

            // Class with modern features
            let event_emitter = symbols.iter().find(|s| s.name == "EventEmitter");
            assert!(event_emitter.is_some());
            assert_eq!(event_emitter.unwrap().kind, SymbolKind::Class);

            // Private field
            let listeners = symbols.iter().find(|s| s.name == "#listeners");
            assert!(listeners.is_some());
            assert_eq!(listeners.unwrap().kind, SymbolKind::Field);
            assert_eq!(
                listeners.unwrap().parent_id,
                Some(event_emitter.unwrap().id.clone())
            );

            // Constructor
            let constructor = symbols.iter().find(|s| {
                s.name == "constructor" && s.parent_id == Some(event_emitter.unwrap().id.clone())
            });
            assert!(constructor.is_some());
            assert_eq!(constructor.unwrap().kind, SymbolKind::Constructor);

            // Static method
            let create_static = symbols
                .iter()
                .find(|s| s.name == "create" && s.signature.as_ref().unwrap().contains("static"));
            assert!(create_static.is_some());
            assert_eq!(create_static.unwrap().kind, SymbolKind::Method);

            // Getter/setter
            let listener_count = symbols.iter().find(|s| {
                s.name == "listenerCount" && s.signature.as_ref().unwrap().contains("get")
            });
            assert!(listener_count.is_some());
            assert_eq!(listener_count.unwrap().kind, SymbolKind::Method);

            let max_listeners_setter = symbols.iter().find(|s| {
                s.name == "maxListeners" && s.signature.as_ref().unwrap().contains("set")
            });
            assert!(max_listeners_setter.is_some());

            // Private method
            let validate_event = symbols.iter().find(|s| s.name == "#validateEvent");
            assert!(validate_event.is_some());
            assert_eq!(validate_event.unwrap().kind, SymbolKind::Method);
            assert_eq!(
                validate_event.unwrap().visibility.as_ref().unwrap(),
                &Visibility::Private
            );

            // Inheritance
            let async_event_emitter = symbols.iter().find(|s| s.name == "AsyncEventEmitter");
            assert!(async_event_emitter.is_some());
            assert!(async_event_emitter
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("extends EventEmitter"));
        }
    }

    mod legacy_javascript_patterns {
        use super::*;

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

            let mut extractor = JavaScriptExtractor::new(
                "javascript".to_string(),
                "legacy.js".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Constructor function
            let calculator = symbols.iter().find(|s| s.name == "Calculator");
            assert!(calculator.is_some());
            assert_eq!(calculator.unwrap().kind, SymbolKind::Function);
            assert!(calculator
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function Calculator(initialValue)"));

            // Prototype methods
            let prototype_add = symbols
                .iter()
                .find(|s| s.name == "add" && s.signature.as_ref().unwrap().contains("prototype"));
            assert!(prototype_add.is_some());
            assert_eq!(prototype_add.unwrap().kind, SymbolKind::Method);
            assert!(prototype_add
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Calculator.prototype.add = function(num)"));

            let prototype_subtract = symbols.iter().find(|s| {
                s.name == "subtract" && s.signature.as_ref().unwrap().contains("prototype")
            });
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
            let multiply_fn = symbols.iter().find(|s| {
                s.name == "multiply" && s.signature.as_ref().unwrap().contains("var multiply")
            });
            assert!(multiply_fn.is_some());
            assert_eq!(multiply_fn.unwrap().kind, SymbolKind::Function);

            let divide_fn = symbols.iter().find(|s| {
                s.name == "divide" && s.signature.as_ref().unwrap().contains("function divide")
            });
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
            assert!(counter
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function Counter(initialValue)"));

            // Closure methods
            let increment = symbols.iter().find(|s| {
                s.name == "increment" && s.signature.as_ref().unwrap().contains("this.increment")
            });
            assert!(increment.is_some());
            assert_eq!(increment.unwrap().kind, SymbolKind::Method);
        }
    }

    mod modern_modules_and_destructuring {
        use super::*;

        #[test]
        fn test_extract_destructuring_rest_spread_and_template_literals() {
            let code = r#"
// Destructuring imports
import { createElement as h, Fragment } from 'react';
import { connect, Provider } from 'react-redux';

// Dynamic imports
const loadModule = async () => {
  const { utils } = await import('./utils.js');
  return utils;
};

// Destructuring assignments
const user = { name: 'John', age: 30, email: 'john@example.com' };
const { name, age, ...rest } = user;
const [first, second, ...remaining] = [1, 2, 3, 4, 5];

// Destructuring parameters
function processUser({ name, age = 18, ...preferences }) {
  return {
    displayName: name.toUpperCase(),
    isAdult: age >= 18,
    preferences
  };
}

const processArray = ([head, ...tail]) => {
  return { head, tail };
};

// Rest and spread in functions
function sum(...numbers) {
  return numbers.reduce((total, num) => total + num, 0);
}

const combineArrays = (arr1, arr2, ...others) => {
  return [...arr1, ...arr2, ...others.flat()];
};

// Template literals and tagged templates
const formatUser = (user) => `
  Name: ${user.name}
  Age: ${user.age}
  Email: ${user.email || 'Not provided'}
`;

function sql(strings, ...values) {
  return strings.reduce((query, string, i) => {
    return query + string + (values[i] || '');
  }, '');
}

const query = sql`
  SELECT * FROM users
  WHERE age > ${minAge}
  AND status = ${status}
`;

// Object shorthand and computed properties
const createConfig = (env, debug = false) => {
  const apiUrl = env === 'production' ? 'https://api.prod.com' : 'https://api.dev.com';

  return {
    env,
    debug,
    apiUrl,
    [`${env}_settings`]: {
      caching: env === 'production',
      logging: debug
    },

    // Method shorthand
    init() {
      console.log(`Initializing ${env} environment`);
    },

    async connect() {
      const response = await fetch(this.apiUrl);
      return response.json();
    }
  };
};

// Default parameters and destructuring
const createUser = (
  name = 'Anonymous',
  { age = 0, email = null, preferences = {} } = {},
  ...roles
) => {
  return {
    id: Math.random().toString(36),
    name,
    age,
    email,
    preferences: { theme: 'light', ...preferences },
    roles: ['user', ...roles]
  };
};

// Async iterators and generators
async function* fetchPages(baseUrl) {
  let page = 1;
  let hasMore = true;

  while (hasMore) {
    const response = await fetch(`${baseUrl}?page=${page}`);
    const data = await response.json();

    yield data.items;

    hasMore = data.hasNextPage;
    page++;
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let mut extractor = JavaScriptExtractor::new(
                "javascript".to_string(),
                "modern.js".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Destructuring imports with aliases
            let react_import = symbols.iter().find(|s| {
                s.name == "createElement" && s.signature.as_ref().unwrap().contains("as h")
            });
            assert!(react_import.is_some());
            assert_eq!(react_import.unwrap().kind, SymbolKind::Import);

            // Dynamic import function
            let load_module = symbols.iter().find(|s| s.name == "loadModule");
            assert!(load_module.is_some());
            assert!(load_module
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const loadModule = async () =>"));

            // Destructuring variables
            let name_var = symbols.iter().find(|s| {
                s.name == "name" && s.signature.as_ref().unwrap().contains("const { name")
            });
            assert!(name_var.is_some());
            assert_eq!(name_var.unwrap().kind, SymbolKind::Variable);

            let rest_var = symbols
                .iter()
                .find(|s| s.name == "rest" && s.signature.as_ref().unwrap().contains("...rest"));
            assert!(rest_var.is_some());

            // Destructuring parameters function
            let process_user = symbols.iter().find(|s| s.name == "processUser");
            assert!(process_user.is_some());
            assert!(process_user
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function processUser({ name, age = 18, ...preferences })"));

            let process_array = symbols.iter().find(|s| s.name == "processArray");
            assert!(process_array.is_some());
            assert!(process_array
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const processArray = ([head, ...tail]) =>"));

            // Rest parameters
            let sum_fn = symbols.iter().find(|s| s.name == "sum");
            assert!(sum_fn.is_some());
            assert!(sum_fn
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function sum(...numbers)"));

            let combine_arrays = symbols.iter().find(|s| s.name == "combineArrays");
            assert!(combine_arrays.is_some());
            assert!(combine_arrays
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const combineArrays = (arr1, arr2, ...others) =>"));

            // Template literal function
            let format_user = symbols.iter().find(|s| s.name == "formatUser");
            assert!(format_user.is_some());
            assert!(format_user
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const formatUser = (user) =>"));

            // Tagged template function
            let sql_fn = symbols.iter().find(|s| s.name == "sql");
            assert!(sql_fn.is_some());
            assert!(sql_fn
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function sql(strings, ...values)"));

            // Object with computed properties and shorthand methods
            let create_config = symbols.iter().find(|s| s.name == "createConfig");
            assert!(create_config.is_some());
            assert!(create_config
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const createConfig = (env, debug = false) =>"));

            // Default parameters with destructuring
            let create_user = symbols.iter().find(|s| s.name == "createUser");
            assert!(create_user.is_some());
            assert!(create_user
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const createUser = ("));

            // Async generator
            let fetch_pages = symbols.iter().find(|s| s.name == "fetchPages");
            assert!(fetch_pages.is_some());
            assert!(fetch_pages
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("async function* fetchPages(baseUrl)"));
        }
    }

    mod hoisting_and_scoping {
        use super::*;

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

            let mut extractor = JavaScriptExtractor::new(
                "javascript".to_string(),
                "scoping.js".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Hoisted function declaration
            let hoisted_function = symbols.iter().find(|s| s.name == "hoistedFunction");
            assert!(hoisted_function.is_some());
            assert_eq!(hoisted_function.unwrap().kind, SymbolKind::Function);
            assert!(hoisted_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function hoistedFunction()"));

            // Var declaration
            let hoisted_var = symbols.iter().find(|s| s.name == "hoistedVar");
            assert!(hoisted_var.is_some());
            assert_eq!(hoisted_var.unwrap().kind, SymbolKind::Variable);

            // Function with block scoping
            let scoping_example = symbols.iter().find(|s| s.name == "scopingExample");
            assert!(scoping_example.is_some());
            assert!(scoping_example
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function scopingExample()"));

            // Inner function
            let inner_function = symbols.iter().find(|s| s.name == "innerFunction");
            assert!(inner_function.is_some());
            assert_eq!(inner_function.unwrap().kind, SymbolKind::Function);

            // Function expressions
            let not_hoisted = symbols.iter().find(|s| s.name == "notHoisted");
            assert!(not_hoisted.is_some());
            assert!(not_hoisted
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("var notHoisted = function()"));

            let also_not_hoisted = symbols.iter().find(|s| s.name == "alsoNotHoisted");
            assert!(also_not_hoisted.is_some());
            assert!(also_not_hoisted
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const alsoNotHoisted = function()"));

            // Arrow function
            let arrow_not_hoisted = symbols.iter().find(|s| s.name == "arrowNotHoisted");
            assert!(arrow_not_hoisted.is_some());
            assert!(arrow_not_hoisted
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("const arrowNotHoisted = () =>"));

            // Scoping demonstration function
            let demonstrate_scoping = symbols.iter().find(|s| s.name == "demonstrateScoping");
            assert!(demonstrate_scoping.is_some());

            // Closure creation function
            let create_closures = symbols.iter().find(|s| s.name == "createClosures");
            assert!(create_closures.is_some());
            assert!(create_closures
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function createClosures()"));
        }
    }

    mod error_handling_and_strict_mode {
        use super::*;

        #[test]
        fn test_extract_try_catch_blocks_error_classes_and_strict_mode_indicators() {
            let code = r#"
'use strict';

// Custom error classes
class ValidationError extends Error {
  constructor(message, field) {
    super(message);
    this.name = 'ValidationError';
    this.field = field;
  }
}

class NetworkError extends Error {
  constructor(message, statusCode, url) {
    super(message);
    this.name = 'NetworkError';
    this.statusCode = statusCode;
    this.url = url;
  }

  static fromResponse(response) {
    return new NetworkError(
      `HTTP ${response.status}: ${response.statusText}`,
      response.status,
      response.url
    );
  }
}

// Error handling utilities
function validateUser(user) {
  if (!user) {
    throw new ValidationError('User is required');
  }

  if (!user.email) {
    throw new ValidationError('Email is required', 'email');
  }

  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(user.email)) {
    throw new ValidationError('Invalid email format', 'email');
  }

  return true;
}

async function fetchWithRetry(url, options = {}, maxRetries = 3) {
  let lastError;

  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      const response = await fetch(url, options);

      if (!response.ok) {
        throw NetworkError.fromResponse(response);
      }

      return await response.json();
    } catch (error) {
      lastError = error;

      if (error instanceof NetworkError && error.statusCode < 500) {
        // Don't retry client errors
        throw error;
      }

      if (attempt === maxRetries) {
        throw new NetworkError(
          `Failed after ${maxRetries} attempts: ${lastError.message}`,
          0,
          url
        );
      }

      // Exponential backoff
      await new Promise(resolve =>
        setTimeout(resolve, Math.pow(2, attempt) * 1000)
      );
    }
  }
}

// Error boundary function
function withErrorHandling(fn) {
  return async function(...args) {
    try {
      return await fn.apply(this, args);
    } catch (error) {
      console.error('Error in', fn.name, ':', error);

      if (error instanceof ValidationError) {
        return { error: 'validation', message: error.message, field: error.field };
      }

      if (error instanceof NetworkError) {
        return { error: 'network', message: error.message, status: error.statusCode };
      }

      return { error: 'unknown', message: 'An unexpected error occurred' };
    }
  };
}

// Finally block example
function processFile(filename) {
  let file = null;

  try {
    file = openFile(filename);

    if (!file) {
      throw new Error(`Unable to open file: ${filename}`);
    }

    const content = file.read();

    if (content.length === 0) {
      throw new Error('File is empty');
    }

    return JSON.parse(content);
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new ValidationError(`Invalid JSON in file: ${filename}`);
    }

    throw error;
  } finally {
    if (file) {
      file.close();
    }
  }
}

// Multiple catch blocks simulation (not native JS, but common pattern)
function handleMultipleErrors(operation) {
  try {
    return operation();
  } catch (error) {
    switch (error.constructor) {
      case ValidationError:
        logValidationError(error);
        break;

      case NetworkError:
        logNetworkError(error);
        break;

      case TypeError:
        logTypeError(error);
        break;

      default:
        logUnknownError(error);
    }

    throw error;
  }
}

function logValidationError(error) {
  console.warn('Validation failed:', error.message);
}

function logNetworkError(error) {
  console.error('Network error:', error.message, 'Status:', error.statusCode);
}

function logTypeError(error) {
  console.error('Type error:', error.message);
}

function logUnknownError(error) {
  console.error('Unknown error:', error);
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let mut extractor = JavaScriptExtractor::new(
                "javascript".to_string(),
                "errors.js".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Custom error classes
            let validation_error = symbols.iter().find(|s| s.name == "ValidationError");
            assert!(validation_error.is_some());
            assert_eq!(validation_error.unwrap().kind, SymbolKind::Class);
            assert!(validation_error
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("class ValidationError extends Error"));

            let network_error = symbols.iter().find(|s| s.name == "NetworkError");
            assert!(network_error.is_some());
            assert!(network_error
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("class NetworkError extends Error"));

            // Static method
            let from_response = symbols.iter().find(|s| {
                s.name == "fromResponse" && s.signature.as_ref().unwrap().contains("static")
            });
            assert!(from_response.is_some());
            assert_eq!(from_response.unwrap().kind, SymbolKind::Method);

            // Error handling functions
            let validate_user = symbols.iter().find(|s| s.name == "validateUser");
            assert!(validate_user.is_some());
            assert!(validate_user
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function validateUser(user)"));

            let fetch_with_retry = symbols.iter().find(|s| s.name == "fetchWithRetry");
            assert!(fetch_with_retry.is_some());
            assert!(fetch_with_retry
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("async function fetchWithRetry"));

            // Error boundary
            let with_error_handling = symbols.iter().find(|s| s.name == "withErrorHandling");
            assert!(with_error_handling.is_some());
            assert!(with_error_handling
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function withErrorHandling(fn)"));

            // Finally block function
            let process_file = symbols.iter().find(|s| s.name == "processFile");
            assert!(process_file.is_some());
            assert!(process_file
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function processFile(filename)"));

            // Multiple error handling
            let handle_multiple_errors = symbols.iter().find(|s| s.name == "handleMultipleErrors");
            assert!(handle_multiple_errors.is_some());

            // Logging functions
            let log_validation_error = symbols.iter().find(|s| s.name == "logValidationError");
            assert!(log_validation_error.is_some());

            let log_network_error = symbols.iter().find(|s| s.name == "logNetworkError");
            assert!(log_network_error.is_some());

            let log_type_error = symbols.iter().find(|s| s.name == "logTypeError");
            assert!(log_type_error.is_some());

            let log_unknown_error = symbols.iter().find(|s| s.name == "logUnknownError");
            assert!(log_unknown_error.is_some());
        }
    }

    mod nodejs_and_browser_apis {
        use super::*;

        #[test]
        fn test_extract_commonjs_modules_require_statements_and_global_apis() {
            let code = r#"
// CommonJS exports and requires
const fs = require('fs');
const path = require('path');
const { promisify } = require('util');
const { EventEmitter } = require('events');

// Mixed module syntax (transpiled code)
const express = require('express');
import chalk from 'chalk';

// Module exports
module.exports = {
  createServer,
  middleware,
  utils: {
    formatDate,
    validateInput
  }
};

// Named exports
exports.logger = createLogger();
exports.config = loadConfig();

// Global APIs and polyfills
const globalThis = globalThis || global || window || self;

if (typeof window !== 'undefined') {
  // Browser environment
  window.myLibrary = {
    version: '1.0.0',
    init: initBrowser
  };

  // DOM APIs
  document.addEventListener('DOMContentLoaded', initBrowser);

  // Browser storage
  const storage = {
    set: (key, value) => localStorage.setItem(key, JSON.stringify(value)),
    get: (key) => JSON.parse(localStorage.getItem(key) || 'null'),
    remove: (key) => localStorage.removeItem(key)
  };

} else if (typeof global !== 'undefined') {
  // Node.js environment
  global.myLibrary = {
    version: '1.0.0',
    init: initNode
  };

  // Process APIs
  process.on('exit', cleanup);
  process.on('SIGINT', gracefulShutdown);
  process.on('uncaughtException', handleUncaughtException);
}

// Server creation
function createServer(options = {}) {
  const app = express();

  // Middleware setup
  app.use(express.json());
  app.use(express.urlencoded({ extended: true }));
  app.use(middleware.cors());
  app.use(middleware.logging());

  // Routes
  app.get('/health', (req, res) => {
    res.json({ status: 'healthy', timestamp: new Date().toISOString() });
  });

  app.post('/api/data', async (req, res) => {
    try {
      const validated = validateInput(req.body);
      const result = await processData(validated);
      res.json({ success: true, data: result });
    } catch (error) {
      res.status(400).json({ success: false, error: error.message });
    }
  });

  return app;
}

// Middleware functions
const middleware = {
  cors: () => (req, res, next) => {
    res.header('Access-Control-Allow-Origin', '*');
    res.header('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
    res.header('Access-Control-Allow-Headers', 'Content-Type, Authorization');
    next();
  },

  logging: () => (req, res, next) => {
    const start = Date.now();

    res.on('finish', () => {
      const duration = Date.now() - start;
      console.log(`${req.method} ${req.url} - ${res.statusCode} [${duration}ms]`);
    });

    next();
  },

  auth: (options = {}) => (req, res, next) => {
    const token = req.header('Authorization')?.replace('Bearer ', '');

    if (!token) {
      return res.status(401).json({ error: 'No token provided' });
    }

    try {
      const decoded = verifyToken(token, options.secret);
      req.user = decoded;
      next();
    } catch (error) {
      res.status(401).json({ error: 'Invalid token' });
    }
  }
};

// Utility functions
function formatDate(date, format = 'ISO') {
  if (format === 'ISO') {
    return date.toISOString();
  }

  return date.toLocaleDateString();
}

function validateInput(data) {
  if (!data || typeof data !== 'object') {
    throw new Error('Invalid input data');
  }

  return data;
}

function createLogger() {
  return {
    info: (message) => console.log(`[INFO] ${message}`),
    warn: (message) => console.warn(`[WARN] ${message}`),
    error: (message) => console.error(`[ERROR] ${message}`)
  };
}

function loadConfig() {
  return {
    port: process.env.PORT || 3000,
    database: {
      host: process.env.DB_HOST || 'localhost',
      port: process.env.DB_PORT || 5432
    }
  };
}

function initBrowser() {
  console.log('Initializing browser environment');
}

function initNode() {
  console.log('Initializing Node.js environment');
}

function cleanup() {
  console.log('Cleaning up resources...');
}

function gracefulShutdown() {
  console.log('Received SIGINT, shutting down gracefully...');
  process.exit(0);
}

function handleUncaughtException(error) {
  console.error('Uncaught exception:', error);
  process.exit(1);
}

async function processData(data) {
  // Simulate async processing
  await new Promise(resolve => setTimeout(resolve, 100));
  return { processed: true, ...data };
}

function verifyToken(token, secret) {
  // Simplified token verification
  return { userId: '123', username: 'user' };
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let mut extractor = JavaScriptExtractor::new(
                "javascript".to_string(),
                "server.js".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // CommonJS requires
            let fs_require = symbols
                .iter()
                .find(|s| s.name == "fs" && s.signature.as_ref().unwrap().contains("require"));
            assert!(fs_require.is_some());
            assert_eq!(fs_require.unwrap().kind, SymbolKind::Import);

            let path_require = symbols
                .iter()
                .find(|s| s.name == "path" && s.signature.as_ref().unwrap().contains("require"));
            assert!(path_require.is_some());

            let promisify_require = symbols.iter().find(|s| {
                s.name == "promisify" && s.signature.as_ref().unwrap().contains("require")
            });
            assert!(promisify_require.is_some());

            // Mixed module syntax
            let express_require = symbols
                .iter()
                .find(|s| s.name == "express" && s.signature.as_ref().unwrap().contains("require"));
            assert!(express_require.is_some());

            let chalk_import = symbols
                .iter()
                .find(|s| s.name == "chalk" && s.signature.as_ref().unwrap().contains("import"));
            assert!(chalk_import.is_some());

            // Main server function
            let create_server = symbols.iter().find(|s| s.name == "createServer");
            assert!(create_server.is_some());
            assert!(create_server
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function createServer(options = {})"));

            // Middleware object
            let middleware_obj = symbols.iter().find(|s| s.name == "middleware");
            assert!(middleware_obj.is_some());
            assert_eq!(middleware_obj.unwrap().kind, SymbolKind::Variable);

            // Middleware methods
            let cors_middleware = symbols.iter().find(|s| {
                s.name == "cors" && s.parent_id == Some(middleware_obj.unwrap().id.clone())
            });
            assert!(cors_middleware.is_some());
            assert_eq!(cors_middleware.unwrap().kind, SymbolKind::Method);

            let logging_middleware = symbols.iter().find(|s| {
                s.name == "logging" && s.parent_id == Some(middleware_obj.unwrap().id.clone())
            });
            assert!(logging_middleware.is_some());

            let auth_middleware = symbols.iter().find(|s| {
                s.name == "auth" && s.parent_id == Some(middleware_obj.unwrap().id.clone())
            });
            assert!(auth_middleware.is_some());

            // Utility functions
            let format_date = symbols.iter().find(|s| s.name == "formatDate");
            assert!(format_date.is_some());
            assert!(format_date
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("function formatDate(date, format = 'ISO')"));

            let validate_input = symbols.iter().find(|s| s.name == "validateInput");
            assert!(validate_input.is_some());

            let create_logger = symbols.iter().find(|s| s.name == "createLogger");
            assert!(create_logger.is_some());

            let load_config = symbols.iter().find(|s| s.name == "loadConfig");
            assert!(load_config.is_some());

            // Environment-specific functions
            let init_browser = symbols.iter().find(|s| s.name == "initBrowser");
            assert!(init_browser.is_some());

            let init_node = symbols.iter().find(|s| s.name == "initNode");
            assert!(init_node.is_some());

            // Process handlers
            let cleanup = symbols.iter().find(|s| s.name == "cleanup");
            assert!(cleanup.is_some());

            let graceful_shutdown = symbols.iter().find(|s| s.name == "gracefulShutdown");
            assert!(graceful_shutdown.is_some());

            let handle_uncaught_exception =
                symbols.iter().find(|s| s.name == "handleUncaughtException");
            assert!(handle_uncaught_exception.is_some());

            // Async functions
            let process_data = symbols.iter().find(|s| s.name == "processData");
            assert!(process_data.is_some());
            assert!(process_data
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("async function processData(data)"));

            let verify_token = symbols.iter().find(|s| s.name == "verifyToken");
            assert!(verify_token.is_some());
        }
    }
}
