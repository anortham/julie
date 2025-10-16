use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_transactions_and_concurrency_control() {
        let code = r#"
-- Table definitions
CREATE TABLE accounts (
    id SERIAL PRIMARY KEY,
    balance DECIMAL(10,2) NOT NULL DEFAULT 0
);

CREATE TABLE orders (
    id SERIAL PRIMARY KEY,
    customer_id INT NOT NULL,
    total DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE order_items (
    id SERIAL PRIMARY KEY,
    order_id INT NOT NULL REFERENCES orders(id),
    product_id INT NOT NULL,
    quantity INT NOT NULL,
    price DECIMAL(10,2) NOT NULL
);

CREATE TABLE inventory (
    product_id INT PRIMARY KEY,
    quantity INT NOT NULL DEFAULT 0
);

CREATE TABLE products (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    stock INT NOT NULL DEFAULT 0
);

CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    version INT NOT NULL DEFAULT 1
);

CREATE TABLE user_actions (
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id),
    action VARCHAR(255) NOT NULL,
    error TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE large_table (
    id SERIAL PRIMARY KEY,
    status VARCHAR(50) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Basic transactions
BEGIN;
    UPDATE accounts SET balance = balance - 100 WHERE id = 1;
    UPDATE accounts SET balance = balance + 100 WHERE id = 2;
COMMIT;

-- Transaction with savepoints
BEGIN;
    INSERT INTO orders (customer_id, total) VALUES (123, 99.99);

    SAVEPOINT order_created;

    INSERT INTO order_items (order_id, product_id, quantity)
    VALUES (currval('orders_id_seq'), 456, 2);

    -- If something goes wrong, rollback to savepoint
    -- ROLLBACK TO SAVEPOINT order_created;

COMMIT;

-- Transaction isolation levels
BEGIN ISOLATION LEVEL SERIALIZABLE;
    SELECT * FROM inventory WHERE product_id = 123 FOR UPDATE;
    UPDATE inventory SET quantity = quantity - 1 WHERE product_id = 123;
COMMIT;

BEGIN ISOLATION LEVEL READ COMMITTED;
    SELECT balance FROM accounts WHERE id = 1;
COMMIT;

-- Explicit locking
BEGIN;
    SELECT * FROM products WHERE id = 123 FOR UPDATE;
    UPDATE products SET stock = stock - 1 WHERE id = 123;
COMMIT;

-- Advisory locks
SELECT pg_advisory_lock(12345);
-- Do some work...
SELECT pg_advisory_unlock(12345);

-- Row-level locking with different modes
SELECT * FROM orders WHERE status = 'pending' FOR SHARE;
SELECT * FROM inventory WHERE product_id = 123 FOR UPDATE NOWAIT;

-- Table-level locks
LOCK TABLE orders IN EXCLUSIVE MODE;
LOCK TABLE inventory IN ACCESS SHARE MODE;

-- Deadlock handling
CREATE OR REPLACE FUNCTION transfer_funds(
    from_account INT,
    to_account INT,
    amount DECIMAL
) RETURNS BOOLEAN AS $$
DECLARE
    available_balance DECIMAL;
BEGIN
    -- Lock accounts in consistent order to prevent deadlocks
    IF from_account < to_account THEN
        SELECT balance INTO available_balance FROM accounts
        WHERE id = from_account FOR UPDATE;

        UPDATE accounts SET balance = balance - amount
        WHERE id = from_account;

        UPDATE accounts SET balance = balance + amount
        WHERE id = to_account;
    ELSE
        SELECT balance INTO available_balance FROM accounts
        WHERE id = to_account FOR UPDATE;

        UPDATE accounts SET balance = balance - amount
        WHERE id = from_account;

        UPDATE accounts SET balance = balance + amount
        WHERE id = to_account;
    END IF;

    RETURN TRUE;
EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE 'Transfer failed: %', SQLERRM;
        RETURN FALSE;
END;
$$ LANGUAGE plpgsql;

-- Optimistic concurrency control
CREATE OR REPLACE FUNCTION update_user_version(
    user_id INT,
    new_name TEXT,
    current_version INT
) RETURNS BOOLEAN AS $$
BEGIN
    UPDATE users
    SET name = new_name, version = version + 1
    WHERE id = user_id AND version = current_version;

    IF FOUND THEN
        RETURN TRUE;
    ELSE
        RAISE EXCEPTION 'Concurrent update detected for user %', user_id;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Transaction with timeout
BEGIN;
    SET LOCAL statement_timeout = '5s';
    SET LOCAL lock_timeout = '2s';

    -- Operations that might take time
    UPDATE large_table SET status = 'processed'
    WHERE created_at < NOW() - INTERVAL '1 hour';

COMMIT;

-- Nested transactions (savepoints)
CREATE OR REPLACE FUNCTION complex_business_logic(user_id INT) RETURNS VOID AS $$
BEGIN
    BEGIN
        -- Outer transaction operations
        INSERT INTO user_actions (user_id, action) VALUES (user_id, 'start');

        -- Nested transaction-like operations with savepoints
        SAVEPOINT before_inventory_update;

        UPDATE inventory SET reserved = reserved + 1 WHERE product_id = 123;

        -- Call another function that might fail
        PERFORM process_payment(user_id, 99.99);

        RELEASE SAVEPOINT before_inventory_update;

        INSERT INTO user_actions (user_id, action) VALUES (user_id, 'complete');

    EXCEPTION
        WHEN OTHERS THEN
            ROLLBACK TO SAVEPOINT before_inventory_update;
            INSERT INTO user_actions (user_id, action, error)
            VALUES (user_id, 'failed', SQLERRM);
            RAISE;
    END;
END;
$$ LANGUAGE plpgsql;

-- Two-phase commit (prepared transactions)
BEGIN;
    UPDATE accounts SET balance = balance - 100 WHERE id = 1;
    PREPARE TRANSACTION 'debit_account_1';
-- In another session:
-- BEGIN;
--     UPDATE accounts SET balance = balance + 100 WHERE id = 2;
--     PREPARE TRANSACTION 'credit_account_2';
-- Then commit both:
-- COMMIT PREPARED 'debit_account_1';
-- COMMIT PREPARED 'credit_account_2';

-- Transaction monitoring
SELECT
    datname,
    usename,
    state,
    query_start,
    state_change
FROM pg_stat_activity
WHERE state = 'active'
ORDER BY query_start;

-- Long-running transaction detection
SELECT
    pid,
    datname,
    usename,
    query_start,
    now() - query_start as duration
FROM pg_stat_activity
WHERE state = 'active'
    AND now() - query_start > interval '1 minute'
ORDER BY duration DESC;
"#;

        let symbols = extract_symbols(code);

        // Test transaction-related functions
        let transfer_funds = symbols.iter().find(|s| s.name == "transfer_funds");
        assert!(transfer_funds.is_some());
        assert_eq!(transfer_funds.unwrap().kind, SymbolKind::Function);

        let update_user_version = symbols.iter().find(|s| s.name == "update_user_version");
        assert!(update_user_version.is_some());

        let complex_business_logic = symbols.iter().find(|s| s.name == "complex_business_logic");
        assert!(complex_business_logic.is_some());

        // Test transaction-related variables/tables
        let orders = symbols.iter().find(|s| s.name == "orders");
        assert!(orders.is_some());

        let order_items = symbols.iter().find(|s| s.name == "order_items");
        assert!(order_items.is_some());

        let inventory = symbols.iter().find(|s| s.name == "inventory");
        assert!(inventory.is_some());

        let accounts = symbols.iter().find(|s| s.name == "accounts");
        assert!(accounts.is_some());

        let products = symbols.iter().find(|s| s.name == "products");
        assert!(products.is_some());

        let user_actions = symbols.iter().find(|s| s.name == "user_actions");
        assert!(user_actions.is_some());

        let users = symbols.iter().find(|s| s.name == "users");
        assert!(users.is_some());

        let large_table = symbols.iter().find(|s| s.name == "large_table");
        assert!(large_table.is_some());
    }
}