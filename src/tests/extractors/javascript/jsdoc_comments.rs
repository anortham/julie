//! JSDoc Comment Extraction Tests for JavaScript
//!
//! Tests for extracting JSDoc comments from JavaScript code. Validates that:
//! - Function JSDoc comments are extracted with @param and @returns tags
//! - Class JSDoc comments are extracted with @class tags
//! - Method JSDoc comments are extracted
//! - Multi-line comments are preserved
//! - Property JSDoc comments are extracted with @type tags
//! - Functions without comments correctly return None
//! - Import statement JSDoc comments are extracted

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
fn test_jsdoc_function_comment() {
    let code = r#"
/**
 * Handles authentication and user session management
 * @param {string} username - The user's username
 * @param {string} password - The user's password
 * @returns {Promise<User>} User object if auth succeeds
 */
function authenticate(username, password) {
    return fetch('/api/auth', { method: 'POST', body: JSON.stringify({ username, password }) });
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

    // Find the authenticate function
    let auth_func = symbols.iter().find(|s| s.name == "authenticate");
    assert!(auth_func.is_some(), "Should extract authenticate function");

    let auth_sym = auth_func.unwrap();
    // Verify the doc_comment field is populated
    assert!(
        auth_sym.doc_comment.is_some(),
        "Should extract JSDoc comment"
    );
    let doc_comment = auth_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc_comment.contains("Handles authentication"),
        "Doc comment should contain description"
    );
    assert!(
        doc_comment.contains("@param"),
        "Doc comment should contain @param tags"
    );
    assert!(
        doc_comment.contains("@returns"),
        "Doc comment should contain @returns tag"
    );
}

#[test]
fn test_jsdoc_class_comment() {
    let code = r#"
/**
 * A user authentication service
 * Handles login, logout, and token refresh
 * @class
 */
class AuthService {
    /**
     * Authenticate a user
     * @param {string} username
     * @param {string} password
     */
    authenticate(username, password) {
        return this.login(username, password);
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

    // Find the AuthService class
    let auth_service = symbols.iter().find(|s| s.name == "AuthService");
    assert!(auth_service.is_some(), "Should extract AuthService class");

    let class_sym = auth_service.unwrap();
    assert!(
        class_sym.doc_comment.is_some(),
        "Class should have JSDoc comment"
    );
    let class_doc = class_sym.doc_comment.as_ref().unwrap();
    assert!(
        class_doc.contains("user authentication service"),
        "Class doc should describe purpose"
    );
    assert!(
        class_doc.contains("@class"),
        "Class doc should contain @class tag"
    );

    // Find the authenticate method
    let auth_method = symbols
        .iter()
        .find(|s| s.name == "authenticate" && s.parent_id == Some(class_sym.id.clone()));
    assert!(auth_method.is_some(), "Should extract authenticate method");

    let method_sym = auth_method.unwrap();
    assert!(
        method_sym.doc_comment.is_some(),
        "Method should have JSDoc comment"
    );
    let method_doc = method_sym.doc_comment.as_ref().unwrap();
    assert!(
        method_doc.contains("Authenticate a user"),
        "Method doc should describe action"
    );
}

#[test]
fn test_jsdoc_multiple_comments() {
    let code = r#"
/**
 * First line of documentation
 * Second line of documentation
 * Third line of documentation
 */
const processData = (data) => {
    return data.map(item => item * 2);
};
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

    // Find the processData arrow function
    let process_data = symbols.iter().find(|s| s.name == "processData");
    assert!(
        process_data.is_some(),
        "Should extract processData function"
    );

    let func_sym = process_data.unwrap();
    assert!(
        func_sym.doc_comment.is_some(),
        "Function should have JSDoc comment"
    );
    let doc_comment = func_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc_comment.contains("First line"),
        "Should preserve first line"
    );
    assert!(
        doc_comment.contains("Second line"),
        "Should preserve second line"
    );
    assert!(
        doc_comment.contains("Third line"),
        "Should preserve third line"
    );
}

#[test]
fn test_jsdoc_property_comment() {
    let code = r#"
class Config {
    /**
     * The API endpoint URL
     * @type {string}
     */
    endpoint = 'https://api.example.com';

    /**
     * Maximum retry attempts
     * @type {number}
     */
    maxRetries = 3;
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

    // Find the Config class
    let config_class = symbols.iter().find(|s| s.name == "Config");
    assert!(config_class.is_some(), "Should extract Config class");
    let config_id = config_class.unwrap().id.clone();

    // Find the endpoint property
    let endpoint = symbols
        .iter()
        .find(|s| s.name == "endpoint" && s.parent_id == Some(config_id.clone()));
    assert!(endpoint.is_some(), "Should extract endpoint property");
    assert!(
        endpoint.unwrap().doc_comment.is_some(),
        "Property should have JSDoc comment"
    );
    assert!(
        endpoint
            .unwrap()
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("API endpoint")
    );

    // Find the maxRetries property
    let max_retries = symbols
        .iter()
        .find(|s| s.name == "maxRetries" && s.parent_id == Some(config_id.clone()));
    assert!(max_retries.is_some(), "Should extract maxRetries property");
    assert!(
        max_retries.unwrap().doc_comment.is_some(),
        "Property should have JSDoc comment"
    );
    assert!(
        max_retries
            .unwrap()
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Maximum retry")
    );
}

#[test]
fn test_jsdoc_with_no_comment() {
    let code = r#"
function noDocumentation() {
    return "This function has no JSDoc";
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

    // Find the function without documentation
    let no_doc = symbols.iter().find(|s| s.name == "noDocumentation");
    assert!(no_doc.is_some(), "Should extract function");
    assert!(
        no_doc.unwrap().doc_comment.is_none(),
        "Function without JSDoc should have None doc_comment"
    );
}

#[test]
fn test_jsdoc_import_statement() {
    let code = r#"
/**
 * Import React for building UI components
 * @typedef {Object} React
 */
import React from 'react';

/**
 * Import utility functions for data processing
 */
import { debounce, throttle } from 'lodash';
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

    // Find the React import
    let react_import = symbols.iter().find(|s| s.name == "React");
    assert!(react_import.is_some(), "Should extract React import");
    assert!(
        react_import.unwrap().doc_comment.is_some(),
        "Import should have JSDoc comment"
    );
    assert!(
        react_import
            .unwrap()
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Import React")
    );

    // Find lodash imports
    let debounce_import = symbols.iter().find(|s| s.name == "debounce");
    assert!(debounce_import.is_some(), "Should extract debounce import");
    assert!(
        debounce_import.unwrap().doc_comment.is_some(),
        "Import should have JSDoc comment"
    );
    assert!(
        debounce_import
            .unwrap()
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("utility functions")
    );
}
