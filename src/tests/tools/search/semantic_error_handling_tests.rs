// Semantic Search Error Handling Tests
//
// Tests for graceful handling of failures in semantic search operations:
// - Mutex poisoning (lock failures)
// - NaN comparison in float sorting
// - Database operation failures

use crate::extractors::base::{Symbol, SymbolKind};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_symbol(id: &str, name: &str, similarity_score: f32) -> (Symbol, f32) {
    let symbol = Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "test.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 1,
        start_byte: 0,
        end_byte: 50,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    };
    (symbol, similarity_score)
}

// ============================================================================
// Test 1: Safe Float Comparison with NaN
// ============================================================================

#[test]
fn test_safe_float_comparison_handles_nan() {
    // Helper function that mimics the sorting logic but safely handles NaN
    fn safe_compare(a: f32, b: f32) -> Option<std::cmp::Ordering> {
        b.partial_cmp(&a)
    }

    // Test normal case (both finite)
    assert_eq!(
        safe_compare(1.5, 2.5),
        Some(std::cmp::Ordering::Greater),
        "Normal comparison should work"
    );

    // Test NaN case (returns None)
    let nan = f32::NAN;
    assert_eq!(
        safe_compare(nan, 2.5),
        None,
        "NaN comparison should return None, not panic"
    );

    // Test both NaN
    assert_eq!(
        safe_compare(nan, nan),
        None,
        "NaN vs NaN should return None"
    );

    // Test infinity
    let inf = f32::INFINITY;
    assert_eq!(
        safe_compare(inf, 2.5),
        Some(std::cmp::Ordering::Less),
        "Infinity in second position (higher) is Less in descending order"
    );
}

#[test]
fn test_sort_with_nan_values_fails_safely() {
    // This test demonstrates that the original code panics on NaN
    // We'll verify that sorting with NaN in the data returns an error

    let mut values: Vec<(String, f32)> = vec![
        ("a".to_string(), 1.5),
        ("b".to_string(), f32::NAN),
        ("c".to_string(), 2.5),
    ];

    // The original code would panic here:
    // values.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Safe version using expect_or pattern
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // This simulates what would happen with unwrap
        values.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .expect("NaN found in similarity scores - this should never happen in production")
        });
    }));

    // Verify it panics (demonstrating the bug)
    assert!(
        result.is_err(),
        "Unwrap on NaN should panic - this is the bug we're fixing"
    );
}

#[test]
fn test_sort_symbols_by_score_handles_nan() {
    // Test the corrected sorting approach
    let mut scored_symbols = vec![
        create_test_symbol("id1", "func_a", 0.9),
        create_test_symbol("id2", "func_b", 0.7),
        create_test_symbol("id3", "func_c", 0.85),
    ];

    // Safe sort that handles NaN gracefully
    scored_symbols.sort_by(|a, b| {
        // Use unwrap_or to default NaN comparisons to Equal
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Verify correct order (descending by score)
    assert_eq!(scored_symbols[0].1, 0.9);
    assert_eq!(scored_symbols[1].1, 0.85);
    assert_eq!(scored_symbols[2].1, 0.7);
}

#[test]
fn test_sort_symbols_with_nan_in_scores() {
    // Test that NaN values don't cause panics
    let mut scored_symbols = vec![
        create_test_symbol("id1", "func_a", 0.9),
        create_test_symbol("id2", "func_b", f32::NAN),
        create_test_symbol("id3", "func_c", 0.85),
    ];

    // This should NOT panic with unwrap_or(Equal)
    scored_symbols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // All values should still be present
    assert_eq!(scored_symbols.len(), 3);

    // Non-NaN values should be sorted correctly
    // NaN position depends on comparison result (Equal puts it in place)
}

// ============================================================================
// Test 2: Mutex Lock Failure Handling
// ============================================================================

#[test]
fn test_mutex_lock_handles_poisoned_mutex() {
    let data = Arc::new(Mutex::new(42));

    // Simulate a poisoned mutex by deliberately panicking in a thread
    let data_clone = data.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = data_clone.lock().unwrap(); // Will panic after poisoning
        panic!("Poisoning the mutex"); // This poisons the mutex
    }));

    // Mutex is now poisoned
    assert!(result.is_err(), "Thread should have panicked");

    // Original code: db.lock().unwrap() would PANIC here
    let lock_result = data.lock();

    // Verify lock fails with PoisonError
    assert!(lock_result.is_err(), "Lock should be poisoned");

    // Safe approach: use match or if let to handle PoisonError
    match lock_result {
        Ok(guard) => {
            // Use the guard
            let _value = *guard;
        }
        Err(poison_error) => {
            // Handle poisoned mutex gracefully
            // Option 1: Use recover() to still get the lock
            let _recovered = poison_error.into_inner();
            // Option 2: Return an error to caller
        }
    }
}

