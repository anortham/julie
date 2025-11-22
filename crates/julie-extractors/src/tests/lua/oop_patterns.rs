use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_advanced_oop_and_design_patterns() {
        let code = r#"
-- Singleton pattern
local ConfigManager = {}
ConfigManager.__index = ConfigManager

local instance = nil

function ConfigManager:getInstance()
    if not instance then
        instance = setmetatable({
            settings = {},
            loaded = false
        }, ConfigManager)
    end
    return instance
end

function ConfigManager:loadConfig(filename)
    if not self.loaded then
        -- Load config logic here
        self.settings = {theme = "dark", language = "en"}
        self.loaded = true
    end
    return self.settings
end

function ConfigManager:getSetting(key)
    return self.settings[key]
end

-- Factory pattern
local ShapeFactory = {}
ShapeFactory.__index = ShapeFactory

function ShapeFactory:new()
    local instance = setmetatable({}, ShapeFactory)
    instance.shapes = {}
    return instance
end

function ShapeFactory:createShape(shapeType, ...)
    local args = {...}

    if shapeType == "circle" then
        return self:createCircle(unpack(args))
    elseif shapeType == "rectangle" then
        return self:createRectangle(unpack(args))
    elseif shapeType == "triangle" then
        return self:createTriangle(unpack(args))
    end

    return nil
end

function ShapeFactory:createCircle(radius)
    return {
        type = "circle",
        radius = radius,
        area = function(self) return math.pi * self.radius * self.radius end,
        perimeter = function(self) return 2 * math.pi * self.radius end
    }
end

function ShapeFactory:createRectangle(width, height)
    return {
        type = "rectangle",
        width = width,
        height = height,
        area = function(self) return self.width * self.height end,
        perimeter = function(self) return 2 * (self.width + self.height) end
    }
end

function ShapeFactory:createTriangle(base, height)
    return {
        type = "triangle",
        base = base,
        height = height,
        area = function(self) return 0.5 * self.base * self.height end,
        perimeter = function(self) return 3 * self.base end  -- equilateral assumption
    }
end

-- Observer pattern
local EventEmitter = {}
EventEmitter.__index = EventEmitter

function EventEmitter:new()
    local instance = setmetatable({}, EventEmitter)
    instance.listeners = {}
    return instance
end

function EventEmitter:on(event, callback)
    if not self.listeners[event] then
        self.listeners[event] = {}
    end
    table.insert(self.listeners[event], callback)
end

function EventEmitter:off(event, callback)
    if self.listeners[event] then
        for i, cb in ipairs(self.listeners[event]) do
            if cb == callback then
                table.remove(self.listeners[event], i)
                break
            end
        end
    end
end

function EventEmitter:emit(event, ...)
    if self.listeners[event] then
        for _, callback in ipairs(self.listeners[event]) do
            callback(...)
        end
    end
end

-- Strategy pattern
local SortStrategy = {}
SortStrategy.__index = SortStrategy

function SortStrategy:new()
    return setmetatable({}, SortStrategy)
end

function SortStrategy:sort(data)
    -- Default implementation
    table.sort(data)
    return data
end

local QuickSort = setmetatable({}, SortStrategy)
QuickSort.__index = QuickSort

function QuickSort:new()
    return setmetatable({}, QuickSort)
end

function QuickSort:sort(data)
    -- Quick sort implementation
    if #data <= 1 then
        return data
    end

    local pivot = data[1]
    local left, right = {}, {}

    for i = 2, #data do
        if data[i] < pivot then
            table.insert(left, data[i])
        else
            table.insert(right, data[i])
        end
    end

    local result = {}
    local sortedLeft = self:sort(left)
    local sortedRight = self:sort(right)

    for _, v in ipairs(sortedLeft) do
        table.insert(result, v)
    end
    table.insert(result, pivot)
    for _, v in ipairs(sortedRight) do
        table.insert(result, v)
    end

    return result
end

local BubbleSort = setmetatable({}, SortStrategy)
BubbleSort.__index = BubbleSort

function BubbleSort:new()
    return setmetatable({}, BubbleSort)
end

function BubbleSort:sort(data)
    -- Bubble sort implementation
    for i = 1, #data - 1 do
        for j = 1, #data - i do
            if data[j] > data[j + 1] then
                data[j], data[j + 1] = data[j + 1], data[j]
            end
        end
    end
    return data
end

-- Decorator pattern
local function Logger(targetFunction)
    return function(...)
        print("Calling function with args:", ...)
        local result = {targetFunction(...)}
        print("Function returned:", unpack(result))
        return unpack(result)
    end
end

local function Timer(targetFunction)
    return function(...)
        local start = os.clock()
        local result = {targetFunction(...)}
        local elapsed = os.clock() - start
        print("Function took " .. elapsed .. " seconds")
        return unpack(result)
    end
end

-- Mixin pattern
local Movable = {
    move = function(self, dx, dy)
        self.x = self.x + dx
        self.y = self.y + dy
    end,

    getPosition = function(self)
        return self.x, self.y
    end
}

local Drawable = {
    draw = function(self)
        print("Drawing at", self.x, self.y)
    end
}

local Collidable = {
    checkCollision = function(self, other)
        return math.abs(self.x - other.x) < 10 and math.abs(self.y - other.y) < 10
    end
}

local function applyMixin(target, mixin)
    for key, value in pairs(mixin) do
        target[key] = value
    end
end

local GameObject = {}
GameObject.__index = GameObject

function GameObject:new(x, y)
    local instance = setmetatable({}, GameObject)
    instance.x = x
    instance.y = y
    return instance
end

-- Apply mixins
applyMixin(GameObject, Movable)
applyMixin(GameObject, Drawable)
applyMixin(GameObject, Collidable)
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "oop_patterns.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Test Singleton pattern
        let config_manager = symbols.iter().find(|s| s.name == "ConfigManager");
        assert!(config_manager.is_some());
        assert_eq!(config_manager.unwrap().kind, SymbolKind::Class);

        let get_instance = symbols.iter().find(|s| {
            s.name == "getInstance" && s.parent_id == Some(config_manager.unwrap().id.clone())
        });
        assert!(get_instance.is_some());
        assert_eq!(get_instance.unwrap().kind, SymbolKind::Method);

        // Test Factory pattern
        let shape_factory = symbols.iter().find(|s| s.name == "ShapeFactory");
        assert!(shape_factory.is_some());
        assert_eq!(shape_factory.unwrap().kind, SymbolKind::Class);

        let create_shape = symbols.iter().find(|s| {
            s.name == "createShape" && s.parent_id == Some(shape_factory.unwrap().id.clone())
        });
        assert!(create_shape.is_some());

        // Test Observer pattern
        let event_emitter = symbols.iter().find(|s| s.name == "EventEmitter");
        assert!(event_emitter.is_some());
        assert_eq!(event_emitter.unwrap().kind, SymbolKind::Class);

        let emit = symbols
            .iter()
            .find(|s| s.name == "emit" && s.parent_id == Some(event_emitter.unwrap().id.clone()));
        assert!(emit.is_some());

        // Test Strategy pattern
        let sort_strategy = symbols.iter().find(|s| s.name == "SortStrategy");
        assert!(sort_strategy.is_some());

        let quick_sort = symbols.iter().find(|s| s.name == "QuickSort");
        assert!(quick_sort.is_some());
        assert_eq!(quick_sort.unwrap().kind, SymbolKind::Class);

        let bubble_sort = symbols.iter().find(|s| s.name == "BubbleSort");
        assert!(bubble_sort.is_some());

        // Test decorator functions
        let logger = symbols.iter().find(|s| s.name == "Logger");
        assert!(logger.is_some());
        assert_eq!(logger.unwrap().kind, SymbolKind::Function);

        let timer = symbols.iter().find(|s| s.name == "Timer");
        assert!(timer.is_some());

        // Test mixin pattern
        let movable = symbols.iter().find(|s| s.name == "Movable");
        assert!(movable.is_some());

        let drawable = symbols.iter().find(|s| s.name == "Drawable");
        assert!(drawable.is_some());

        let collidable = symbols.iter().find(|s| s.name == "Collidable");
        assert!(collidable.is_some());

        let game_object = symbols.iter().find(|s| s.name == "GameObject");
        assert!(game_object.is_some());
        assert_eq!(game_object.unwrap().kind, SymbolKind::Class);
    }
}
