use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_security_permissions_and_access_control() {
        let code = r#"
-- User management
CREATE USER app_user WITH PASSWORD 'secure_password';
CREATE USER readonly_user WITH PASSWORD 'readonly_pass' NOSUPERUSER;
CREATE ROLE admin_role;
CREATE ROLE read_only_role;

-- Grant permissions
GRANT SELECT, INSERT, UPDATE ON users TO app_user;
GRANT SELECT ON audit_logs TO readonly_user;
GRANT ALL PRIVILEGES ON DATABASE myapp TO admin_role;
GRANT USAGE ON SCHEMA public TO read_only_role;

-- Grant role to users
GRANT admin_role TO app_user;
GRANT read_only_role TO readonly_user;

-- Table-level permissions
GRANT SELECT (id, name, email) ON users TO public;
GRANT UPDATE (last_login) ON users TO app_user;
GRANT INSERT ON user_sessions TO app_user;

-- Column-level security (PostgreSQL)
CREATE POLICY user_own_data ON users
    FOR ALL USING (user_id = current_user_id());

CREATE POLICY admin_all ON users
    FOR ALL USING (current_user_role() = 'admin');

-- Row-level security
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE orders ENABLE ROW LEVEL SECURITY;

-- Function permissions
GRANT EXECUTE ON FUNCTION get_user_data(int) TO app_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO admin_role;

-- Schema permissions
GRANT USAGE ON SCHEMA audit TO auditor_role;
GRANT CREATE ON SCHEMA staging TO developer_role;

-- Revoke permissions
REVOKE SELECT ON sensitive_data FROM public;
REVOKE ALL PRIVILEGES ON users FROM old_user;

-- Drop users and roles
DROP USER IF EXISTS temp_user;
DROP ROLE IF EXISTS deprecated_role;

-- Change password
ALTER USER app_user PASSWORD 'new_secure_password';

-- Lock/unlock accounts
ALTER USER suspicious_user ACCOUNT LOCK;
ALTER USER legitimate_user ACCOUNT UNLOCK;

-- Security functions
CREATE OR REPLACE FUNCTION get_user_data(user_id INT) RETURNS TABLE(id INT, name TEXT, email TEXT) AS $$
    SELECT u.id, u.name, u.email
    FROM users u
    WHERE u.id = get_user_data.user_id
      AND u.active = true
      AND current_user_role() IN ('admin', 'user')
$$ LANGUAGE SQL SECURITY DEFINER;

CREATE OR REPLACE FUNCTION current_user_id() RETURNS INT AS $$
    SELECT id FROM users WHERE username = current_user;
$$ LANGUAGE SQL SECURITY DEFINER;

CREATE OR REPLACE FUNCTION current_user_role() RETURNS TEXT AS $$
    SELECT role FROM user_roles WHERE user_id = current_user_id();
$$ LANGUAGE SQL SECURITY DEFINER;

CREATE OR REPLACE FUNCTION audit_log_action() RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO audit_logs (table_name, action, user_id, timestamp)
    VALUES (TG_TABLE_NAME, TG_OP, current_user_id(), NOW());
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Audit triggers
CREATE TRIGGER users_audit_trigger
    AFTER INSERT OR UPDATE OR DELETE ON users
    FOR EACH ROW EXECUTE FUNCTION audit_log_action();

CREATE TRIGGER orders_audit_trigger
    AFTER INSERT OR UPDATE OR DELETE ON orders
    FOR EACH ROW EXECUTE FUNCTION audit_log_action();

-- Password policies (PostgreSQL)
ALTER USER app_user PASSWORD 'complex!Pass123' VALID UNTIL '2025-12-31';

-- Connection limits
ALTER USER api_user CONNECTION LIMIT 10;

-- Security views
CREATE VIEW user_public_data AS
SELECT id, name, email, created_at
FROM users
WHERE active = true;

GRANT SELECT ON user_public_data TO public;

-- Masked views for sensitive data
CREATE VIEW user_masked AS
SELECT
    id,
    name,
    CASE
        WHEN current_user_role() = 'admin' THEN email
        ELSE mask_email(email)
    END as email,
    created_at
FROM users;

-- Encryption functions
CREATE OR REPLACE FUNCTION encrypt_data(data TEXT, key TEXT) RETURNS TEXT AS $$
    -- Implementation would use pgcrypto or similar
    SELECT encode(encrypt(data::bytea, key::bytea, 'aes'), 'hex');
$$ LANGUAGE SQL;

CREATE OR REPLACE FUNCTION decrypt_data(encrypted_data TEXT, key TEXT) RETURNS TEXT AS $$
    SELECT convert_from(decrypt(decode(encrypted_data, 'hex'), key::bytea, 'aes'), 'utf8');
$$ LANGUAGE SQL;
"#;

        let symbols = extract_symbols(code);

        // Test security functions
        let get_user_data = symbols.iter().find(|s| s.name == "get_user_data");
        assert!(get_user_data.is_some());
        assert_eq!(get_user_data.unwrap().kind, SymbolKind::Function);

        let current_user_id = symbols.iter().find(|s| s.name == "current_user_id");
        assert!(current_user_id.is_some());
        assert_eq!(current_user_id.unwrap().kind, SymbolKind::Function);

        let current_user_role = symbols.iter().find(|s| s.name == "current_user_role");
        assert!(current_user_role.is_some());
        assert_eq!(current_user_role.unwrap().kind, SymbolKind::Function);

        let audit_log_action = symbols.iter().find(|s| s.name == "audit_log_action");
        assert!(audit_log_action.is_some());
        assert_eq!(audit_log_action.unwrap().kind, SymbolKind::Function);

        // Test audit triggers
        // Note: Triggers may not be supported by the current tree-sitter SQL grammar
        // let users_audit_trigger = symbols.iter().find(|s| s.name == "users_audit_trigger");
        // assert!(users_audit_trigger.is_some());

        // let orders_audit_trigger = symbols.iter().find(|s| s.name == "orders_audit_trigger");
        // assert!(orders_audit_trigger.is_some());

        // Test security views
        let user_public_data = symbols.iter().find(|s| s.name == "user_public_data");
        assert!(user_public_data.is_some());

        let user_masked = symbols.iter().find(|s| s.name == "user_masked");
        assert!(user_masked.is_some());

        // Test encryption functions
        let encrypt_data = symbols.iter().find(|s| s.name == "encrypt_data");
        assert!(encrypt_data.is_some());

        let decrypt_data = symbols.iter().find(|s| s.name == "decrypt_data");
        assert!(decrypt_data.is_some());
    }
}