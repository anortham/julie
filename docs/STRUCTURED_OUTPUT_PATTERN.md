# Structured Output Pattern for Julie Tools

**Created:** 2025-10-02
**Status:** ✅ COMPLETE (8 of 8 tools migrated - 100%)
**Philosophy:** Trust AI agents to format output for humans - return structured data

---

## Why Structured Output?

**Problem:** Markdown-only responses prevent tool chaining. Agents can't parse:
```
✅ **Fuzzy Replace Complete: src/main.rs**
**Matches replaced:** 3
```

**Solution:** Dual output - structured JSON for machines + markdown for humans:
```rust
CallToolResult::text_content(vec![TextContent::from(markdown)])
    .with_structured_content(json_map)
```

---

## The Pattern

### 1. Define a Structured Result Type

```rust
#[derive(Debug, Clone, Serialize)]
pub struct YourToolResult {
    /// Tool identifier (enables routing and schema detection)
    pub tool: String,
    /// Main results data
    pub results: Vec<YourDataType>,
    /// Quality/confidence signal
    pub confidence: f32,
    /// Success indicators
    pub success: bool,
    /// Files or entities modified
    pub files_modified: Vec<String>,
    /// Suggested next actions (KEY FOR TOOL CHAINING)
    pub next_actions: Vec<String>,
    /// Tool-specific metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}
```

### 2. Format Both Outputs in call_tool()

```rust
pub async fn call_tool(&self, handler: &Handler) -> Result<CallToolResult> {
    // ... do the work ...

    // Create structured result
    let result = YourToolResult {
        tool: "your_tool".to_string(),
        // ... populate fields ...
        next_actions: vec![
            "Review changes".to_string(),
            "Run tests".to_string(),
        ],
    };

    // Format markdown for humans
    let markdown = format!(
        "✅ **Operation Complete**\n\n\
         Details: {}\n\
         Next: {}",
        result.some_field,
        result.next_actions.join(", ")
    );

    // Serialize to JSON for machines
    let structured = serde_json::to_value(&result)?;
    let structured_map = if let serde_json::Value::Object(map) = structured {
        map
    } else {
        return Err(anyhow::anyhow!("Expected JSON object"));
    };

    // Return both
    Ok(CallToolResult::text_content(vec![TextContent::from(markdown)])
        .with_structured_content(structured_map))
}
```

### 3. Key Fields for Tool Interoperability

**Required:**
- `tool` (String): Identifier for routing ("fast_search", "fuzzy_replace", etc.)
- `success` or similar (bool): Quick status check
- `next_actions` (Vec<String>): Enables tool chaining

**Recommended:**
- `confidence` (f32): Quality signal for decision-making
- `files_modified` (Vec<String>): Track side effects
- `metadata` (Option<Value>): Tool-specific details

---

## Real Examples

### ✅ FastSearchTool (Migrated)

**Result Type:**
```rust
pub struct OptimizedResponse<T> {
    pub tool: String,              // "fast_search"
    pub results: Vec<T>,            // Vec<Symbol>
    pub confidence: f32,            // 0.85
    pub total_found: usize,         // 42
    pub insights: Option<String>,   // "Found across 3 languages"
    pub next_actions: Vec<String>,  // ["use fast_goto on getUserData"]
}
```

**Usage:**
```rust
let mut optimized = OptimizedResponse::new("fast_search", symbols, confidence);
optimized = optimized
    .with_insights(insights)
    .with_next_actions(next_actions);

let markdown = self.format_optimized_results(&optimized);
let structured = serde_json::to_value(&optimized)?;
// ... return both ...
```

### ✅ FuzzyReplaceTool (Migrated)

**Result Type:**
```rust
pub struct FuzzyReplaceResult {
    pub tool: String,                // "fuzzy_replace"
    pub file_path: String,           // "src/main.rs"
    pub pattern: String,             // "getUserData"
    pub replacement: String,         // "fetchUserData"
    pub matches_found: usize,        // 3
    pub threshold: f32,              // 0.8
    pub dry_run: bool,               // false
    pub validation_passed: bool,     // true
    pub next_actions: Vec<String>,   // ["Review changes", "Run tests"]
}
```

