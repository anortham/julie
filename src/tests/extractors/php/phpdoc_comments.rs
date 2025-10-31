//! PHPDoc Comment Extraction Tests for PHP
//!
//! Tests for extracting PHPDoc comments from PHP code. Validates that:
//! - Class PHPDoc comments are extracted with @author, @version tags
//! - Function PHPDoc comments are extracted with @param, @return, @throws tags
//! - Method PHPDoc comments are extracted
//! - Property PHPDoc comments are extracted with @var tags
//! - Interface PHPDoc comments are extracted
//! - Constant PHPDoc comments are extracted
//! - Symbols without PHPDoc correctly return None

use crate::extractors::php::PhpExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;
use crate::extractors::base::Symbol;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("Error loading PHP grammar");
    parser
}

// Helper function to extract symbols from PHP code
fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        PhpExtractor::new("php".to_string(), "test.php".to_string(), code.to_string(), &workspace_root);

    extractor.extract_symbols(&tree)
}

#[test]
fn test_extract_phpdoc_from_class() {
    let php_code = r#"<?php
    /**
     * UserService manages user authentication and account operations.
     * Provides login, logout, and user management functionality.
     *
     * @author John Doe
     * @version 2.0
     * @since 1.0
     */
    class UserService {
        public function authenticate() {}
    }
    "#;

    let symbols = extract_symbols(php_code);
    let class_symbol = symbols.iter().find(|s| s.name == "UserService");

    assert!(class_symbol.is_some());
    let class_sym = class_symbol.unwrap();
    assert!(
        class_sym.doc_comment.is_some(),
        "UserService should have a doc comment"
    );

    let doc = class_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("manages user authentication"),
        "Doc should contain class description"
    );
    assert!(doc.contains("@author"), "Doc should contain @author tag");
    assert!(doc.contains("@version"), "Doc should contain @version tag");
}

#[test]
fn test_extract_phpdoc_from_function() {
    let php_code = r#"<?php
    /**
     * Validates user credentials against the database.
     * Returns true if credentials are valid, false otherwise.
     *
     * @param string $username The username to validate
     * @param string $password The password to validate
     * @return bool True if valid, false otherwise
     * @throws InvalidArgumentException if username is empty
     */
    function validateCredentials($username, $password) {
        return true;
    }
    "#;

    let symbols = extract_symbols(php_code);
    let func = symbols.iter().find(|s| s.name == "validateCredentials");

    assert!(func.is_some());
    let func_sym = func.unwrap();
    assert!(
        func_sym.doc_comment.is_some(),
        "validateCredentials should have a doc comment"
    );

    let doc = func_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("Validates user credentials"),
        "Doc should contain function description"
    );
    assert!(
        doc.contains("@param string $username"),
        "Doc should contain parameter documentation"
    );
    assert!(
        doc.contains("@return bool"),
        "Doc should contain return type documentation"
    );
    assert!(
        doc.contains("@throws InvalidArgumentException"),
        "Doc should contain exception documentation"
    );
}

#[test]
fn test_extract_phpdoc_from_method() {
    let php_code = r#"<?php
    class UserRepository {
        /**
         * Find a user by ID.
         *
         * @param int $id The user ID
         * @return User|null The user if found, null otherwise
         */
        public function findById($id) {
            return null;
        }
    }
    "#;

    let symbols = extract_symbols(php_code);
    let method = symbols.iter().find(|s| s.name == "findById");

    assert!(method.is_some());
    let method_sym = method.unwrap();
    assert!(
        method_sym.doc_comment.is_some(),
        "findById method should have a doc comment"
    );

    let doc = method_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("Find a user by ID"),
        "Doc should contain method description"
    );
    assert!(
        doc.contains("@param int $id"),
        "Doc should contain parameter documentation"
    );
    assert!(
        doc.contains("@return User|null"),
        "Doc should contain return type documentation"
    );
}

#[test]
fn test_extract_phpdoc_from_property() {
    let php_code = r#"<?php
    class User {
        /**
         * The user's email address.
         * Must be a valid email format.
         *
         * @var string
         */
        private $email;
    }
    "#;

    let symbols = extract_symbols(php_code);
    let property = symbols.iter().find(|s| s.name == "email");

    assert!(property.is_some());
    let prop_sym = property.unwrap();
    assert!(
        prop_sym.doc_comment.is_some(),
        "email property should have a doc comment"
    );

    let doc = prop_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("user's email address"),
        "Doc should contain property description"
    );
    assert!(
        doc.contains("@var string"),
        "Doc should contain variable type documentation"
    );
}

