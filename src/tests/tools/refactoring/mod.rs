// Tests for refactoring tools (RenameSymbolTool, SmartRefactorTool rename logic)

// AST-aware refactoring tests (CRITICAL - verifies tree-sitter is actually used)
mod ast_aware;

// Import update tests for rename operations
mod import_update_tests;

// RenameSymbolTool focused tests
mod rename_symbol;

// SmartRefactorTool rename tests
mod smart_refactor;
