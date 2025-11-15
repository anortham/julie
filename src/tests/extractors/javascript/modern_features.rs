//! Modern JavaScript Features Tests
//!
//! Tests for modern JavaScript (ES6+) language features including:
//! - Arrow functions and async/await
//! - Classes with private fields, getters/setters, static methods
//! - Generator functions and async generators
//! - Import/export statements
//! - Destructuring (objects, arrays, parameters)
//! - Rest/spread operators
//! - Template literals and tagged templates
//! - Default parameters and object shorthand

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::javascript::JavaScriptExtractor;
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

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // ES6 Imports
    let lodash_import = symbols
        .iter()
        .find(|s| s.name == "debounce" && s.kind == SymbolKind::Import);
    assert!(lodash_import.is_some());
    assert!(
        lodash_import
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("import { debounce, throttle } from 'lodash'")
    );

    let react_import = symbols
        .iter()
        .find(|s| s.name == "React" && s.kind == SymbolKind::Import);
    assert!(react_import.is_some());
    assert!(
        react_import
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("import React, { useState, useEffect } from 'react'")
    );

    // ES6 Exports
    let component_export = symbols
        .iter()
        .find(|s| s.name == "Component" && s.kind == SymbolKind::Export);
    assert!(component_export.is_some());
    assert!(
        component_export
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("export { default as Component }")
    );

    let api_url_export = symbols
        .iter()
        .find(|s| s.name == "API_URL" && s.kind == SymbolKind::Export);
    assert!(api_url_export.is_some());
    assert!(
        api_url_export
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("export const API_URL")
    );

    // Arrow functions
    let add_arrow = symbols.iter().find(|s| s.name == "add");
    assert!(add_arrow.is_some());
    assert_eq!(add_arrow.unwrap().kind, SymbolKind::Function);
    assert!(
        add_arrow
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const add = (a, b) => a + b")
    );

    let multiply_arrow = symbols.iter().find(|s| s.name == "multiply");
    assert!(multiply_arrow.is_some());
    assert!(
        multiply_arrow
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const multiply = (x, y) =>")
    );

    // Async functions
    let fetch_data = symbols.iter().find(|s| s.name == "fetchData");
    assert!(fetch_data.is_some());
    assert_eq!(fetch_data.unwrap().kind, SymbolKind::Function);
    assert!(
        fetch_data
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("async function fetchData(url)")
    );

    let async_arrow = symbols.iter().find(|s| s.name == "asyncArrow");
    assert!(async_arrow.is_some());
    assert!(
        async_arrow
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const asyncArrow = async (id) =>")
    );

    // Generator functions
    let fibonacci = symbols.iter().find(|s| s.name == "fibonacci");
    assert!(fibonacci.is_some());
    assert_eq!(fibonacci.unwrap().kind, SymbolKind::Function);
    assert!(
        fibonacci
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function* fibonacci()")
    );

    let generator_arrow = symbols.iter().find(|s| s.name == "generatorArrow");
    assert!(generator_arrow.is_some());
    assert!(
        generator_arrow
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const generatorArrow = function* (items)")
    );

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
    let listener_count = symbols
        .iter()
        .find(|s| s.name == "listenerCount" && s.signature.as_ref().unwrap().contains("get"));
    assert!(listener_count.is_some());
    assert_eq!(listener_count.unwrap().kind, SymbolKind::Method);

    let max_listeners_setter = symbols
        .iter()
        .find(|s| s.name == "maxListeners" && s.signature.as_ref().unwrap().contains("set"));
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
    assert!(
        async_event_emitter
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("extends EventEmitter")
    );
}

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

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "modern.js".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // Destructuring imports with aliases
    let react_import = symbols
        .iter()
        .find(|s| s.name == "createElement" && s.signature.as_ref().unwrap().contains("as h"));
    assert!(react_import.is_some());
    assert_eq!(react_import.unwrap().kind, SymbolKind::Import);

    // Dynamic import function
    let load_module = symbols.iter().find(|s| s.name == "loadModule");
    assert!(load_module.is_some());
    assert!(
        load_module
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const loadModule = async () =>")
    );

    // Destructuring variables
    let name_var = symbols
        .iter()
        .find(|s| s.name == "name" && s.signature.as_ref().unwrap().contains("const { name"));
    assert!(name_var.is_some());
    assert_eq!(name_var.unwrap().kind, SymbolKind::Variable);

    let rest_var = symbols
        .iter()
        .find(|s| s.name == "rest" && s.signature.as_ref().unwrap().contains("...rest"));
    assert!(rest_var.is_some());

    // Destructuring parameters function
    let process_user = symbols.iter().find(|s| s.name == "processUser");
    assert!(process_user.is_some());
    assert!(
        process_user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function processUser({ name, age = 18, ...preferences })")
    );

    let process_array = symbols.iter().find(|s| s.name == "processArray");
    assert!(process_array.is_some());
    assert!(
        process_array
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const processArray = ([head, ...tail]) =>")
    );

    // Rest parameters
    let sum_fn = symbols.iter().find(|s| s.name == "sum");
    assert!(sum_fn.is_some());
    assert!(
        sum_fn
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function sum(...numbers)")
    );

    let combine_arrays = symbols.iter().find(|s| s.name == "combineArrays");
    assert!(combine_arrays.is_some());
    assert!(
        combine_arrays
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const combineArrays = (arr1, arr2, ...others) =>")
    );

    // Template literal function
    let format_user = symbols.iter().find(|s| s.name == "formatUser");
    assert!(format_user.is_some());
    assert!(
        format_user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const formatUser = (user) =>")
    );

    // Tagged template function
    let sql_fn = symbols.iter().find(|s| s.name == "sql");
    assert!(sql_fn.is_some());
    assert!(
        sql_fn
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function sql(strings, ...values)")
    );

    // Object with computed properties and shorthand methods
    let create_config = symbols.iter().find(|s| s.name == "createConfig");
    assert!(create_config.is_some());
    assert!(
        create_config
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const createConfig = (env, debug = false) =>")
    );

    // Default parameters with destructuring
    let create_user = symbols.iter().find(|s| s.name == "createUser");
    assert!(create_user.is_some());
    assert!(
        create_user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const createUser = (")
    );

    // Async generator
    let fetch_pages = symbols.iter().find(|s| s.name == "fetchPages");
    assert!(fetch_pages.is_some());
    assert!(
        fetch_pages
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("async function* fetchPages(baseUrl)")
    );
}