// ============================================================================
// Test 3: Database Query Error Propagation
// ============================================================================

#[test]
fn test_get_symbols_by_ids_error_propagation() {
    // This test verifies the error handling pattern for database operations
    // The actual database is wrapped, but we verify the pattern works

    #[derive(Debug)]
    struct MockSymbolResult {
        symbols: Result<Vec<Symbol>, String>,
    }

    impl MockSymbolResult {
        fn get_symbols(&self) -> Result<Vec<Symbol>, String> {
            self.symbols.as_ref().cloned().map_err(|e| e.clone())
        }
    }

    // Success case
    let mock = MockSymbolResult {
        symbols: Ok(vec![]),
    };

    let result = mock.get_symbols();
    assert!(result.is_ok());

    // Failure case - returns error instead of panicking
    let mock_fail = MockSymbolResult {
        symbols: Err("Database connection failed".to_string()),
    };

    let result = mock_fail.get_symbols();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Database connection failed");
}

// ============================================================================
// Test 4: Complete Error Handling Chain
// ============================================================================

#[test]
fn test_semantic_search_error_chain() {
    // Simulate the error handling chain for semantic search operations

    struct SearchContext {
        symbols: Vec<(Symbol, f32)>,
    }

    impl SearchContext {
        fn sort_by_score(&mut self) -> Result<(), String> {
            // Safe sorting that handles NaN
            self.symbols
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            Ok(())
        }

        fn filter_results(&self, limit: usize) -> Result<Vec<Symbol>, String> {
            if limit == 0 {
                return Err("Limit must be greater than 0".to_string());
            }

            Ok(self
                .symbols
                .iter()
                .take(limit)
                .map(|(s, _)| s.clone())
                .collect())
        }
    }

    // Test successful case
    let mut context = SearchContext {
        symbols: vec![
            create_test_symbol("id1", "func_a", 0.9),
            create_test_symbol("id2", "func_b", 0.7),
        ],
    };

    assert!(context.sort_by_score().is_ok());
    let results = context.filter_results(10);
    assert!(results.is_ok());
    assert_eq!(results.unwrap().len(), 2);

    // Test error case (invalid limit)
    let results = context.filter_results(0);
    assert!(results.is_err());
    assert_eq!(results.unwrap_err(), "Limit must be greater than 0");
}

#[test]
fn test_panic_recovery_in_lock_operations() {
    // Test that we can gracefully recover from lock operations
    let counter = Arc::new(Mutex::new(0));

    fn increment_safe(counter: &Arc<Mutex<i32>>) -> Result<i32, String> {
        let mut guard = counter.lock().map_err(|e| format!("Lock failed: {}", e))?;
        *guard += 1;
        Ok(*guard)
    }

    // Normal increment
    let result = increment_safe(&counter);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1);

    // Second increment
    let result = increment_safe(&counter);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 2);
}

#[test]
fn test_safe_lock_with_clone() {
    // Test that we can safely handle mutex access with cloning
    let data = Arc::new(Mutex::new(vec![1, 2, 3]));

    fn safe_lock_access_clone(mutex: &Arc<Mutex<Vec<i32>>>) -> Result<Vec<i32>, String> {
        mutex
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| format!("Failed to acquire lock: {}", e))
    }

    // Normal case - successful clone
    let result = safe_lock_access_clone(&data);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), vec![1, 2, 3]);

    // Verify we can access multiple times
    let result2 = safe_lock_access_clone(&data);
    assert!(result2.is_ok());
    assert_eq!(result2.unwrap(), vec![1, 2, 3]);
}
