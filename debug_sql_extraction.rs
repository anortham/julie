// Debug SQL extraction to see what symbols are being extracted

use std::collections::HashMap;

// Temporary debug script to check SQL symbol extraction
fn main() {
    let sql_code = r#"
CREATE DOMAIN email_address AS VARCHAR(255)
    CHECK (VALUE ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$');

DELIMITER $$
CREATE PROCEDURE GetUserAnalytics(
    IN p_user_id BIGINT,
    IN p_start_date DATE,
    IN p_end_date DATE,
    OUT p_total_events INT,
    OUT p_unique_sessions INT
)
BEGIN
    SELECT COUNT(*) INTO p_total_events
    FROM analytics_events
    WHERE user_id = p_user_id
      AND event_date BETWEEN p_start_date AND p_end_date;

    SELECT COUNT(DISTINCT session_id) INTO p_unique_sessions
    FROM analytics_events
    WHERE user_id = p_user_id
      AND event_date BETWEEN p_start_date AND p_end_date;
END $$
DELIMITER ;
"#;

    println!("SQL Code to parse:");
    println!("{}", sql_code);
    println!("-------------------");

    // This would normally extract symbols but we can't run this here
    // Just showing what we're trying to extract:
    // 1. Domain: email_address
    // 2. Stored Procedure: GetUserAnalytics
}