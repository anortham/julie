use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_complex_queries_and_ctes() {
        let sql_code = r#"
-- Complex CTE query with window functions
WITH monthly_user_stats AS (
    SELECT
        DATE_TRUNC('month', created_at) as month,
        COUNT(*) as new_users,
        COUNT(*) OVER (ORDER BY DATE_TRUNC('month', created_at)) as cumulative_users
    FROM users
    WHERE created_at >= '2023-01-01'
    GROUP BY DATE_TRUNC('month', created_at)
),
active_user_metrics AS (
    SELECT
        user_id,
        COUNT(DISTINCT DATE(occurred_at)) as active_days,
        AVG(EXTRACT(EPOCH FROM (occurred_at - LAG(occurred_at) OVER (PARTITION BY user_id ORDER BY occurred_at)))) as avg_session_gap
    FROM analytics_events
    WHERE occurred_at >= NOW() - INTERVAL '30 days'
    GROUP BY user_id
    HAVING COUNT(DISTINCT DATE(occurred_at)) > 5
)
SELECT
    u.username,
    u.email,
    mus.month,
    mus.new_users,
    aum.active_days,
    aum.avg_session_gap,
    CASE
        WHEN aum.active_days > 20 THEN 'high_activity'
        WHEN aum.active_days > 10 THEN 'medium_activity'
        ELSE 'low_activity'
    END as activity_level,
    ROW_NUMBER() OVER (PARTITION BY mus.month ORDER BY aum.active_days DESC) as activity_rank
FROM users u
JOIN monthly_user_stats mus ON DATE_TRUNC('month', u.created_at) = mus.month
LEFT JOIN active_user_metrics aum ON u.id = aum.user_id
WHERE u.is_active = TRUE
ORDER BY mus.month DESC, aum.active_days DESC;

-- Recursive CTE for hierarchical data
WITH RECURSIVE user_hierarchy AS (
    -- Base case: top-level users
    SELECT id, username, manager_id, 0 as level, username as path
    FROM users
    WHERE manager_id IS NULL

    UNION ALL

    -- Recursive case: users with managers
    SELECT u.id, u.username, u.manager_id, uh.level + 1,
           uh.path || ' -> ' || u.username as path
    FROM users u
    JOIN user_hierarchy uh ON u.manager_id = uh.id
    WHERE uh.level < 10  -- Prevent infinite recursion
)
SELECT * FROM user_hierarchy ORDER BY level, path;

-- UPSERT operation (PostgreSQL syntax)
INSERT INTO user_profiles (user_id, bio, avatar_url)
VALUES (1, 'Software Engineer', 'https://example.com/avatar.jpg')
ON CONFLICT (user_id)
DO UPDATE SET
    bio = EXCLUDED.bio,
    avatar_url = EXCLUDED.avatar_url,
    updated_at = NOW();
"#;

        let symbols = extract_symbols(sql_code);

        let monthly_stats_function = symbols.iter().find(|s| s.name == "monthly_user_stats");
        assert!(monthly_stats_function.is_some());

        let active_user_metrics = symbols.iter().find(|s| s.name == "active_user_metrics");
        assert!(active_user_metrics.is_some());

        let user_hierarchy = symbols.iter().find(|s| s.name == "user_hierarchy");
        assert!(user_hierarchy.is_some());
        assert!(
            user_hierarchy
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("RECURSIVE")
        );

        let activity_level = symbols.iter().find(|s| s.name == "activity_level");
        assert!(activity_level.is_some());

        let activity_rank = symbols.iter().find(|s| s.name == "activity_rank");
        assert!(activity_rank.is_some());

        let window_functions = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("OVER ("))
            })
            .collect::<Vec<_>>();
        assert!(!window_functions.is_empty());
    }
}
