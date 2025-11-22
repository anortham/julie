use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_string_patterns_and_regex() {
        let code = r#"
local function validateEmail(email)
  local pattern = "^[%w._-]+@[%w.-]+%.%w+$"
  return string.match(email, pattern) ~= nil
end

local function extractNumbers(text)
  local numbers = {}
  for number in string.gmatch(text, "%d+") do
    table.insert(numbers, tonumber(number))
  end
  return numbers
end

local function parseLogLine(line)
  local pattern = "(%d+%-%d+%-%d+) (%d+:%d+:%d+) %[(%w+)%] (.+)"
  local date, time, level, message = string.match(line, pattern)

  if date then
    return {
      date = date,
      time = time,
      level = level,
      message = message
    }
  else
    return nil
  end
end

local function extractUrls(text)
  local urls = {}
  local pattern = "https?://[%w%-._~:/?#%[%]@!$&'()*+,;=]+"

  for url in string.gmatch(text, pattern) do
    table.insert(urls, url)
  end

  return urls
end

local function cleanHtml(html)
  local cleaned = string.gsub(html, "<[^>]*>", "")

  local entities = {
    ["&lt;"] = "<",
    ["&gt;"] = ">",
    ["&amp;"] = "&",
    ["&quot;"] = '"',
    ["&apos;"] = "'",
    ["&#39;"] = "'"
  }

  for entity, replacement in pairs(entities) do
    cleaned = string.gsub(cleaned, entity, replacement)
  end

  cleaned = string.gsub(cleaned, "%s+", " ")
  cleaned = string.gsub(cleaned, "^%s+", "")
  cleaned = string.gsub(cleaned, "%s+$", "")

  return cleaned
end

local function formatPhoneNumber(phone)
  local digits = string.gsub(phone, "%D", "")

  if #digits == 10 then
    local area = string.sub(digits, 1, 3)
    local exchange = string.sub(digits, 4, 6)
    local number = string.sub(digits, 7, 10)
    return "(" .. area .. ") " .. exchange .. "-" .. number
  elseif #digits == 11 and string.sub(digits, 1, 1) == "1" then
    local area = string.sub(digits, 2, 4)
    local exchange = string.sub(digits, 5, 7)
    local number = string.sub(digits, 8, 11)
    return "+1 (" .. area .. ") " .. exchange .. "-" .. number
  else
    return digits
  end
end

local function processTemplate(template, variables)
  local result = template

  result = string.gsub(result, "{{(%w+)}}", function(varName)
    return tostring(variables[varName] or "")
  end)

  result = string.gsub(result, "{(%w+)}", function(varName)
    return tostring(variables[varName] or "")
  end)

  return result
end

local function parseQueryString(queryString)
  local params = {}

  for pair in string.gmatch(queryString, "[^&]+") do
    local key, value = string.match(pair, "([^=]+)=([^=]*)")
    if key and value then
      key = string.gsub(key, "+", " ")
      key = string.gsub(key, "%%(%x%x)", function(hex)
        return string.char(tonumber(hex, 16))
      end)

      value = string.gsub(value, "+", " ")
      value = string.gsub(value, "%%(%x%x)", function(hex)
        return string.char(tonumber(hex, 16))
      end)

      params[key] = value
    end
  end

  return params
end

local function parseCSV(text)
  local rows = {}

  for line in string.gmatch(text, "[^\r\n]+") do
    local row = {}
    local pattern = '([^,]+)'

    string.gsub(line, pattern, function(value)
      value = string.gsub(value, '^%s+', '')
      value = string.gsub(value, '%s+$', '')
      table.insert(row, value)
    end)

    table.insert(rows, row)
  end

  return rows
end

local Validator = {}
Validator.__index = Validator

function Validator:new(rules)
  local instance = {
    rules = rules or {}
  }
  setmetatable(instance, Validator)
  return instance
end

function Validator:validate(data)
  local errors = {}

  for field, rule in pairs(self.rules) do
    local value = data[field]

    if rule.required and (value == nil or value == '') then
      table.insert(errors, field .. " is required")
    end

    if rule.pattern and value then
      if not string.match(value, rule.pattern) then
        table.insert(errors, field .. " has invalid format")
      end
    end
  end

  return errors
end

local TextUtils = {}

function TextUtils.splitLines(text)
  local lines = {}
  for line in string.gmatch(text, "[^\r\n]+") do
    table.insert(lines, line)
  end
  return lines
end

function TextUtils.splitWords(text)
  local words = {}
  for word in string.gmatch(text, "%S+") do
    table.insert(words, word)
  end
  return words
end

function TextUtils.capitalize(text)
  return string.gsub(text, "(%l)(%w*)", function(first, rest)
    return string.upper(first) .. rest
  end)
end

function TextUtils.camelCase(text)
  local result = string.gsub(text, "[-_](%l)", function(letter)
    return string.upper(letter)
  end)
  return string.gsub(result, "^%u", string.lower)
end

function TextUtils.snakeCase(text)
  local result = string.gsub(text, "(%u)", function(letter)
    return "_" .. string.lower(letter)
  end)
  return string.gsub(result, "^_", "")
end

function TextUtils.truncate(text, maxLength, suffix)
  suffix = suffix or "..."
  if #text <= maxLength then
    return text
  else
    return string.sub(text, 1, maxLength - #suffix) .. suffix
  end
end

local function escapePattern(text)
  return string.gsub(text, "([%^%$%(%)%%%.%[%]%*%+%-%?])", "%%%1")
end

local function replaceAll(text, search, replacement)
  local escapedSearch = escapePattern(search)
  return string.gsub(text, escapedSearch, replacement)
end

local function contains(text, substring, ignoreCase)
  if ignoreCase then
    text = string.lower(text)
    substring = string.lower(substring)
  end

  return string.find(text, escapePattern(substring), 1, true) ~= nil
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "strings.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let validate_email = symbols.iter().find(|s| s.name == "validateEmail");
        assert!(validate_email.is_some());

        let extract_numbers = symbols.iter().find(|s| s.name == "extractNumbers");
        assert!(extract_numbers.is_some());

        let parse_log_line = symbols.iter().find(|s| s.name == "parseLogLine");
        assert!(parse_log_line.is_some());

        let extract_urls = symbols.iter().find(|s| s.name == "extractUrls");
        assert!(extract_urls.is_some());

        let clean_html = symbols.iter().find(|s| s.name == "cleanHtml");
        assert!(clean_html.is_some());

        let format_phone_number = symbols.iter().find(|s| s.name == "formatPhoneNumber");
        assert!(format_phone_number.is_some());

        let process_template = symbols.iter().find(|s| s.name == "processTemplate");
        assert!(process_template.is_some());

        let parse_query_string = symbols.iter().find(|s| s.name == "parseQueryString");
        assert!(parse_query_string.is_some());

        let parse_csv = symbols.iter().find(|s| s.name == "parseCSV");
        assert!(parse_csv.is_some());

        let validator = symbols.iter().find(|s| s.name == "Validator");
        if validator.is_some() {
            assert_eq!(validator.unwrap().kind, SymbolKind::Class);
        }

        let text_utils = symbols.iter().find(|s| s.name == "TextUtils");
        if text_utils.is_some() {
            assert_eq!(text_utils.unwrap().kind, SymbolKind::Variable);
        }

        let escape_pattern = symbols.iter().find(|s| s.name == "escapePattern");
        assert!(escape_pattern.is_some());

        let replace_all = symbols.iter().find(|s| s.name == "replaceAll");
        assert!(replace_all.is_some());

        let contains_fn = symbols.iter().find(|s| s.name == "contains");
        assert!(contains_fn.is_some());
    }
}
