// JSON Extractor Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR
//
// Comprehensive test coverage matching the quality of TypeScript/Rust extractors
// Target: 600+ lines with edge cases, special syntax, and real-world validation

#[cfg(test)]
mod json_extractor_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::extractors::json::JsonExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_json::LANGUAGE.into())
            .expect("Error loading JSON grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor = JsonExtractor::new(
            "json".to_string(),
            "test.json".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    // ========================================================================
    // Basic Object Key Extraction
    // ========================================================================

    #[test]
    fn test_extract_json_object_keys() {
        let json = r#"{
  "name": "my-project",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.0.0",
    "lodash": "^4.17.21"
  },
  "scripts": {
    "build": "webpack",
    "test": "jest"
  }
}"#;

        let symbols = extract_symbols(json);

        // We should extract top-level keys as symbols
        assert!(symbols.len() >= 4, "Expected at least 4 top-level keys, got {}", symbols.len());

        // Check for top-level keys
        let name_key = symbols.iter().find(|s| s.name == "name");
        assert!(name_key.is_some(), "Should find 'name' key");
        assert_eq!(name_key.unwrap().kind, SymbolKind::Variable);

        let version_key = symbols.iter().find(|s| s.name == "version");
        assert!(version_key.is_some(), "Should find 'version' key");

        let deps_key = symbols.iter().find(|s| s.name == "dependencies");
        assert!(deps_key.is_some(), "Should find 'dependencies' key");
        // Objects should be treated as namespaces/modules
        assert_eq!(deps_key.unwrap().kind, SymbolKind::Module);

        let scripts_key = symbols.iter().find(|s| s.name == "scripts");
        assert!(scripts_key.is_some(), "Should find 'scripts' key");
        assert_eq!(scripts_key.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_extract_nested_json_keys() {
        let json = r#"{
  "config": {
    "database": {
      "host": "localhost",
      "port": 5432
    }
  }
}"#;

        let symbols = extract_symbols(json);

        // Should have config, database, host, port
        assert!(symbols.len() >= 4, "Expected nested keys, got {}", symbols.len());

        let config = symbols.iter().find(|s| s.name == "config");
        assert!(config.is_some(), "Should find 'config' key");

        let database = symbols.iter().find(|s| s.name == "database");
        assert!(database.is_some(), "Should find 'database' key");
    }

    #[test]
    fn test_simple_flat_object() {
        let json = r#"{
  "string": "value",
  "number": 42,
  "boolean": true,
  "null": null
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 4, "Should extract all primitive keys");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"string"), "Should find 'string' key");
        assert!(names.contains(&"number"), "Should find 'number' key");
        assert!(names.contains(&"boolean"), "Should find 'boolean' key");
        assert!(names.contains(&"null"), "Should find 'null' key");
    }

    // ========================================================================
    // Array Handling
    // ========================================================================

    #[test]
    fn test_array_of_primitives() {
        let json = r#"{
  "tags": ["typescript", "javascript", "rust"],
  "scores": [1, 2, 3, 4, 5],
  "flags": [true, false, true]
}"#;

        let symbols = extract_symbols(json);

        // Arrays themselves might be extracted as keys
        let tags = symbols.iter().find(|s| s.name == "tags");
        assert!(tags.is_some(), "Should find 'tags' array key");
    }

    #[test]
    fn test_array_of_objects() {
        let json = r#"{
  "users": [
    {
      "name": "Alice",
      "age": 30
    },
    {
      "name": "Bob",
      "age": 25
    }
  ]
}"#;

        let symbols = extract_symbols(json);

        // Should at least extract the 'users' key
        let users = symbols.iter().find(|s| s.name == "users");
        assert!(users.is_some(), "Should find 'users' array");
    }

    #[test]
    fn test_nested_arrays() {
        let json = r#"{
  "matrix": [
    [1, 2, 3],
    [4, 5, 6],
    [7, 8, 9]
  ]
}"#;

        let symbols = extract_symbols(json);

        let matrix = symbols.iter().find(|s| s.name == "matrix");
        assert!(matrix.is_some(), "Should find 'matrix' nested array");
    }

    // ========================================================================
    // Deep Nesting
    // ========================================================================

    #[test]
    fn test_deeply_nested_objects() {
        let json = r#"{
  "level1": {
    "level2": {
      "level3": {
        "level4": {
          "level5": {
            "level6": "deep value"
          }
        }
      }
    }
  }
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 5, "Should extract deeply nested keys");

        let level1 = symbols.iter().find(|s| s.name == "level1");
        assert!(level1.is_some(), "Should find 'level1'");

        let has_deep = symbols.iter().any(|s| s.name.contains("level5") || s.name.contains("level6"));
        assert!(has_deep, "Should find deeply nested levels");
    }

    #[test]
    fn test_mixed_nesting() {
        let json = r#"{
  "data": {
    "users": [
      {
        "name": "Alice",
        "settings": {
          "theme": "dark",
          "notifications": true
        }
      }
    ],
    "config": {
      "timeout": 5000
    }
  }
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 3, "Should extract mixed nested structure");

        let data = symbols.iter().find(|s| s.name == "data");
        assert!(data.is_some(), "Should find 'data' key");
    }

    // ========================================================================
    // Special Characters & Escaping
    // ========================================================================

    #[test]
    fn test_keys_with_special_characters() {
        let json = r#"{
  "key-with-dashes": "value",
  "key_with_underscores": "value",
  "key.with.dots": "value",
  "key$with$dollars": "value",
  "@special": "value"
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 4, "Should extract keys with special characters");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|&n| n.contains("dashes")), "Should find key with dashes");
        assert!(names.iter().any(|&n| n.contains("underscores")), "Should find key with underscores");
    }

    #[test]
    fn test_string_values_with_escaping() {
        let json = r#"{
  "escaped_quote": "She said \"Hello\"",
  "escaped_backslash": "C:\\path\\to\\file",
  "newline": "Line 1\nLine 2",
  "tab": "Col1\tCol2",
  "unicode": "Hello \u0041\u0042\u0043"
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 5, "Should handle escaped string values");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"escaped_quote"), "Should find escaped_quote key");
        assert!(names.contains(&"unicode"), "Should find unicode key");
    }

    #[test]
    fn test_unicode_in_keys() {
        let json = r#"{
  "日本語": "Japanese",
  "Ελληνικά": "Greek",
  "العربية": "Arabic",
  "emoji🎉": "celebration"
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 4, "Should extract Unicode keys");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|&n| n.contains("日本語")), "Should preserve Japanese characters");
        assert!(names.iter().any(|&n| n.contains("🎉") || n.contains("emoji")), "Should handle emoji in keys");
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_empty_object() {
        let json = r#"{}"#;
        let symbols = extract_symbols(json);
        assert_eq!(symbols.len(), 0, "Empty object should yield no symbols");
    }

    #[test]
    fn test_empty_nested_objects() {
        let json = r#"{
  "config": {},
  "settings": {},
  "data": {}
}"#;

        let symbols = extract_symbols(json);

        // Should extract the keys even if values are empty objects
        assert!(symbols.len() >= 3, "Should extract keys with empty object values");
    }

    #[test]
    fn test_array_as_root() {
        let json = r#"[
  {"name": "item1"},
  {"name": "item2"}
]"#;

        let symbols = extract_symbols(json);

        // Array as root - may or may not extract items
        // At minimum, should not crash
        assert!(symbols.len() >= 0, "Should handle array as root");
    }

    #[test]
    fn test_primitive_as_root() {
        let json_cases = vec![
            r#""just a string""#,
            r#"42"#,
            r#"true"#,
            r#"null"#,
        ];

        for json in json_cases {
            let symbols = extract_symbols(json);
            // Primitives as root should not extract symbols
            assert_eq!(symbols.len(), 0, "Primitive root should yield no symbols");
        }
    }

    // ========================================================================
    // Numeric Values
    // ========================================================================

    #[test]
    fn test_various_number_formats() {
        let json = r#"{
  "integer": 42,
  "negative": -123,
  "decimal": 3.14159,
  "scientific": 1.23e10,
  "negative_scientific": -4.56e-7,
  "zero": 0
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 6, "Should extract all numeric keys");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"scientific"), "Should find scientific notation key");
        assert!(names.contains(&"decimal"), "Should find decimal key");
    }

    // ========================================================================
    // Real-World Patterns
    // ========================================================================

    #[test]
    fn test_package_json_pattern() {
        let json = r#"{
  "name": "@scope/package-name",
  "version": "1.0.0",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "scripts": {
    "build": "tsc",
    "test": "jest",
    "lint": "eslint src/**/*.ts"
  },
  "dependencies": {
    "react": "^18.0.0",
    "typescript": "^5.0.0"
  },
  "devDependencies": {
    "jest": "^29.0.0",
    "@types/react": "^18.0.0"
  },
  "engines": {
    "node": ">=18.0.0"
  },
  "repository": {
    "type": "git",
    "url": "https://github.com/user/repo.git"
  }
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 8, "Should extract package.json structure");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"), "Should find name");
        assert!(names.contains(&"scripts"), "Should find scripts");
        assert!(names.contains(&"dependencies"), "Should find dependencies");
        assert!(names.contains(&"devDependencies"), "Should find devDependencies");
    }

    #[test]
    fn test_tsconfig_json_pattern() {
        let json = r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020", "DOM"],
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "paths": {
      "@/*": ["./src/*"]
    }
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 3, "Should extract tsconfig structure");

        let compiler_options = symbols.iter().find(|s| s.name == "compilerOptions");
        assert!(compiler_options.is_some(), "Should find compilerOptions");

        let include = symbols.iter().find(|s| s.name == "include");
        assert!(include.is_some(), "Should find include");
    }

    #[test]
    fn test_api_response_pattern() {
        let json = r#"{
  "status": "success",
  "data": {
    "user": {
      "id": 123,
      "username": "alice",
      "email": "alice@example.com",
      "profile": {
        "avatar": "https://example.com/avatar.jpg",
        "bio": "Software developer"
      }
    },
    "posts": [
      {
        "id": 1,
        "title": "First Post",
        "content": "Hello World"
      }
    ]
  },
  "metadata": {
    "timestamp": "2024-01-01T00:00:00Z",
    "version": "1.0"
  }
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 3, "Should extract API response structure");

        let data = symbols.iter().find(|s| s.name == "data");
        assert!(data.is_some(), "Should find data field");

        let metadata = symbols.iter().find(|s| s.name == "metadata");
        assert!(metadata.is_some(), "Should find metadata field");
    }

    #[test]
    fn test_config_file_pattern() {
        let json = r#"{
  "app": {
    "name": "MyApp",
    "version": "2.0.0",
    "debug": false
  },
  "database": {
    "host": "localhost",
    "port": 5432,
    "name": "mydb",
    "credentials": {
      "username": "admin",
      "password": "secret"
    }
  },
  "cache": {
    "enabled": true,
    "ttl": 3600
  },
  "logging": {
    "level": "info",
    "format": "json"
  }
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 4, "Should extract config structure");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"app"), "Should find app config");
        assert!(names.contains(&"database"), "Should find database config");
        assert!(names.contains(&"cache"), "Should find cache config");
        assert!(names.contains(&"logging"), "Should find logging config");
    }

    // ========================================================================
    // Performance & Large Files
    // ========================================================================

    #[test]
    fn test_large_json_with_many_keys() {
        // Simulate a large JSON file with 100 top-level keys
        let mut json = String::from("{\n");

        for i in 0..100 {
            json.push_str(&format!("  \"key{}\": \"value{}\"{}\n", i, i, if i < 99 { "," } else { "" }));
        }

        json.push_str("}");

        let symbols = extract_symbols(&json);

        assert!(
            symbols.len() >= 100,
            "Should handle large files, got {} symbols",
            symbols.len()
        );
    }

    #[test]
    fn test_deeply_nested_performance() {
        // Create a deeply nested object (20 levels)
        let mut json = String::from("{");

        for i in 1..=20 {
            json.push_str(&format!("\"level{}\": {{", i));
        }

        json.push_str("\"value\": 42");

        for _ in 1..=20 {
            json.push_str("}");
        }

        json.push_str("}");

        let symbols = extract_symbols(&json);

        assert!(symbols.len() >= 10, "Should handle deep nesting");
    }

    // ========================================================================
    // Position Tracking
    // ========================================================================

    #[test]
    fn test_key_position_tracking() {
        let json = r#"{
  "first": "value1",
  "second": "value2",
  "third": "value3"
}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 3, "Should extract three keys");

        // Verify positions are tracked
        for symbol in &symbols {
            assert!(symbol.start_line > 0, "Should track start line for {}", symbol.name);
            assert!(symbol.end_line > 0, "Should track end line for {}", symbol.name);
            assert!(symbol.start_line <= symbol.end_line, "Start should be before end for {}", symbol.name);
        }

        // Verify keys are in order
        let first = symbols.iter().find(|s| s.name == "first");
        let third = symbols.iter().find(|s| s.name == "third");

        if let (Some(f), Some(t)) = (first, third) {
            assert!(f.start_line < t.start_line, "First key should be before third key");
        }
    }

    // ========================================================================
    // Whitespace Handling
    // ========================================================================

    #[test]
    fn test_minified_json() {
        let json = r#"{"name":"compact","version":"1.0.0","config":{"debug":false}}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 3, "Should extract from minified JSON");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"), "Should find name in minified JSON");
        assert!(names.contains(&"config"), "Should find config in minified JSON");
    }

    #[test]
    fn test_prettified_json_with_extra_whitespace() {
        let json = r#"{


  "key1":    "value1"    ,


  "key2":    "value2"


}"#;

        let symbols = extract_symbols(json);

        assert!(symbols.len() >= 2, "Should handle extra whitespace");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"key1"), "Should find key1");
        assert!(names.contains(&"key2"), "Should find key2");
    }

    // ========================================================================
    // JSONL (JSON Lines) Support Tests
    // ========================================================================
    // JSONL files have one JSON object per line, commonly used for logs,
    // streaming data, and memory storage systems. Each line must be parsed
    // independently and symbols should track which line they came from.

    fn extract_symbols_jsonl(code: &str, file_name: &str) -> Vec<Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();

        // For JSONL, we need to parse line by line
        let mut all_symbols = Vec::new();
        for (line_num, line) in code.lines().enumerate() {
            if line.trim().is_empty() {
                continue; // Skip empty lines
            }

            // Create a new extractor for THIS line (so byte positions match)
            let mut extractor = JsonExtractor::new(
                "json".to_string(),
                file_name.to_string(),
                line.to_string(), // Use the single line as source_code
                &workspace_root,
            );

            let tree = parser.parse(line, None).expect("Failed to parse JSONL line");
            let mut symbols = extractor.extract_symbols(&tree);

            // Adjust line numbers for each symbol based on which line of JSONL it came from
            for symbol in &mut symbols {
                symbol.start_line += line_num as u32;
                symbol.end_line += line_num as u32;
            }

            all_symbols.extend(symbols);
        }

        all_symbols
    }

    #[test]
    fn test_jsonl_basic_parsing() {
        let jsonl = r#"{"type":"feature","content":"Added search feature"}
{"type":"bug","content":"Fixed null pointer"}
{"type":"refactor","content":"Cleaned up code"}"#;

        let symbols = extract_symbols_jsonl(jsonl, "memories.jsonl");

        // Should extract symbols from all three lines
        assert!(symbols.len() >= 6, "Should extract at least 2 keys per line (type, content) × 3 lines");

        // Check that we extracted "type" keys from each line
        let type_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.name == "type")
            .collect();
        assert_eq!(type_symbols.len(), 3, "Should find 'type' key from each of 3 lines");

        // Check line numbers are correct (1-based: 1, 2, 3)
        assert_eq!(type_symbols[0].start_line, 1, "First object should be on line 1");
        assert_eq!(type_symbols[1].start_line, 2, "Second object should be on line 2");
        assert_eq!(type_symbols[2].start_line, 3, "Third object should be on line 3");
    }

    #[test]
    fn test_jsonl_with_empty_lines() {
        let jsonl = r#"{"name":"first"}

{"name":"second"}

{"name":"third"}"#;

        let symbols = extract_symbols_jsonl(jsonl, "test.jsonl");

        // Should skip empty lines and extract from the 3 valid lines
        let name_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.name == "name")
            .collect();
        assert_eq!(name_symbols.len(), 3, "Should find 'name' from 3 non-empty lines");
    }

    #[test]
    fn test_jsonl_memory_event_format() {
        // Real-world JSONL format from recall memory system
        let jsonl = r#"{"type":"decision","source":"agent","content":"Chose SQLite over PostgreSQL for vector storage","timestamp":"2025-11-08T12:00:00Z"}
{"type":"feature","source":"agent","content":"Implemented semantic search with embeddings","timestamp":"2025-11-08T13:30:00Z"}
{"type":"bug-fix","source":"user","content":"Fixed race condition in file watcher","timestamp":"2025-11-08T15:45:00Z"}"#;

        let symbols = extract_symbols_jsonl(jsonl, "memories.jsonl");

        // Each line has 4 keys: type, source, content, timestamp
        assert!(symbols.len() >= 12, "Should extract 4 keys × 3 lines = 12 symbols minimum");

        // Verify all expected keys exist
        let key_names: std::collections::HashSet<&str> = symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect();

        assert!(key_names.contains("type"), "Should extract 'type' key");
        assert!(key_names.contains("source"), "Should extract 'source' key");
        assert!(key_names.contains("content"), "Should extract 'content' key");
        assert!(key_names.contains("timestamp"), "Should extract 'timestamp' key");

        // Verify line numbers are tracked correctly
        let timestamp_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.name == "timestamp")
            .collect();

        assert_eq!(timestamp_symbols.len(), 3, "Should find 'timestamp' from each line");
        assert_eq!(timestamp_symbols[0].start_line, 1, "First timestamp on line 1");
        assert_eq!(timestamp_symbols[1].start_line, 2, "Second timestamp on line 2");
        assert_eq!(timestamp_symbols[2].start_line, 3, "Third timestamp on line 3");
    }

    #[test]
    fn test_jsonl_complex_nested_objects() {
        let jsonl = r#"{"id":"mem_001","data":{"user":"alice","action":"login"}}
{"id":"mem_002","data":{"user":"bob","action":"logout"}}"#;

        let symbols = extract_symbols_jsonl(jsonl, "events.jsonl");

        // Should extract nested keys with proper hierarchy
        assert!(symbols.len() >= 8, "Should extract id, data, user, action from 2 lines");

        // Find nested "user" symbols
        let user_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.name == "user")
            .collect();

        assert_eq!(user_symbols.len(), 2, "Should find nested 'user' key from both lines");
        assert_eq!(user_symbols[0].start_line, 1, "First nested user on line 1");
        assert_eq!(user_symbols[1].start_line, 2, "Second nested user on line 2");
    }

    #[test]
    fn test_real_world_jsonl_memories_fixture() {
        // Test real-world JSONL fixture (recall memory format)
        let content = std::fs::read_to_string("fixtures/real-world/json/memories.jsonl")
            .expect("Should find memories.jsonl fixture");

        let symbols = extract_symbols_jsonl(&content, "memories.jsonl");

        // Each line has 5 keys: type, source, content, timestamp, workspace_path
        // 6 lines × 5 keys = 30 symbols minimum
        assert!(symbols.len() >= 30, "Should extract at least 30 symbols from 6-line fixture");

        // Verify all expected keys exist
        let key_names: std::collections::HashSet<&str> = symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect();

        assert!(key_names.contains("type"), "Should extract 'type' key");
        assert!(key_names.contains("source"), "Should extract 'source' key");
        assert!(key_names.contains("content"), "Should extract 'content' key");
        assert!(key_names.contains("timestamp"), "Should extract 'timestamp' key");
        assert!(key_names.contains("workspace_path"), "Should extract 'workspace_path' key");

        // Verify line numbers are tracked correctly across all 6 lines
        let timestamps: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.name == "timestamp")
            .collect();

        assert_eq!(timestamps.len(), 6, "Should find 'timestamp' from all 6 lines");
        assert_eq!(timestamps[0].start_line, 1, "First line");
        assert_eq!(timestamps[1].start_line, 2, "Second line");
        assert_eq!(timestamps[2].start_line, 3, "Third line");
        assert_eq!(timestamps[3].start_line, 4, "Fourth line");
        assert_eq!(timestamps[4].start_line, 5, "Fifth line");
        assert_eq!(timestamps[5].start_line, 6, "Sixth line");
    }
}
