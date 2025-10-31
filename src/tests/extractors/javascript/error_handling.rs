//! Error Handling and Strict Mode Tests for JavaScript
//!
//! Tests for JavaScript error handling patterns including:
//! - Custom error classes extending Error
//! - Try/catch/finally blocks
//! - Error boundaries and error handling utilities
//! - Async error handling with retry logic
//! - Strict mode indicators

use crate::extractors::base::SymbolKind;
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

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        "errors.js".to_string(),
        code.to_string(),
        &workspace_root,
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
