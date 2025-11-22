use super::{SymbolKind, extract_symbols};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_stored_procedures_functions_and_triggers() {
        let sql_code = r#"
-- Stored procedure with parameters
DELIMITER $$
CREATE PROCEDURE GetUserAnalytics(
    IN p_user_id BIGINT,
    IN p_start_date DATE,
    IN p_end_date DATE,
    OUT p_total_events INT,
    OUT p_unique_sessions INT
)
BEGIN
    DECLARE EXIT HANDLER FOR SQLEXCEPTION
    BEGIN
        ROLLBACK;
        RESIGNAL;
    END;

    START TRANSACTION;

    SELECT
        COUNT(*) INTO p_total_events,
        COUNT(DISTINCT session_id) INTO p_unique_sessions
    FROM analytics_events
    WHERE user_id = p_user_id
      AND DATE(occurred_at) BETWEEN p_start_date AND p_end_date;

    COMMIT;
END$$
DELIMITER ;

-- User-defined function
CREATE FUNCTION CalculateUserScore(p_user_id BIGINT)
RETURNS DECIMAL(10,2)
READS SQL DATA
DETERMINISTIC
BEGIN
    DECLARE v_score DECIMAL(10,2) DEFAULT 0.0;
    DECLARE v_event_count INT;
    DECLARE v_account_age_days INT;

    SELECT COUNT(*), DATEDIFF(NOW(), created_at)
    INTO v_event_count, v_account_age_days
    FROM analytics_events ae
    JOIN users u ON ae.user_id = u.id
    WHERE ae.user_id = p_user_id;

    SET v_score = (v_event_count * 0.1) + (v_account_age_days * 0.01);

    RETURN COALESCE(v_score, 0.0);
END;

-- PostgreSQL function with JSON processing
CREATE OR REPLACE FUNCTION update_user_preferences(
    p_user_id BIGINT,
    p_preferences JSONB
) RETURNS BOOLEAN
LANGUAGE plpgsql
AS $$
DECLARE
    v_current_prefs JSONB;
    v_merged_prefs JSONB;
BEGIN
    -- Get current preferences
    SELECT preferences INTO v_current_prefs
    FROM user_profiles
    WHERE user_id = p_user_id;

    -- Merge with new preferences
    v_merged_prefs := COALESCE(v_current_prefs, '{}'::jsonb) || p_preferences;

    -- Update the user profile
    UPDATE user_profiles
    SET preferences = v_merged_prefs,
        updated_at = NOW()
    WHERE user_id = p_user_id;

    RETURN FOUND;
EXCEPTION
    WHEN OTHERS THEN
        RAISE LOG 'Error updating preferences for user %: %', p_user_id, SQLERRM;
        RETURN FALSE;
END;
$$;

-- Trigger for audit logging
CREATE TRIGGER audit_user_changes
    AFTER UPDATE ON users
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
BEGIN
    INSERT INTO audit_log (
        table_name,
        record_id,
        action,
        new_values,
        new_values,
        changed_by,
        changed_at
    ) VALUES (
        'users',
        NEW.id,
        'UPDATE',
        json_object(OLD.*),
        json_object(NEW.*),
        USER(),
        NOW()
    );
END;

-- View with complex aggregations
CREATE VIEW user_engagement_summary AS
SELECT
    u.id,
    u.username,
    u.email,
    COUNT(DISTINCT ae.session_id) as total_sessions,
    COUNT(ae.id) as total_events,
    MIN(ae.occurred_at) as first_event,
    MAX(ae.occurred_at) as last_event,
    AVG(EXTRACT(EPOCH FROM (ae.occurred_at - LAG(ae.occurred_at) OVER (PARTITION BY u.id ORDER BY ae.occurred_at)))) as avg_time_between_events,
    EXTRACT(DAYS FROM (MAX(ae.occurred_at) - MIN(ae.occurred_at))) as engagement_span_days,
    CalculateUserScore(u.id) as user_score
FROM users u
LEFT JOIN analytics_events ae ON u.id = ae.user_id
WHERE u.is_active = TRUE
GROUP BY u.id, u.username, u.email
HAVING COUNT(ae.id) > 0;
"#;

        let symbols = extract_symbols(sql_code);

        let get_user_analytics = symbols
            .iter()
            .find(|s| s.name == "GetUserAnalytics" && s.kind == SymbolKind::Function);
        assert!(get_user_analytics.is_some());
        assert!(
            get_user_analytics
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE PROCEDURE")
        );

        let user_id_param = symbols.iter().find(|s| s.name == "p_user_id");
        assert!(user_id_param.is_some());
        assert!(
            user_id_param
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("IN p_user_id BIGINT")
        );

        let total_events_param = symbols.iter().find(|s| s.name == "p_total_events");
        assert!(total_events_param.is_some());
        assert!(
            total_events_param
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("OUT")
        );

        let calculate_user_score = symbols.iter().find(|s| s.name == "CalculateUserScore");
        assert!(calculate_user_score.is_some());
        assert!(
            calculate_user_score
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("RETURNS DECIMAL(10,2)")
        );

        let update_user_prefs = symbols.iter().find(|s| s.name == "update_user_preferences");
        assert!(update_user_prefs.is_some());
        assert!(
            update_user_prefs
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("RETURNS BOOLEAN")
        );
        assert!(
            update_user_prefs
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("LANGUAGE plpgsql")
        );

        let score_var = symbols.iter().find(|s| s.name == "v_score");
        assert!(score_var.is_some());
        assert!(
            score_var
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("DECLARE v_score DECIMAL(10,2)")
        );

        let current_prefs_var = symbols.iter().find(|s| s.name == "v_current_prefs");
        assert!(current_prefs_var.is_some());
        assert!(
            current_prefs_var
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("JSONB")
        );

        let audit_trigger = symbols.iter().find(|s| s.name == "audit_user_changes");
        assert!(audit_trigger.is_some());
        assert!(
            audit_trigger
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE TRIGGER")
        );
        assert!(
            audit_trigger
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("AFTER UPDATE ON users")
        );

        let engagement_view = symbols.iter().find(|s| s.name == "user_engagement_summary");
        assert!(engagement_view.is_some());
        assert!(
            engagement_view
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE VIEW")
        );

        let total_sessions = symbols.iter().find(|s| s.name == "total_sessions");
        assert!(total_sessions.is_some());

        let user_score = symbols.iter().find(|s| s.name == "user_score");
        assert!(user_score.is_some());
    }
}
