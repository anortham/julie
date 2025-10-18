use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_handling_and_pcall_patterns() {
        let code = r#"
-- Basic error handling with pcall
local function safeDivide(a, b)
    if b == 0 then
        error("Division by zero")
    end
    return a / b
end

local function calculate()
    local success, result = pcall(safeDivide, 10, 0)
    if not success then
        print("Error occurred: " .. result)
        return nil
    end
    return result
end

-- xpcall with error handler
local function errorHandler(err)
    return "Caught error: " .. err
end

local function riskyOperation()
    local data = nil
    return data.someField  -- This will error
end

local function safeRiskyOperation()
    local success, result = xpcall(riskyOperation, errorHandler)
    if success then
        return result
    else
        print("Handled error: " .. result)
        return nil
    end
end

-- Custom error objects
local function createError(message, code)
    return {
        message = message,
        code = code,
        timestamp = os.time(),
        stack = debug.traceback()
    }
end

local function validateUser(user)
    if not user.name then
        error(createError("User name is required", 400))
    end
    if not user.email then
        error(createError("User email is required", 400))
    end
    return true
end

-- Try-catch simulation
local function tryCatch(tryBlock, catchBlock)
    local success, result = pcall(tryBlock)
    if not success then
        return catchBlock(result)
    end
    return result
end

local function exampleUsage()
    local result = tryCatch(
        function()
            return safeDivide(10, 2)
        end,
        function(err)
            return "Default value: 0"
        end
    )
    return result
end

-- Assert with custom error messages
local function assertWithMessage(condition, message)
    if not condition then
        error(message, 2)  -- Level 2 to skip this function in stack trace
    end
end

local function processData(data)
    assertWithMessage(data ~= nil, "Data cannot be nil")
    assertWithMessage(type(data) == "table", "Data must be a table")
    assertWithMessage(#data > 0, "Data table cannot be empty")

    return "Data processed successfully"
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "error_handling.lua".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Test error handling functions
        let safe_divide = symbols.iter().find(|s| s.name == "safeDivide");
        assert!(safe_divide.is_some());
        assert_eq!(safe_divide.unwrap().kind, SymbolKind::Function);

        let calculate = symbols.iter().find(|s| s.name == "calculate");
        assert!(calculate.is_some());
        assert_eq!(calculate.unwrap().kind, SymbolKind::Function);

        let error_handler = symbols.iter().find(|s| s.name == "errorHandler");
        assert!(error_handler.is_some());
        assert_eq!(error_handler.unwrap().kind, SymbolKind::Function);

        let risky_operation = symbols.iter().find(|s| s.name == "riskyOperation");
        assert!(risky_operation.is_some());

        let safe_risky_operation = symbols.iter().find(|s| s.name == "safeRiskyOperation");
        assert!(safe_risky_operation.is_some());

        // Test custom error functions
        let create_error = symbols.iter().find(|s| s.name == "createError");
        assert!(create_error.is_some());
        assert_eq!(create_error.unwrap().kind, SymbolKind::Function);

        let validate_user = symbols.iter().find(|s| s.name == "validateUser");
        assert!(validate_user.is_some());

        // Test utility functions
        let try_catch = symbols.iter().find(|s| s.name == "tryCatch");
        assert!(try_catch.is_some());

        let example_usage = symbols.iter().find(|s| s.name == "exampleUsage");
        assert!(example_usage.is_some());

        let assert_with_message = symbols.iter().find(|s| s.name == "assertWithMessage");
        assert!(assert_with_message.is_some());

        let process_data = symbols.iter().find(|s| s.name == "processData");
        assert!(process_data.is_some());
    }
}
