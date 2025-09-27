use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tables_and_data_structures() {
        let code = r#"
-- Simple table
local config = {
  host = "localhost",
  port = 3000,
  debug = true
}

-- Table with numeric indices
local colors = {"red", "green", "blue"}

-- Mixed table
local mixed = {
  [1] = "first",
  [2] = "second",
  name = "mixed table",
  count = 42
}

-- Table with functions (methods)
local calculator = {
  value = 0,

  add = function(self, num)
    self.value = self.value + num
    return self
  end,

  subtract = function(self, num)
    self.value = self.value - num
    return self
  end,

  getValue = function(self)
    return self.value
  end
}

-- Table with colon syntax method definition
function calculator:multiply(num)
  self.value = self.value * num
  return self
end

function calculator:divide(num)
  if num ~= 0 then
    self.value = self.value / num
  end
  return self
end

-- Nested tables
local database = {
  users = {
    {id = 1, name = "Alice", active = true},
    {id = 2, name = "Bob", active = false}
  },

  settings = {
    theme = "dark",
    language = "en",
    notifications = {
      email = true,
      push = false,
      sms = true
    }
  },

  methods = {
    findUser = function(id)
      for _, user in ipairs(database.users) do
        if user.id == id then
          return user
        end
      end
      return nil
    end,

    addUser = function(user)
      table.insert(database.users, user)
    end
  }
}

-- Constructor pattern
function Person(name, age)
  local self = {
    name = name,
    age = age
  }

  function self:getName()
    return self.name
  end

  function self:getAge()
    return self.age
  end

  function self:setAge(newAge)
    if newAge >= 0 then
      self.age = newAge
    end
  end

  return self
end

-- Class-like pattern with metatable
local Animal = {}
Animal.__index = Animal

function Animal:new(species, name)
  local instance = setmetatable({}, Animal)
  instance.species = species
  instance.name = name
  return instance
end

function Animal:speak()
  return self.name .. " makes a sound"
end

function Animal:getInfo()
  return "Species: " .. self.species .. ", Name: " .. self.name
end

-- Inheritance pattern
local Dog = setmetatable({}, Animal)
Dog.__index = Dog

function Dog:new(name, breed)
  local instance = Animal.new(self, "dog", name)
  setmetatable(instance, Dog)
  instance.breed = breed
  return instance
end

function Dog:speak()
  return self.name .. " barks!"
end

function Dog:getBreed()
  return self.breed
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "tables.lua".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Simple table
        let config = symbols.iter().find(|s| s.name == "config");
        assert!(config.is_some());
        assert_eq!(config.unwrap().kind, SymbolKind::Variable);

        // Array-like table
        let colors = symbols.iter().find(|s| s.name == "colors");
        assert!(colors.is_some());

        // Calculator table with methods
        let calculator = symbols.iter().find(|s| s.name == "calculator");
        assert!(calculator.is_some());
        assert_eq!(calculator.unwrap().kind, SymbolKind::Variable);

        // Table field
        let value = symbols
            .iter()
            .find(|s| s.name == "value" && s.parent_id == Some(calculator.unwrap().id.clone()));
        assert!(value.is_some());
        assert_eq!(value.unwrap().kind, SymbolKind::Field);

        // Table methods
        let add_method = symbols
            .iter()
            .find(|s| s.name == "add" && s.parent_id == Some(calculator.unwrap().id.clone()));
        assert!(add_method.is_some());
        assert_eq!(add_method.unwrap().kind, SymbolKind::Method);

        let subtract_method = symbols
            .iter()
            .find(|s| s.name == "subtract" && s.parent_id == Some(calculator.unwrap().id.clone()));
        assert!(subtract_method.is_some());

        // Colon syntax methods
        let multiply = symbols.iter().find(|s| {
            s.name == "multiply"
                && s.signature
                    .as_ref()
                    .unwrap()
                    .contains("calculator:multiply")
        });
        assert!(multiply.is_some());
        assert_eq!(multiply.unwrap().kind, SymbolKind::Method);

        let divide = symbols.iter().find(|s| {
            s.name == "divide" && s.signature.as_ref().unwrap().contains("calculator:divide")
        });
        assert!(divide.is_some());

        // Nested table
        let database = symbols.iter().find(|s| s.name == "database");
        assert!(database.is_some());
        assert_eq!(database.unwrap().kind, SymbolKind::Variable);

        // Nested table fields
        let users = symbols
            .iter()
            .find(|s| s.name == "users" && s.parent_id == Some(database.unwrap().id.clone()));
        assert!(users.is_some());
        assert_eq!(users.unwrap().kind, SymbolKind::Field);

        let settings = symbols
            .iter()
            .find(|s| s.name == "settings" && s.parent_id == Some(database.unwrap().id.clone()));
        assert!(settings.is_some());

        // Constructor function
        let person = symbols.iter().find(|s| s.name == "Person");
        assert!(person.is_some());
        assert_eq!(person.unwrap().kind, SymbolKind::Function);
        assert!(person
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("function Person(name, age)"));

        // Class-like pattern
        let animal = symbols.iter().find(|s| s.name == "Animal");
        assert!(animal.is_some());
        assert_eq!(animal.unwrap().kind, SymbolKind::Class);

        // Class methods
        let animal_new = symbols
            .iter()
            .find(|s| s.name == "new" && s.parent_id == Some(animal.unwrap().id.clone()));
        assert!(animal_new.is_some());
        assert_eq!(animal_new.unwrap().kind, SymbolKind::Method);

        let speak = symbols
            .iter()
            .find(|s| s.name == "speak" && s.parent_id == Some(animal.unwrap().id.clone()));
        assert!(speak.is_some());

        // Inheritance
        let dog = symbols.iter().find(|s| s.name == "Dog");
        assert!(dog.is_some());
        assert_eq!(dog.unwrap().kind, SymbolKind::Class);

        let dog_speak = symbols
            .iter()
            .find(|s| s.name == "speak" && s.parent_id == Some(dog.unwrap().id.clone()));
        assert!(dog_speak.is_some());
        assert_eq!(dog_speak.unwrap().kind, SymbolKind::Method);
    }
}
