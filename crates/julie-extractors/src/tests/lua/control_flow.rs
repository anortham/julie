use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_control_structures_and_loops() {
        let code = r#"
-- If statements with local variables
function checkAge(age)
  if age < 0 then
    local errorMsg = "Invalid age"
    error(errorMsg)
  elseif age < 18 then
    local status = "minor"
    return status
  elseif age < 65 then
    local status = "adult"
    return status
  else
    local status = "senior"
    return status
  end
end

-- Nested if statements
function processGrade(score)
  if score >= 0 and score <= 100 then
    if score >= 90 then
      local grade = "A"
      local comment = "Excellent"
      return grade, comment
    elseif score >= 80 then
      local grade = "B"
      return grade
    else
      local grade = "C or below"
      return grade
    end
  else
    error("Invalid score")
  end
end

-- For loops with local variables
function processNumbers()
  -- Numeric for loop
  for i = 1, 10 do
    local squared = i * i
    print("Square of " .. i .. " is " .. squared)
  end

  -- For loop with step
  for j = 10, 1, -1 do
    local countdown = "T-minus " .. j
    print(countdown)
  end

  -- Generic for loop with pairs
  local data = {name = "John", age = 30, city = "NYC"}
  for key, value in pairs(data) do
    local entry = key .. ": " .. tostring(value)
    print(entry)
  end

  -- Generic for loop with ipairs
  local fruits = {"apple", "banana", "orange"}
  for index, fruit in ipairs(fruits) do
    local item = "Item " .. index .. ": " .. fruit
    print(item)
  end
end

-- While loops
function waitForCondition()
  local attempts = 0
  local maxAttempts = 10
  local success = false

  while not success and attempts < maxAttempts do
    local result = performOperation()
    attempts = attempts + 1

    if result then
      success = true
      local message = "Success after " .. attempts .. " attempts"
      print(message)
    else
      local waitTime = attempts * 100
      sleep(waitTime)
    end
  end

  return success
end

-- Repeat-until loops
function readInput()
  local input
  local isValid = false

  repeat
    local prompt = "Enter a number (1-10): "
    io.write(prompt)
    input = io.read("*n")

    if input and input >= 1 and input <= 10 then
      isValid = true
      local confirmation = "You entered: " .. input
      print(confirmation)
    else
      local errorMsg = "Invalid input, please try again"
      print(errorMsg)
    end
  until isValid

  return input
end

-- Break and continue simulation
function processItems(items)
  local processed = {}
  local skipCount = 0

  for i = 1, #items do
    local item = items[i]

    -- Skip nil or empty items
    if not item or item == "" then
      skipCount = skipCount + 1
      goto continue
    end

    -- Break on special marker
    if item == "STOP" then
      break
    end

    -- Process valid item
    local processedItem = string.upper(item)
    table.insert(processed, processedItem)

    ::continue::
  end

  local summary = "Processed " .. #processed .. " items, skipped " .. skipCount
  print(summary)
  return processed
end

-- Nested loops with local scoping
function createMatrix(rows, cols)
  local matrix = {}

  for i = 1, rows do
    local row = {}

    for j = 1, cols do
      local value = i * cols + j
      row[j] = value

      -- Conditional processing within nested loop
      if value % 2 == 0 then
        local evenMarker = "even"
        row[j] = {value = value, type = evenMarker}
      else
        local oddMarker = "odd"
        row[j] = {value = value, type = oddMarker}
      end
    end

    matrix[i] = row
  end

  return matrix
end

-- Iterator functions
function fibonacci(n)
  local function iter(a, b, i)
    if i > n then
      return nil
    else
      local next = a + b
      return i, a, next
    end
  end

  return iter, 1, 0, 1
end

-- Custom iterator
function range(start, stop, step)
  local step = step or 1

  return function()
    if start <= stop then
      local current = start
      start = start + step
      return current
    end
  end
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "control.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Main functions
        let check_age = symbols.iter().find(|s| s.name == "checkAge");
        assert!(check_age.is_some());
        assert_eq!(check_age.unwrap().kind, SymbolKind::Function);

        let process_grade = symbols.iter().find(|s| s.name == "processGrade");
        assert!(process_grade.is_some());

        let process_numbers = symbols.iter().find(|s| s.name == "processNumbers");
        assert!(process_numbers.is_some());

        // Local variables in control structures should be detected
        let error_msg = symbols
            .iter()
            .find(|s| s.name == "errorMsg" && s.visibility == Some(Visibility::Private));
        assert!(error_msg.is_some());
        assert_eq!(error_msg.unwrap().kind, SymbolKind::Variable);

        let status = symbols
            .iter()
            .find(|s| s.name == "status" && s.visibility == Some(Visibility::Private));
        assert!(status.is_some());

        // Loop-related functions
        let wait_for_condition = symbols.iter().find(|s| s.name == "waitForCondition");
        assert!(wait_for_condition.is_some());

        let read_input = symbols.iter().find(|s| s.name == "readInput");
        assert!(read_input.is_some());

        let process_items = symbols.iter().find(|s| s.name == "processItems");
        assert!(process_items.is_some());

        let create_matrix = symbols.iter().find(|s| s.name == "createMatrix");
        assert!(create_matrix.is_some());

        // Iterator functions
        let fibonacci = symbols.iter().find(|s| s.name == "fibonacci");
        assert!(fibonacci.is_some());

        let range = symbols.iter().find(|s| s.name == "range");
        assert!(range.is_some());

        // Nested function in iterator
        let iter = symbols
            .iter()
            .find(|s| s.name == "iter" && s.parent_id == Some(fibonacci.unwrap().id.clone()));
        assert!(iter.is_some());
        assert_eq!(iter.unwrap().kind, SymbolKind::Function);
        assert_eq!(iter.unwrap().visibility, Some(Visibility::Private));
    }
}