#[test]
fn test_extract_phpdoc_from_interface() {
    let php_code = r#"<?php
    /**
     * Defines the contract for serialization.
     * Implementing classes must provide serialization functionality.
     *
     * @since 1.5
     */
    interface Serializable {
        /**
         * Serialize the object to a string.
         *
         * @return string The serialized representation
         */
        public function serialize(): string;
    }
    "#;

    let symbols = extract_symbols(php_code);
    let interface = symbols.iter().find(|s| s.name == "Serializable");

    assert!(interface.is_some());
    let interface_sym = interface.unwrap();
    assert!(
        interface_sym.doc_comment.is_some(),
        "Serializable interface should have a doc comment"
    );

    let doc = interface_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("contract for serialization"),
        "Doc should contain interface description"
    );
    assert!(doc.contains("@since"), "Doc should contain @since tag");
}

#[test]
fn test_extract_phpdoc_from_constant() {
    let php_code = r#"<?php
    class Config {
        /**
         * Maximum number of connection attempts.
         *
         * @var int
         */
        public const MAX_RETRIES = 3;
    }
    "#;

    let symbols = extract_symbols(php_code);
    let constant = symbols.iter().find(|s| s.name == "MAX_RETRIES");

    assert!(constant.is_some());
    let const_sym = constant.unwrap();
    assert!(
        const_sym.doc_comment.is_some(),
        "MAX_RETRIES constant should have a doc comment"
    );

    let doc = const_sym.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("Maximum number of connection attempts"),
        "Doc should contain constant description"
    );
}

#[test]
fn test_phpdoc_extraction_mixed_symbols() {
    let php_code = r#"<?php
    /**
     * Handles payment processing
     */
    class PaymentProcessor {
        /**
         * Total amount processed.
         *
         * @var float
         */
        private $totalAmount = 0.0;

        /**
         * Process a payment transaction.
         *
         * @param float $amount The amount to process
         * @return bool Success status
         */
        public function processPayment($amount) {
            return true;
        }

        /**
         * Get the transaction history.
         *
         * @return array List of transactions
         */
        public function getHistory() {
            return [];
        }
    }
    "#;

    let symbols = extract_symbols(php_code);

    // Check class doc
    let class_sym = symbols
        .iter()
        .find(|s| s.name == "PaymentProcessor")
        .unwrap();
    assert!(class_sym.doc_comment.is_some());
    assert!(class_sym
        .doc_comment
        .as_ref()
        .unwrap()
        .contains("Handles payment processing"));

    // Check property doc
    let prop_sym = symbols.iter().find(|s| s.name == "totalAmount").unwrap();
    assert!(prop_sym.doc_comment.is_some());
    assert!(prop_sym
        .doc_comment
        .as_ref()
        .unwrap()
        .contains("Total amount processed"));

    // Check method docs
    let process_method = symbols.iter().find(|s| s.name == "processPayment").unwrap();
    assert!(process_method.doc_comment.is_some());
    assert!(process_method
        .doc_comment
        .as_ref()
        .unwrap()
        .contains("Process a payment"));

    let history_method = symbols.iter().find(|s| s.name == "getHistory").unwrap();
    assert!(history_method.doc_comment.is_some());
    assert!(history_method
        .doc_comment
        .as_ref()
        .unwrap()
        .contains("Get the transaction history"));
}

#[test]
fn test_symbols_without_phpdoc_have_none() {
    let php_code = r#"<?php
    class SimpleClass {
        public $property;

        public function method() {}
    }
    "#;

    let symbols = extract_symbols(php_code);

    let class_sym = symbols.iter().find(|s| s.name == "SimpleClass").unwrap();
    assert!(
        class_sym.doc_comment.is_none(),
        "Class without doc comment should have None"
    );

    let prop_sym = symbols.iter().find(|s| s.name == "property").unwrap();
    assert!(
        prop_sym.doc_comment.is_none(),
        "Property without doc comment should have None"
    );

    let method_sym = symbols.iter().find(|s| s.name == "method").unwrap();
    assert!(
        method_sym.doc_comment.is_none(),
        "Method without doc comment should have None"
    );
}
