use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_metatables_and_metamethods() {
        let code = r#"
-- Basic metatable example
local Vector = {}
Vector.__index = Vector

function Vector:new(x, y)
  local instance = {x = x or 0, y = y or 0}
  setmetatable(instance, Vector)
  return instance
end

function Vector:__add(other)
  return Vector:new(self.x + other.x, self.y + other.y)
end

function Vector:__sub(other)
  return Vector:new(self.x - other.x, self.y - other.y)
end

function Vector:__mul(scalar)
  if type(scalar) == "number" then
    return Vector:new(self.x * scalar, self.y * scalar)
  else
    error("Can only multiply vector by number")
  end
end

function Vector:__div(scalar)
  if type(scalar) == "number" and scalar ~= 0 then
    return Vector:new(self.x / scalar, self.y / scalar)
  else
    error("Can only divide vector by non-zero number")
  end
end

function Vector:__eq(other)
  return self.x == other.x and self.y == other.y
end

function Vector:__lt(other)
  return self:magnitude() < other:magnitude()
end

function Vector:__le(other)
  return self:magnitude() <= other:magnitude()
end

function Vector:__tostring()
  return "Vector(" .. self.x .. ", " .. self.y .. ")"
end

function Vector:__len()
  return math.sqrt(self.x * self.x + self.y * self.y)
end

function Vector:__index(key)
  if key == "magnitude" then
    return function(self)
      return math.sqrt(self.x * self.x + self.y * self.y)
    end
  elseif key == "normalized" then
    return function(self)
      local mag = self:magnitude()
      if mag > 0 then
        return Vector:new(self.x / mag, self.y / mag)
      else
        return Vector:new(0, 0)
      end
    end
  else
    return Vector[key]
  end
end

function Vector:__newindex(key, value)
  if key == "x" or key == "y" then
    if type(value) == "number" then
      rawset(self, key, value)
    else
      error("Vector coordinates must be numbers")
    end
  else
    error("Cannot set property '" .. key .. "' on Vector")
  end
end

function Vector:__call(x, y)
  self.x = x or self.x
  self.y = y or self.y
  return self
end

local Complex = {}
Complex.__index = Complex

function Complex:new(real, imag)
  local instance = {
    real = real or 0,
    imag = imag or 0
  }
  setmetatable(instance, Complex)
  return instance
end

function Complex:__add(other)
  if type(other) == "number" then
    return Complex:new(self.real + other, self.imag)
  else
    return Complex:new(self.real + other.real, self.imag + other.imag)
  end
end

function Complex:__sub(other)
  if type(other) == "number" then
    return Complex:new(self.real - other, self.imag)
  else
    return Complex:new(self.real - other.real, self.imag - other.imag)
  end
end

function Complex:__mul(other)
  if type(other) == "number" then
    return Complex:new(self.real * other, self.imag * other)
  else
    local real = self.real * other.real - self.imag * other.imag
    local imag = self.real * other.imag + self.imag * other.real
    return Complex:new(real, imag)
  end
end

function Complex:__div(other)
  if type(other) == "number" then
    return Complex:new(self.real / other, self.imag / other)
  else
    local denom = other.real * other.real + other.imag * other.imag
    local real = (self.real * other.real + self.imag * other.imag) / denom
    local imag = (self.imag * other.real - self.real * other.imag) / denom
    return Complex:new(real, imag)
  end
end

function Complex:__pow(exponent)
  if type(exponent) == "number" then
    local magnitude = (self.real^2 + self.imag^2)^(exponent / 2)
    local angle = math.atan(self.imag, self.real) * exponent
    return Complex:new(magnitude * math.cos(angle), magnitude * math.sin(angle))
  else
    error("Exponent must be a number")
  end
end

function Complex:__unm()
  return Complex:new(-self.real, -self.imag)
end

function Complex:__tostring()
  return string.format("%0.2f + %0.2fi", self.real, self.imag)
end

local Matrix = {}
Matrix.__index = Matrix

function Matrix:new(rows, cols)
  local instance = {
    rows = rows or 0,
    cols = cols or 0,
    data = {}
  }

  for i = 1, rows do
    instance.data[i] = {}
    for j = 1, cols do
      instance.data[i][j] = 0
    end
  end

  setmetatable(instance, Matrix)
  return instance
end

function Matrix:__add(other)
  local result = Matrix:new(self.rows, self.cols)

  for i = 1, self.rows do
    for j = 1, self.cols do
      result.data[i][j] = self.data[i][j] + other.data[i][j]
    end
  end

  return result
end

function Matrix:__mul(other)
  if type(other) == "number" then
    local result = Matrix:new(self.rows, self.cols)
    for i = 1, self.rows do
      for j = 1, self.cols do
        result.data[i][j] = self.data[i][j] * other
      end
    end
    return result
  elseif type(other) == "table" and getmetatable(other) == Matrix then
    if self.cols ~= other.rows then
      error("Matrix dimensions incompatible for multiplication")
    end

    local result = Matrix:new(self.rows, other.cols)
    for i = 1, self.rows do
      for j = 1, other.cols do
        local sum = 0
        for k = 1, self.cols do
          sum = sum + self.data[i][k] * other.data[k][j]
        end
        result.data[i][j] = sum
      end
    end
    return result
  else
    error("Unsupported multiplication operand")
  end
end

local Cache = {}
Cache.__index = Cache

function Cache:new(maxSize)
  local instance = {
    data = {},
    maxSize = maxSize or 100
  }

  local meta = {
    __index = function(_, key)
      return rawget(instance.data, key)
    end,

    __newindex = function(_, key, value)
      if type(key) ~= "string" then
        error("Cache keys must be strings")
      end

      if value == nil then
        rawset(instance.data, key, nil)
        return
      end

      if type(value) ~= "number" and type(value) ~= "string" then
        error("Cache values must be numbers or strings")
      end

      if instance:size() >= instance.maxSize then
        for oldestKey in pairs(instance.data) do
          rawset(instance.data, oldestKey, nil)
          break
        end
      end

      rawset(instance.data, key, value)
    end,

    __len = function()
      local count = 0
      for _ in pairs(instance.data) do
        count = count + 1
      end
      return count
    end
  }

  setmetatable(instance.data, meta)
  setmetatable(instance, Cache)

  return instance
end

function Cache:set(key, value)
  self.data[key] = value
end

function Cache:get(key)
  return self.data[key]
end

function Cache:size()
  local count = 0
  for _ in pairs(self.data) do
    count = count + 1
  end
  return count
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "metatables.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let vector = symbols.iter().find(|s| s.name == "Vector");
        assert!(vector.is_some());
        assert_eq!(vector.unwrap().kind, SymbolKind::Class);

        let vector_new = symbols
            .iter()
            .find(|s| s.name == "new" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_new.is_some());

        let vector_add = symbols
            .iter()
            .find(|s| s.name == "__add" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_add.is_some());

        let vector_sub = symbols
            .iter()
            .find(|s| s.name == "__sub" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_sub.is_some());

        let vector_mul = symbols
            .iter()
            .find(|s| s.name == "__mul" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_mul.is_some());

        let vector_div = symbols
            .iter()
            .find(|s| s.name == "__div" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_div.is_some());

        let vector_eq = symbols
            .iter()
            .find(|s| s.name == "__eq" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_eq.is_some());

        let vector_tostring = symbols
            .iter()
            .find(|s| s.name == "__tostring" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_tostring.is_some());

        let vector_len = symbols
            .iter()
            .find(|s| s.name == "__len" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_len.is_some());

        let vector_index = symbols
            .iter()
            .find(|s| s.name == "__index" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_index.is_some());

        let vector_newindex = symbols
            .iter()
            .find(|s| s.name == "__newindex" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_newindex.is_some());

        let vector_call = symbols
            .iter()
            .find(|s| s.name == "__call" && s.parent_id == Some(vector.unwrap().id.clone()));
        assert!(vector_call.is_some());

        let complex = symbols.iter().find(|s| s.name == "Complex");
        assert!(complex.is_some());
        assert_eq!(complex.unwrap().kind, SymbolKind::Class);

        let complex_add = symbols
            .iter()
            .find(|s| s.name == "__add" && s.parent_id == Some(complex.unwrap().id.clone()));
        assert!(complex_add.is_some());

        let complex_pow = symbols
            .iter()
            .find(|s| s.name == "__pow" && s.parent_id == Some(complex.unwrap().id.clone()));
        assert!(complex_pow.is_some());

        let complex_unm = symbols
            .iter()
            .find(|s| s.name == "__unm" && s.parent_id == Some(complex.unwrap().id.clone()));
        assert!(complex_unm.is_some());

        let matrix = symbols.iter().find(|s| s.name == "Matrix");
        assert!(matrix.is_some());
        assert_eq!(matrix.unwrap().kind, SymbolKind::Class);

        let matrix_add = symbols
            .iter()
            .find(|s| s.name == "__add" && s.parent_id == Some(matrix.unwrap().id.clone()));
        assert!(matrix_add.is_some());

        let matrix_mul = symbols
            .iter()
            .find(|s| s.name == "__mul" && s.parent_id == Some(matrix.unwrap().id.clone()));
        assert!(matrix_mul.is_some());

        let cache = symbols.iter().find(|s| s.name == "Cache");
        assert!(cache.is_some());
        assert_eq!(cache.unwrap().kind, SymbolKind::Class);

        let cache_set = symbols
            .iter()
            .find(|s| s.name == "set" && s.parent_id == Some(cache.unwrap().id.clone()));
        assert!(cache_set.is_some());

        let cache_get = symbols
            .iter()
            .find(|s| s.name == "get" && s.parent_id == Some(cache.unwrap().id.clone()));
        assert!(cache_get.is_some());
    }
}
