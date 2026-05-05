local Worker = {}
Worker.__index = Worker

local function helper(value)
    return value + 1
end

function Worker:new(id)
    return setmetatable({ id = id }, Worker)
end

function Worker:run()
    return helper(self.id)
end

return Worker
