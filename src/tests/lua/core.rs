use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functions_and_variables() {
        let code = r#"
-- Global function
function calculateArea(width, height)
  return width * height
end

-- Local function
local function validateInput(value)
  if type(value) ~= "number" then
    error("Expected number, got " .. type(value))
  end
  return true
end

-- Anonymous function assigned to variable
local multiply = function(a, b)
  return a * b
end

-- Arrow-like function using short syntax
local add = function(x, y) return x + y end

-- Global variables
PI = 3.14159
VERSION = "1.0.0"

-- Local variables
local userName = "John Doe"
local userAge = 30
local isActive = true
local items = {}

-- Multiple assignment
local x, y, z = 10, 20, 30
local first, second = "hello", "world"

-- Function with multiple return values
function getCoordinates()
  return 100, 200
end

-- Function with varargs
function sum(...)
  local args = {...}
  local total = 0
  for i = 1, #args do
    total = total + args[i]
  end
  return total
end

-- Function with default parameter simulation
function greet(name)
  name = name or "World"
  return "Hello, " .. name .. "!"
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            LuaExtractor::new("lua".to_string(), "test.lua".to_string(), code.to_string());

        let symbols = extractor.extract_symbols(&tree);

        // Global function
        let calculate_area = symbols.iter().find(|s| s.name == "calculateArea");
        assert!(calculate_area.is_some());
        assert_eq!(calculate_area.unwrap().kind, SymbolKind::Function);
        assert!(calculate_area
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function calculateArea(width, height)"));
        assert_eq!(calculate_area.unwrap().visibility, Some(Visibility::Public));

        // Local function
        let validate_input = symbols.iter().find(|s| s.name == "validateInput");
        assert!(validate_input.is_some());
        assert_eq!(validate_input.unwrap().kind, SymbolKind::Function);
        assert!(validate_input
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("local function validateInput(value)"));
        assert_eq!(
            validate_input.unwrap().visibility,
            Some(Visibility::Private)
        );

        // Anonymous function
        let multiply = symbols.iter().find(|s| s.name == "multiply");
        assert!(multiply.is_some());
        assert_eq!(multiply.unwrap().kind, SymbolKind::Function);
        assert!(multiply
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("local multiply = function(a, b)"));

        // Short function
        let add = symbols.iter().find(|s| s.name == "add");
        assert!(add.is_some());
        assert!(add
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("local add = function(x, y)"));

        // Global variables
        let pi = symbols.iter().find(|s| s.name == "PI");
        assert!(pi.is_some());
        assert_eq!(pi.unwrap().kind, SymbolKind::Variable);
        assert_eq!(pi.unwrap().visibility, Some(Visibility::Public));

        let version = symbols.iter().find(|s| s.name == "VERSION");
        assert!(version.is_some());
        assert!(version
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("VERSION = \"1.0.0\""));

        // Local variables
        let user_name = symbols.iter().find(|s| s.name == "userName");
        assert!(user_name.is_some());
        assert_eq!(user_name.unwrap().kind, SymbolKind::Variable);
        assert_eq!(user_name.unwrap().visibility, Some(Visibility::Private));
        assert!(user_name
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("local userName = \"John Doe\""));

        let is_active = symbols.iter().find(|s| s.name == "isActive");
        assert!(is_active.is_some());
        assert!(is_active
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("local isActive = true"));

        // Multiple assignment variables
        let x = symbols.iter().find(|s| s.name == "x");
        assert!(x.is_some());
        assert!(x
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("local x, y, z = 10, 20, 30"));

        // Functions with special features
        let get_coordinates = symbols.iter().find(|s| s.name == "getCoordinates");
        assert!(get_coordinates.is_some());
        assert!(get_coordinates
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function getCoordinates()"));

        let sum_fn = symbols.iter().find(|s| s.name == "sum");
        assert!(sum_fn.is_some());
        assert!(sum_fn
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function sum(...)"));

        let greet = symbols.iter().find(|s| s.name == "greet");
        assert!(greet.is_some());
        assert!(greet
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function greet(name)"));
    }
}
