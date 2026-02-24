//! Adaptive token budget allocation for get_context results.
//!
//! Computes how to distribute a token budget across pivots, neighbors, and summary
//! sections based on result count. More pivots → broader/shallower treatment;
//! fewer pivots → deeper treatment with full code bodies.

/// How to render pivot symbols based on count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PivotMode {
    /// 1-3 pivots: full code bodies
    FullBody,
    /// 4-6 pivots: signature + first/last 5 lines
    SignatureAndKey,
    /// 7+: signature only
    SignatureOnly,
}

/// How to render neighbor symbols based on pivot count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeighborMode {
    /// 1-3 pivots: signature + doc comment + 1-line context
    SignatureAndDoc,
    /// 4-6 pivots: signature only
    SignatureOnly,
    /// 7+: just name + file:line
    NameAndLocation,
}

/// Computed token allocation for a get_context response.
#[derive(Debug)]
pub struct Allocation {
    /// Tokens reserved for pivot symbol content.
    pub pivot_tokens: u32,
    /// Tokens reserved for neighbor symbol content.
    pub neighbor_tokens: u32,
    /// Tokens reserved for summary/overview section.
    pub summary_tokens: u32,
    /// Rendering mode for pivot symbols.
    pub pivot_mode: PivotMode,
    /// Rendering mode for neighbor symbols.
    pub neighbor_mode: NeighborMode,
}

/// Token budget manager for get_context results.
///
/// Supports explicit budgets (`new`) and adaptive defaults (`adaptive`)
/// that scale based on how many pivots were found.
pub struct TokenBudget {
    pub max_tokens: u32,
}

impl TokenBudget {
    /// Create a budget with an explicit token limit.
    pub fn new(max_tokens: u32) -> Self {
        Self { max_tokens }
    }

    /// Create an adaptive budget based on pivot count.
    ///
    /// Fewer pivots → smaller budget (deep dive on fewer symbols).
    /// More pivots → larger budget (broad survey needs more room).
    ///
    /// - 0-2 pivots: 2000 tokens
    /// - 3-5 pivots: 3000 tokens
    /// - 6+ pivots: 4000 tokens
    pub fn adaptive(pivot_count: usize) -> Self {
        let budget = match pivot_count {
            0..=2 => 2000,
            3..=5 => 3000,
            _ => 4000,
        };
        Self { max_tokens: budget }
    }

    /// Allocate the token budget across pivots, neighbors, and summary.
    ///
    /// Uses a 60/30/10 split and selects rendering modes based on pivot count:
    /// - 0-3 pivots: FullBody pivots, SignatureAndDoc neighbors
    /// - 4-6 pivots: SignatureAndKey pivots, SignatureOnly neighbors
    /// - 7+ pivots: SignatureOnly pivots, NameAndLocation neighbors
    pub fn allocate(&self, pivot_count: usize, _neighbor_count: usize) -> Allocation {
        let (pivot_mode, neighbor_mode) = match pivot_count {
            0..=3 => (PivotMode::FullBody, NeighborMode::SignatureAndDoc),
            4..=6 => (PivotMode::SignatureAndKey, NeighborMode::SignatureOnly),
            _ => (PivotMode::SignatureOnly, NeighborMode::NameAndLocation),
        };

        let total = self.max_tokens;
        // 60/30/10 split — use integer math to avoid exceeding budget.
        // Floor division ensures sum <= total (since 0.6 + 0.3 + 0.1 = 1.0,
        // but floating point truncation means floor(total*0.6) + floor(total*0.3)
        // + floor(total*0.1) <= total). Assign remainder to summary.
        let pivot_tokens = (total as f64 * 0.6) as u32;
        let neighbor_tokens = (total as f64 * 0.3) as u32;
        let summary_tokens = total - pivot_tokens - neighbor_tokens;

        Allocation {
            pivot_tokens,
            neighbor_tokens,
            summary_tokens,
            pivot_mode,
            neighbor_mode,
        }
    }
}
