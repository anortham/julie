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
    pub(crate) reduction_steps: Vec<u8>,
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
    pub(crate) fn calculate_count(&self, total_items: usize, percentage: u8) -> usize {
        std::cmp::max(1, (total_items * percentage as usize) / 100)
    }
}

impl Default for ProgressiveReducer {
    fn default() -> Self {
        Self::new()
    }
}
