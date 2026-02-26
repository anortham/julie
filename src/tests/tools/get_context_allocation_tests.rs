//! Tests for get_context token allocation strategy.

#[cfg(test)]
mod allocation_tests {
    use crate::tools::get_context::allocation::{NeighborMode, PivotMode, TokenBudget};

    #[test]
    fn test_single_pivot_full_body_mode() {
        let budget = TokenBudget::new(2000);
        let alloc = budget.allocate(1, 3);

        assert!(matches!(alloc.pivot_mode, PivotMode::FullBody));
        assert!(matches!(alloc.neighbor_mode, NeighborMode::SignatureAndDoc));
        assert_eq!(alloc.pivot_tokens, 1200);
    }

    #[test]
    fn test_many_pivots_signature_only_mode() {
        let budget = TokenBudget::new(4000);
        let alloc = budget.allocate(8, 20);

        assert!(matches!(alloc.pivot_mode, PivotMode::SignatureOnly));
        assert!(matches!(alloc.neighbor_mode, NeighborMode::NameAndLocation));
    }

    #[test]
    fn test_budget_respect() {
        for max in [500, 1000, 2000, 3000, 4000, 5000, 9999] {
            let budget = TokenBudget::new(max);
            for pivots in [0, 1, 3, 5, 7, 10] {
                let alloc = budget.allocate(pivots, pivots * 3);
                let total = alloc.pivot_tokens + alloc.neighbor_tokens + alloc.summary_tokens;
                assert!(total <= max);
            }
        }
    }

    #[test]
    fn test_adaptive_defaults() {
        assert_eq!(TokenBudget::adaptive(0).max_tokens, 2000);
        assert_eq!(TokenBudget::adaptive(1).max_tokens, 2000);
        assert_eq!(TokenBudget::adaptive(2).max_tokens, 2000);
        assert_eq!(TokenBudget::adaptive(3).max_tokens, 3000);
        assert_eq!(TokenBudget::adaptive(4).max_tokens, 3000);
        assert_eq!(TokenBudget::adaptive(5).max_tokens, 3000);
        assert_eq!(TokenBudget::adaptive(6).max_tokens, 4000);
        assert_eq!(TokenBudget::adaptive(8).max_tokens, 4000);
    }

    #[test]
    fn test_mid_range_pivots() {
        let budget = TokenBudget::new(3000);
        for pivot_count in [4, 5, 6] {
            let alloc = budget.allocate(pivot_count, 10);
            assert!(matches!(alloc.pivot_mode, PivotMode::SignatureAndKey));
            assert!(matches!(alloc.neighbor_mode, NeighborMode::SignatureOnly));
        }
    }

    #[test]
    fn test_60_30_10_split() {
        let budget = TokenBudget::new(1000);
        let alloc = budget.allocate(2, 5);
        assert_eq!(alloc.pivot_tokens, 600);
        assert_eq!(alloc.neighbor_tokens, 300);
        assert_eq!(alloc.summary_tokens, 100);
        assert_eq!(
            alloc.pivot_tokens + alloc.neighbor_tokens + alloc.summary_tokens,
            1000
        );
    }

    #[test]
    fn test_zero_pivots() {
        let budget = TokenBudget::new(2000);
        let alloc = budget.allocate(0, 0);
        assert!(matches!(alloc.pivot_mode, PivotMode::FullBody));
        assert!(matches!(alloc.neighbor_mode, NeighborMode::SignatureAndDoc));
        assert_eq!(
            alloc.pivot_tokens + alloc.neighbor_tokens + alloc.summary_tokens,
            2000
        );
    }

    #[test]
    fn test_boundary_three_pivots_is_full_body() {
        let budget = TokenBudget::new(2000);
        let alloc = budget.allocate(3, 5);
        assert!(matches!(alloc.pivot_mode, PivotMode::FullBody));
    }

    #[test]
    fn test_boundary_seven_pivots_is_signature_only() {
        let budget = TokenBudget::new(4000);
        let alloc = budget.allocate(7, 15);
        assert!(matches!(alloc.pivot_mode, PivotMode::SignatureOnly));
        assert!(matches!(alloc.neighbor_mode, NeighborMode::NameAndLocation));
    }
}