### ✅ SmartRefactorTool (Migrated)

**Result Type:**
```rust
pub struct SmartRefactorResult {
    pub tool: String,                     // "smart_refactor"
    pub operation: String,                // "rename_symbol", "extract_function", etc.
    pub dry_run: bool,
    pub success: bool,
    pub files_modified: Vec<String>,      // ["src/main.rs", "src/lib.rs"]
    pub changes_count: usize,             // 15
    pub next_actions: Vec<String>,
    pub metadata: Option<serde_json::Value>,  // Operation-specific details
}
```

**Helper Method:**
```rust
impl SmartRefactorTool {
    fn create_result(
        &self,
        operation: &str,
        success: bool,
        files_modified: Vec<String>,
        changes_count: usize,
        next_actions: Vec<String>,
        markdown: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<CallToolResult> {
        // Applies token optimization + creates structured + markdown output
    }
}
```

**Wired Up:**
- ✅ handle_rename_symbol (3 return points: error, dry_run, success)
- ✅ handle_replace_symbol_body (3 return points: error, dry_run, success)
- ✅ handle_extract_function (stub implementation)
- ✅ All 5 stub operations (insert_relative, extract_type, update_imports, inline_variable, inline_function)
- ✅ Unknown operation error handler
- ✅ Token optimization integrated into helper
- ✅ All 15 tests passing

### ✅ FastGotoTool (Migrated)

**Result Type:**
```rust
pub struct FastGotoResult {
    pub tool: String,              // "fast_goto"
    pub symbol: String,             // "UserService"
    pub found: bool,                // true
    pub definitions: Vec<DefinitionResult>,
    pub next_actions: Vec<String>,  // ["Navigate to file", "Use fast_refs"]
}
```

**Wired Up:**
- ✅ Error case (symbol not found)
- ✅ Success case (definitions found)
- ✅ All 6 tests passing

### ✅ FastRefsTool (Migrated)

**Result Type:**
```rust
pub struct FastRefsResult {
    pub tool: String,                  // "fast_refs"
    pub symbol: String,                 // "getUserData"
    pub found: bool,                    // true
    pub include_definition: bool,       // true
    pub definition_count: usize,        // 1
    pub reference_count: usize,         // 15
    pub definitions: Vec<DefinitionResult>,
    pub references: Vec<ReferenceResult>,
    pub next_actions: Vec<String>,
}
```

**Wired Up:**
- ✅ Error case (no references found)
- ✅ Success case with definitions + references
- ✅ Replaced manual json! construction with standardized helper
- ✅ All 6 tests passing

### ✅ FindLogicTool (Migrated)

**Result Type:**
```rust
pub struct FindLogicResult {
    pub tool: String,                       // "find_logic"
    pub domain: String,                     // "payment"
    pub found_count: usize,                 // 42
    pub intelligence_layers: Vec<String>,   // ["Keyword: 50", "Semantic: 12"]
    pub business_symbols: Vec<BusinessLogicSymbol>,
    pub next_actions: Vec<String>,
}
```

**Wired Up:**
- ✅ Single success return with multi-tier intelligence results
- ✅ All 4 tests passing

### ✅ FastExploreTool (Migrated)

**Result Type:**
```rust
pub struct FastExploreResult {
    pub tool: String,        // "fast_explore"
    pub mode: String,         // "overview", "dependencies", etc.
    pub depth: String,        // "minimal", "medium", "deep"
    pub focus: Option<String>, // Optional filter
    pub success: bool,
    pub next_actions: Vec<String>,
}
```

**Wired Up:**
- ✅ Error case (invalid mode)
- ✅ Success cases for all 5 modes (overview, dependencies, hotspots, trace, all)
- ✅ All 4 tests passing

