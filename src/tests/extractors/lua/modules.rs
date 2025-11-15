use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_modules_and_require_system() {
        let code = r#"
-- Require statements
local json = require("json")
local socket = require("socket")
local http = require("socket.http")
local lfs = require("lfs")

-- Relative requires
local utils = require("./utils")
local config = require("../config/settings")

-- Module definition pattern 1: using module()
module("mymodule", package.seeall)

function publicFunction()
  return "This is public"
end

local function privateFunction()
  return "This is private"
end

-- Module definition pattern 2: return table
local M = {}

function M.add(a, b)
  return a + b
end

function M.subtract(a, b)
  return a - b
end

function M.multiply(a, b)
  return a * b
end

M.PI = 3.14159
M.VERSION = "2.0.0"

local function helper()
  return "internal helper"
end

M.getInfo = function()
  return "Math module " .. M.VERSION
end

return M

-- Alternative module pattern
local math_utils = {}

math_utils.square = function(x)
  return x * x
end

math_utils.cube = function(x)
  return x * x * x
end

math_utils.factorial = function(n)
  if n <= 1 then
    return 1
  else
    return n * math_utils.factorial(n - 1)
  end
end

-- Export selected functions
return {
  square = math_utils.square,
  cube = math_utils.cube,
  factorial = math_utils.factorial,
  constants = {
    E = 2.71828,
    PI = 3.14159
  }
}

-- Package initialization
if not package.loaded["mypackage"] then
  package.loaded["mypackage"] = {}
end

local mypackage = package.loaded["mypackage"]

mypackage.init = function()
  print("Package initialized")
end

mypackage.cleanup = function()
  print("Package cleaned up")
end

-- Conditional loading
local success, lib = pcall(require, "optional_library")
if success then
  -- Use the library
  lib.configure({debug = true})
else
  print("Optional library not available")
end

-- Dynamic require
local function loadModule(name)
  local success, module = pcall(require, name)
  if success then
    return module
  else
    error("Failed to load module: " .. name)
  end
end

-- Module caching pattern
local cache = {}

local function getCachedModule(name)
  if not cache[name] then
    cache[name] = require(name)
  end
  return cache[name]
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "modules.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Require statements
        let json_require = symbols.iter().find(|s| {
            s.name == "json" && s.signature.as_ref().unwrap().contains("require(\"json\")")
        });
        assert!(json_require.is_some());
        assert_eq!(json_require.unwrap().kind, SymbolKind::Import);

        let socket_require = symbols.iter().find(|s| {
            s.name == "socket"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("require(\"socket\")")
        });
        assert!(socket_require.is_some());

        let http_require = symbols.iter().find(|s| {
            s.name == "http"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("require(\"socket.http\")")
        });
        assert!(http_require.is_some());

        // Relative requires
        let utils_require = symbols.iter().find(|s| {
            s.name == "utils"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("require(\"./utils\")")
        });
        assert!(utils_require.is_some());

        let config_require = symbols.iter().find(|s| {
            s.name == "config"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("require(\"../config/settings\")")
        });
        assert!(config_require.is_some());

        // Module functions
        let public_function = symbols.iter().find(|s| s.name == "publicFunction");
        assert!(public_function.is_some());
        assert_eq!(public_function.unwrap().kind, SymbolKind::Function);
        assert_eq!(
            public_function.unwrap().visibility,
            Some(Visibility::Public)
        );

        let private_function = symbols.iter().find(|s| s.name == "privateFunction");
        assert!(private_function.is_some());
        assert_eq!(
            private_function.unwrap().visibility,
            Some(Visibility::Private)
        );

        // Module table pattern
        let m = symbols.iter().find(|s| s.name == "M");
        assert!(m.is_some());
        assert_eq!(m.unwrap().kind, SymbolKind::Variable);

        // Module methods
        let add_method = symbols
            .iter()
            .find(|s| s.name == "add" && s.parent_id == Some(m.unwrap().id.clone()));
        assert!(add_method.is_some());
        assert_eq!(add_method.unwrap().kind, SymbolKind::Method);

        let subtract_method = symbols
            .iter()
            .find(|s| s.name == "subtract" && s.parent_id == Some(m.unwrap().id.clone()));
        assert!(subtract_method.is_some());

        // Module constants
        let module_pi = symbols
            .iter()
            .find(|s| s.name == "PI" && s.parent_id == Some(m.unwrap().id.clone()));
        assert!(module_pi.is_some());
        assert_eq!(module_pi.unwrap().kind, SymbolKind::Field);

        // Alternative module pattern
        let math_utils = symbols.iter().find(|s| s.name == "math_utils");
        assert!(math_utils.is_some());

        let square = symbols
            .iter()
            .find(|s| s.name == "square" && s.parent_id == Some(math_utils.unwrap().id.clone()));
        assert!(square.is_some());
        assert_eq!(square.unwrap().kind, SymbolKind::Method);

        // Package functions
        let load_module = symbols.iter().find(|s| s.name == "loadModule");
        assert!(load_module.is_some());
        assert!(
            load_module
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("local function loadModule(name)")
        );

        let get_cached_module = symbols.iter().find(|s| s.name == "getCachedModule");
        assert!(get_cached_module.is_some());
        assert!(
            get_cached_module
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("local function getCachedModule(name)")
        );
    }
}
