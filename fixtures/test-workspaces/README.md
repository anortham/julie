# Test Workspace Fixtures

Small, predictable workspaces for integration testing.

## Structure

```
test-workspaces/
├── tiny-primary/          # Primary workspace fixture (~50 lines)
│   └── src/
│       ├── main.rs       # calculate_sum(), primary_marker_function()
│       └── lib.rs        # PrimaryUser struct, process_primary_data()
└── tiny-reference/        # Reference workspace fixture (~50 lines)
    └── src/
        ├── helper.rs     # calculate_product(), reference_marker_function()
        └── types.rs      # ReferenceProduct struct, process_reference_data()
```

## Usage in Tests

```rust
use std::path::PathBuf;

let primary_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("fixtures/test-workspaces/tiny-primary");

let reference_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("fixtures/test-workspaces/tiny-reference");

// Use these paths directly instead of creating TempDirs
```

## Design Principles

1. **Small** - ~50 lines each, fast to index
2. **Predictable** - Known symbols for assertions
3. **Distinct** - Different markers to verify isolation
4. **Realistic** - Real Rust code, not dummy text