### ✅ TraceCallPathTool (Migrated)

**Result Type:**
```rust
pub struct TraceCallPathResult {
    pub tool: String,                // "trace_call_path"
    pub symbol: String,               // "getUserData"
    pub direction: String,            // "upstream", "downstream", "both"
    pub max_depth: u32,               // 3
    pub cross_language: bool,         // true
    pub success: bool,
    pub paths_found: usize,           // 12
    pub next_actions: Vec<String>,
    pub error_message: Option<String>,
}
```

**Wired Up:**
- ✅ max_depth validation error
- ✅ similarity_threshold validation error
- ✅ Symbol not found error
- ✅ Symbol not found in context file error
- ✅ Invalid direction error
- ✅ Success case with call path tree
- ✅ All 16 tests passing (most complex tool - 6 return points)

---

## Migration Checklist

For each tool, follow these steps:

- [ ] **Define Result Struct** with required fields (tool, success, next_actions)
- [ ] **Add Serialize derive** to make it JSON-serializable
- [ ] **Update call_tool()** to create result struct
- [ ] **Format markdown** from result data (don't duplicate strings)
- [ ] **Serialize to JSON** with `serde_json::to_value`
- [ ] **Return both** via `text_content().with_structured_content()`
- [ ] **Test** - run tool's test suite to verify no regressions
- [ ] **Verify** - check that structured_content is present in output

---

## Benefits

✅ **Tool Chaining:** Agents can parse `next_actions` to determine next tool to call
✅ **Confidence Signals:** Agents can decide whether to retry or escalate
✅ **Side Effect Tracking:** `files_modified` enables cleanup and validation
✅ **Machine + Human:** Same output works for both audiences
✅ **Schema Detection:** `tool` field enables response routing and parsing

---

## Migration Status

| Tool | Status | Lines | Complexity | Tests | Return Points |
|------|--------|-------|------------|-------|---------------|
| FastSearchTool | ✅ Done | ~1100 | Medium | 474 passing | 2 |
| FuzzyReplaceTool | ✅ Done | ~350 | Low | 18 passing | 7 |
| SmartRefactorTool | ✅ Done | ~2100 | High | 15 passing | 13 |
| FastGotoTool | ✅ Done | ~150 | Low | 6 passing | 2 |
| FastRefsTool | ✅ Done | ~850 | Medium | 6 passing | 2 |
| FindLogicTool | ✅ Done | ~350 | Medium | 4 passing | 1 |
| FastExploreTool | ✅ Done | ~450 | Medium | 4 passing | 1 |
| TraceCallPathTool | ✅ Done | ~600 | Medium | 16 passing | 6 |

**Completed:** 8/8 tools (100%) ✅
**Total Return Points Migrated:** 34
**Total Tests Passing:** 523+

---

## Notes for Contributors

1. **Don't break markdown:** Users still see markdown - keep it readable
2. **JSON must be valid:** Test serialization doesn't fail
3. **next_actions are critical:** These enable tool chaining - be specific
4. **Reuse result structs:** If multiple tools return similar data, share the struct
5. **Update tests:** If tests expect TextContent, they may need updates

---

## Testing Structured Output

```rust
#[test]
fn test_returns_structured_output() {
    let tool = YourTool { /* ... */ };
    let result = tool.call_tool(&handler).await.unwrap();

    // Verify structured_content is present
    assert!(result.structured_content.is_some());

    // Verify required fields
    let structured = result.structured_content.unwrap();
    assert_eq!(structured.get("tool").unwrap(), "your_tool");
    assert!(structured.contains_key("next_actions"));

    // Verify markdown is still there
    assert!(!result.content.is_empty());
}
```

---

**Last Updated:** 2025-10-02 (✅ 100% COMPLETE - All 8 tools migrated, 34 return points, 523+ tests passing)
**Contact:** See validation document for implementation details
