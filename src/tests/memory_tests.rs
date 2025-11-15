// Tests for memory system (checkpoint/recall)
// Following TDD: Write tests first, then implement

use serde_json::json;

#[test]
fn test_memory_minimal_schema() {
    // Test that Memory accepts minimal 3-field schema
    let json_str = r#"{
        "id": "mem_1234567890_abc",
        "timestamp": 1234567890,
        "type": "checkpoint"
    }"#;

    // This should deserialize successfully
    let memory: crate::tools::memory::Memory = serde_json::from_str(json_str).unwrap();

    assert_eq!(memory.id, "mem_1234567890_abc");
    assert_eq!(memory.timestamp, 1234567890);
    assert_eq!(memory.memory_type, "checkpoint");
    assert!(memory.git.is_none());
}

#[test]
fn test_memory_with_git_context() {
    // Test Memory with optional git context
    let json_str = r#"{
        "id": "mem_1234567890_def",
        "timestamp": 1234567890,
        "type": "checkpoint",
        "git": {
            "branch": "main",
            "commit": "abc123",
            "dirty": false
        }
    }"#;

    let memory: crate::tools::memory::Memory = serde_json::from_str(json_str).unwrap();

    assert_eq!(memory.id, "mem_1234567890_def");
    let git = memory.git.unwrap();
    assert_eq!(git.branch, "main");
    assert_eq!(git.commit, "abc123");
    assert_eq!(git.dirty, false);
}

#[test]
fn test_memory_flexible_schema_checkpoint() {
    // Test checkpoint-specific fields via flatten
    let json_str = r#"{
        "id": "mem_1234567890_ghi",
        "timestamp": 1234567890,
        "type": "checkpoint",
        "description": "Fixed auth bug",
        "tags": ["bug", "auth"]
    }"#;

    let memory: crate::tools::memory::Memory = serde_json::from_str(json_str).unwrap();

    assert_eq!(memory.memory_type, "checkpoint");
    // Check flattened fields exist in extra
    let extra = memory.extra.as_object().unwrap();
    assert_eq!(
        extra.get("description").unwrap().as_str().unwrap(),
        "Fixed auth bug"
    );
    assert_eq!(extra.get("tags").unwrap().as_array().unwrap().len(), 2);
}

#[test]
fn test_memory_flexible_schema_decision() {
    // Test decision-specific fields (different schema, same Memory struct)
    let json_str = r#"{
        "id": "dec_1234567890_jkl",
        "timestamp": 1234567890,
        "type": "decision",
        "question": "Which database?",
        "chosen": "SQLite",
        "alternatives": ["Postgres", "MySQL"],
        "rationale": "Simplicity"
    }"#;

    let memory: crate::tools::memory::Memory = serde_json::from_str(json_str).unwrap();

    assert_eq!(memory.memory_type, "decision");
    // Check decision-specific fields
    let extra = memory.extra.as_object().unwrap();
    assert_eq!(
        extra.get("question").unwrap().as_str().unwrap(),
        "Which database?"
    );
    assert_eq!(extra.get("chosen").unwrap().as_str().unwrap(), "SQLite");
    assert_eq!(
        extra.get("alternatives").unwrap().as_array().unwrap().len(),
        2
    );
}

#[test]
fn test_memory_serialization_pretty_print() {
    // Test that Memory serializes with pretty-printing
    let memory = create_test_memory();

    let json_str = serde_json::to_string_pretty(&memory).unwrap();

    // Should be pretty-printed (has newlines)
    assert!(json_str.contains('\n'));
    assert!(json_str.contains("  ")); // Indentation

    // Should roundtrip
    let deserialized: crate::tools::memory::Memory = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.id, memory.id);
}

// Helper function to create test memory
fn create_test_memory() -> crate::tools::memory::Memory {
    crate::tools::memory::Memory {
        id: "mem_test_123".to_string(),
        timestamp: 1234567890,
        memory_type: "checkpoint".to_string(),
        git: Some(crate::tools::memory::GitContext {
            branch: "main".to_string(),
            commit: "abc123".to_string(),
            dirty: false,
            files_changed: Some(vec!["src/main.rs".to_string()]),
        }),
        extra: json!({
            "description": "Test memory",
            "tags": ["test"]
        }),
    }
}
