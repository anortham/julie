// Progressive Reduction for Token Optimization
//
// Port of codesearch StandardReductionStrategy.cs - verified implementation
// Provides graceful degradation when token limits are exceeded
//
// VERIFIED: Reduction steps [100, 75, 50, 30, 20, 10, 5] from StandardReductionStrategy.cs:8

/// Progressive reduction strategy for search results
/// Uses verified steps from codesearch to gracefully reduce result counts
pub struct ProgressiveReducer {
    /// Verified reduction steps from StandardReductionStrategy.cs:8
    reduction_steps: Vec<u8>,
}

impl ProgressiveReducer {
    /// Create new progressive reducer with verified steps
    pub fn new() -> Self {
        Self {
            // VERIFIED from StandardReductionStrategy.cs:8
            reduction_steps: vec![100, 75, 50, 30, 20, 10, 5],
        }
    }

    /// Reduce a collection using progressive steps
    ///
    /// # Arguments
    /// * `items` - Items to reduce
    /// * `target_token_count` - Target token count to achieve
    /// * `token_estimator` - Function to estimate tokens for a subset
    ///
    /// # Returns
    /// Reduced items that fit within token limit
    pub fn reduce<T, F>(&self, items: &[T], target_token_count: usize, token_estimator: F) -> Vec<T>
    where
        T: Clone,
        F: Fn(&[T]) -> usize,
    {
        if items.is_empty() {
            return Vec::new();
        }

        // Try each reduction step until we find one that fits within token limit
        for &percentage in &self.reduction_steps {
            let count = self.calculate_count(items.len(), percentage);
            let subset = &items[..count.min(items.len())];

            let estimated_tokens = token_estimator(subset);

            if estimated_tokens <= target_token_count {
                return subset.to_vec();
            }
        }

        // If even the smallest reduction doesn't fit, return just the first item
        // This ensures we never return empty results due to token constraints
        vec![items[0].clone()]
    }

    /// Calculate count for a given percentage
    /// VERIFIED implementation from StandardReductionStrategy.cs:35
    fn calculate_count(&self, total_items: usize, percentage: u8) -> usize {
        std::cmp::max(1, (total_items * percentage as usize) / 100)
    }
}

impl Default for ProgressiveReducer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_has_verified_reduction_steps() {
        let reducer = ProgressiveReducer::new();

        // VERIFIED steps from StandardReductionStrategy.cs:8
        assert_eq!(reducer.reduction_steps, vec![100, 75, 50, 30, 20, 10, 5]);
    }

    #[test]
    fn test_calculate_count_with_verified_formula() {
        let reducer = ProgressiveReducer::new();

        // VERIFIED formula from StandardReductionStrategy.cs:35
        // count = std::cmp::max(1, (items.len() * percentage) / 100)

        assert_eq!(reducer.calculate_count(100, 100), 100); // 100%
        assert_eq!(reducer.calculate_count(100, 75), 75);   // 75%
        assert_eq!(reducer.calculate_count(100, 50), 50);   // 50%
        assert_eq!(reducer.calculate_count(100, 30), 30);   // 30%
        assert_eq!(reducer.calculate_count(100, 20), 20);   // 20%
        assert_eq!(reducer.calculate_count(100, 10), 10);   // 10%
        assert_eq!(reducer.calculate_count(100, 5), 5);     // 5%

        // Edge case: always returns at least 1
        assert_eq!(reducer.calculate_count(10, 5), 1);      // 0.5 -> 1
        assert_eq!(reducer.calculate_count(1, 50), 1);      // 0.5 -> 1
    }

    #[test]
    fn test_reduce_when_items_already_within_limit() {
        let reducer = ProgressiveReducer::new();
        let items = vec!["item1", "item2", "item3"];

        // Token estimator returns low count (already within limit)
        let token_estimator = |items: &[&str]| items.len() * 10; // 10 tokens per item

        let result = reducer.reduce(&items, 1000, token_estimator); // High limit

        // Should return all items unchanged when within limit
        assert_eq!(result, items);
    }

    #[test]
    fn test_reduce_applies_progressive_steps() {
        let reducer = ProgressiveReducer::new();
        let items: Vec<String> = (1..=100).map(|i| format!("item{}", i)).collect();

        // Token estimator that returns count * 100 tokens per item
        let token_estimator = |items: &[String]| items.len() * 100;

        let result = reducer.reduce(&items, 2000, token_estimator); // 2000 token limit

        // With 100 tokens per item and 2000 limit, should reduce to ~20 items
        // This test will fail initially - that's expected for TDD
        assert!(result.len() <= 20);
        assert!(result.len() > 0);
    }

    #[test]
    fn test_reduce_uses_smallest_step_when_needed() {
        let reducer = ProgressiveReducer::new();
        let items: Vec<String> = (1..=100).map(|i| format!("item{}", i)).collect();

        // Token estimator that makes each item very expensive
        let token_estimator = |items: &[String]| items.len() * 1000; // 1000 tokens per item

        let result = reducer.reduce(&items, 6000, token_estimator); // Low limit

        // Should reduce to minimum step (5% = 5 items)
        // This test will fail initially - that's expected for TDD
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_reduce_preserves_order() {
        let reducer = ProgressiveReducer::new();
        let items = vec!["first", "second", "third", "fourth", "fifth"];

        // Token estimator that forces reduction
        let token_estimator = |items: &[&str]| items.len() * 1000;

        let result = reducer.reduce(&items, 2500, token_estimator); // Forces to ~2 items

        // Should preserve order and take first items
        // This test will fail initially - that's expected for TDD
        assert_eq!(result[0], "first");
        if result.len() > 1 {
            assert_eq!(result[1], "second");
        }
    }
}