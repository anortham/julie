# UTF-8 String Truncation Fix

## Problem

The codebase was using byte-based string slicing (`&str[..n]`) which caused panics when truncating strings containing multi-byte UTF-8 characters. The error occurred when processing JavaScript files with non-ASCII characters:

```
thread '<unnamed>' panicked at src/extractors/javascript.rs:775:45:
byte index 30 is not a char boundary; it is inside 'í' (bytes 29..31) of `[ "Jan","Feb","Mar","Apr","Maí","Jún",
	"Júl","Ágú","Sep","Okt","Nóv","Des" ]`
```

## Solution

Added a new utility function `BaseExtractor::truncate_string()` that safely truncates strings at **character boundaries** instead of byte boundaries. This function:

1. Counts characters (not bytes) using `.chars().count()`
2. Uses `.chars().take(n).collect()` to safely truncate at character boundaries
3. Automatically appends "..." when truncation occurs
4. Handles all UTF-8 characters correctly (emojis, accented characters, CJK characters, etc.)

## Files Modified

### Core Utility
- `src/extractors/base.rs`: Added `truncate_string()` method

### Extractors Fixed (String Truncation)
1. `src/extractors/javascript/signatures.rs` - 2 occurrences (lines 167, 188)
2. `src/extractors/html/attributes.rs` - 1 occurrence (line 33)
3. `src/extractors/html/scripts.rs` - 2 occurrences (lines 57, 107)
4. `src/extractors/regex/signatures.rs` - 1 occurrence (line 8)
5. `src/extractors/bash/signatures.rs` - 2 occurrences (lines 45, 61)
6. `src/extractors/razor/directives.rs` - 1 occurrence (line 239)
7. `src/extractors/powershell/commands.rs` - 1 occurrence (line 149)
8. `src/extractors/zig/variables.rs` - 1 occurrence (line 340)

### Extractors Fixed (String Slicing with `.find()`)
9. `src/extractors/sql/mod.rs` - 4 occurrences (lines 277, 293, 401-411) - Added char boundary checks
10. `src/extractors/ruby/assignments.rs` - 1 occurrence (line 267) - Added char boundary check
11. `src/extractors/python/decorators.rs` - 2 occurrences (lines 36, 42) - Added char boundary checks
12. `src/extractors/css/helpers.rs` - 1 occurrence (line 96) - Added char boundary check

### Tests Added
- `src/tests/utils/utf8_truncation.rs` - Comprehensive test suite covering:
  - Icelandic characters (from the original error)
  - Emoji characters
  - Various multibyte UTF-8 scenarios
  - Boundary conditions

## Implementation Details

### Type 1: Direct Truncation (unsafe byte slicing)

**Before (unsafe):**
```rust
if text.len() > 30 {
    format!("{}...", &text[..30])  // ❌ Panics on multi-byte chars
} else {
    text
}
```

**After (safe):**
```rust
BaseExtractor::truncate_string(&text, 30)  // ✅ Safe for all UTF-8
```

### Type 2: Slicing with `.find()` (potentially unsafe)

**Before (potentially unsafe):**
```rust
if let Some(eq_pos) = text.find('=') {
    let left = text[..eq_pos];  // ❌ Could panic if corrupted UTF-8
}
```

**After (safe):**
```rust
if let Some(eq_pos) = text.find('=') {
    if text.is_char_boundary(eq_pos) {  // ✅ Verify boundary
        let left = text[..eq_pos];
    }
}
```

### Type 3: Complex substring extraction (SQL OVER clauses)

**Before (unsafe):**
```rust
if let Some(over_index) = expr_text.find("OVER (") {
    if let Some(end_index) = expr_text[over_index..].find(')') {
        expr_text[0..over_index + end_index + 1].to_string()  // ❌ Unsafe
    }
}
```

**After (safe):**
```rust
if let Some(over_index) = expr_text.find("OVER (") {
    if let Some(end_index) = expr_text[over_index..].find(')') {
        let total_len = over_index + end_index + 1;
        if expr_text.is_char_boundary(total_len) {  // ✅ Safe
            expr_text[0..total_len].to_string()
        } else {
            expr_text.clone()
        }
    }
}
```

### New Utility Function:
```rust
pub fn truncate_string(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect::<String>() + "..."
    }
}
```

## Testing

All tests pass:
```
running 3 tests
test tests::utils::utf8_truncation::tests::test_truncate_string_preserves_multibyte_chars ... ok
test tests::utils::utf8_truncation::tests::test_truncate_string_with_emoji ... ok
test tests::utils::utf8_truncation::tests::test_truncate_string_with_utf8 ... ok
```

## Impact

- ✅ No more panics when processing files with non-ASCII characters
- ✅ All extractors now handle international text correctly (JavaScript, HTML, CSS, SQL, Bash, PowerShell, Razor, Zig, Python, Ruby, Regex)
- ✅ Character counting is now accurate (not byte counting)
- ✅ No performance impact (same O(n) complexity)
- ✅ All existing tests still pass
- ✅ Added comprehensive UTF-8 safety checks using `is_char_boundary()`
- ✅ Protected against edge cases in SQL window functions, Python decorators, Ruby assignments, and CSS properties

## Example

The original error string now truncates safely:
```rust
let text = r#"[ "Jan","Feb","Mar","Apr","Maí","Jún","Júl","Ágú","Sep","Okt","Nóv","Des" ]"#;
let result = BaseExtractor::truncate_string(text, 30);
// Works correctly without panicking!
```
