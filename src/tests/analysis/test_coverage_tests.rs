//! Tests for test-to-code linkage computation.

#[cfg(test)]
mod tests {
    use crate::analysis::test_coverage::{tier_rank, TestCoverageInfo};

    #[test]
    fn test_tier_rank_ordering() {
        assert!(tier_rank("thorough") > tier_rank("adequate"));
        assert!(tier_rank("adequate") > tier_rank("thin"));
        assert!(tier_rank("thin") > tier_rank("stub"));
        assert_eq!(tier_rank("unknown"), 0);
    }

    #[test]
    fn test_tier_best_worst() {
        // "thorough" should be best, "stub" should be worst
        let tiers = vec!["thin", "thorough", "stub"];
        let best = tiers.iter().max_by_key(|t| tier_rank(t)).unwrap();
        let worst = tiers.iter().min_by_key(|t| tier_rank(t)).unwrap();
        assert_eq!(*best, "thorough");
        assert_eq!(*worst, "stub");
    }
}
