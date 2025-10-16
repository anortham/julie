/// Inline tests extracted from src/utils/context_truncation.rs
///
/// This module contains all test cases for the ContextTruncator utility,
/// which provides context truncation functionality for limiting code context
/// while preserving essential structure.
///
/// Tests validate:
/// - Basic truncation behavior (short and long contexts)
/// - Smart truncation with essential line preservation
/// - Edge case handling (empty input, single line, zero limit)
/// - Preservation of structural elements (functions, classes, structs, decorators)

use crate::utils::context_truncation::ContextTruncator;

#[test]
fn test_short_context_unchanged() {
    let truncator = ContextTruncator::new();
    let lines = vec![
        "function getUserData() {".to_string(),
        "  return user.data;".to_string(),
        "}".to_string(),
    ];

    let result = truncator.truncate_lines(&lines, 10);
    assert_eq!(result, lines, "Short context should remain unchanged");
}

#[test]
fn test_long_context_truncated() {
    let truncator = ContextTruncator::new();
    let lines = vec![
        "function processData() {".to_string(),
        "  const data = getData();".to_string(),
        "  let result = [];".to_string(),
        "  for (let i = 0; i < data.length; i++) {".to_string(),
        "    result.push(data[i] * 2);".to_string(),
        "  }".to_string(),
        "  return result;".to_string(),
        "}".to_string(),
    ];

    let result = truncator.truncate_lines(&lines, 5);

    // Should be truncated to first 5 lines with current implementation
    assert_eq!(result.len(), 5);
    assert_eq!(result[0], "function processData() {");
    assert_eq!(result[4], "    result.push(data[i] * 2);");
}

#[test]
fn test_smart_truncation_preserves_function_signatures() {
    let truncator = ContextTruncator::new();
    let lines = vec![
        "/// This is a comprehensive function documentation".to_string(),
        "/// that explains what the function does in detail".to_string(),
        "pub fn process_user_data(user_id: u64, options: &ProcessOptions) -> Result<UserData, Error> {".to_string(),
        "    let connection = establish_database_connection()?;".to_string(),
        "    let raw_data = fetch_user_raw_data(&connection, user_id)?;".to_string(),
        "    let processed = transform_data(raw_data);".to_string(),
        "    let validated = validate_data(processed)?;".to_string(),
        "    let enhanced = enhance_with_metadata(validated);".to_string(),
        "    save_processed_data(&connection, &enhanced)?;".to_string(),
        "    Ok(enhanced)".to_string(),
        "}".to_string(),
    ];

    let result = truncator.smart_truncate(&lines, 5);

    // Should preserve doc comment, function signature, and return statement
    assert!(result.contains("/// This is a comprehensive function documentation"));
    assert!(result.contains("pub fn process_user_data(user_id: u64, options: &ProcessOptions) -> Result<UserData, Error> {"));
    assert!(result.contains("Ok(enhanced)"));
    assert!(result.contains("}"));
    assert!(result.contains("... (6 lines truncated) ..."));
}

#[test]
fn test_smart_truncation_preserves_class_definitions() {
    let truncator = ContextTruncator::new();
    let lines = vec![
        "/**".to_string(),
        " * UserService handles all user-related operations".to_string(),
        " * including authentication, data management, and preferences".to_string(),
        " */".to_string(),
        "class UserService {".to_string(),
        "    private database: Database;".to_string(),
        "    private cache: Cache;".to_string(),
        "    private validator: Validator;".to_string(),
        "    private logger: Logger;".to_string(),
        "    constructor(deps: Dependencies) {".to_string(),
        "        this.database = deps.database;".to_string(),
        "        this.cache = deps.cache;".to_string(),
        "        this.validator = deps.validator;".to_string(),
        "        this.logger = deps.logger;".to_string(),
        "    }".to_string(),
        "    async getUserData(userId: string): Promise<UserData> {".to_string(),
        "        return await this.database.query('SELECT * FROM users WHERE id = ?', [userId]);".to_string(),
        "    }".to_string(),
        "}".to_string(),
    ];

    let result = truncator.smart_truncate(&lines, 8);

    // Should preserve JSDoc comment, class definition, and closing brace
    assert!(result.contains("/**"));
    assert!(result.contains("class UserService {"));
    assert!(result.contains("}"));
    assert!(result.contains("truncated"));
}

#[test]
fn test_smart_truncation_handles_no_truncation_needed() {
    let truncator = ContextTruncator::new();
    let lines = vec![
        "fn small_function() {".to_string(),
        "    println!(\"Hello\");".to_string(),
        "}".to_string(),
    ];

    let result = truncator.smart_truncate(&lines, 10);

    // Should return all lines unchanged when under limit
    assert_eq!(result.lines().count(), 3);
    assert!(!result.contains("truncated"));
}

#[test]
fn test_smart_truncation_preserves_struct_definitions() {
    let truncator = ContextTruncator::new();
    let lines = vec![
        "/// Configuration for the user processing system".to_string(),
        "#[derive(Debug, Clone, Serialize, Deserialize)]".to_string(),
        "pub struct UserProcessingConfig {".to_string(),
        "    pub database_url: String,".to_string(),
        "    pub cache_ttl: Duration,".to_string(),
        "    pub max_connections: u32,".to_string(),
        "    pub enable_logging: bool,".to_string(),
        "    pub retry_attempts: u8,".to_string(),
        "    pub timeout_seconds: u64,".to_string(),
        "    pub batch_size: usize,".to_string(),
        "}".to_string(),
    ];

    let result = truncator.smart_truncate(&lines, 6);

    // Should preserve doc comment, attributes, struct definition, and closing brace
    assert!(result.contains("/// Configuration for the user processing system"));
    assert!(result.contains("#[derive(Debug, Clone, Serialize, Deserialize)]"));
    assert!(result.contains("pub struct UserProcessingConfig {"));
    assert!(result.contains("}"));
    assert!(result.contains("truncated"));
}

#[test]
fn test_smart_truncation_handles_edge_cases() {
    let truncator = ContextTruncator::new();

    // Test empty input
    let result = truncator.smart_truncate(&[], 5);
    assert_eq!(result, "");

    // Test single line
    let lines = vec!["single line".to_string()];
    let result = truncator.smart_truncate(&lines, 5);
    assert_eq!(result.lines().count(), 1);
    assert_eq!(result, "single line");

    // Test when max_lines is 0
    let lines = vec!["fn test() {}".to_string()];
    let result = truncator.smart_truncate(&lines, 0);
    assert_eq!(result, "");
}
