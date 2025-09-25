-- Advanced Lua Web Server Framework
-- Demonstrates complex Lua programming concepts including:
-- - Coroutines and asynchronous programming
-- - Metatable-based OOP
-- - Complex table manipulations
-- - Module system and closures

local json = require("json")
local socket = require("socket")
local url = require("socket.url")

-- HTTP Server Framework
local HttpServer = {}
HttpServer.__index = HttpServer

-- Constructor for HttpServer
function HttpServer:new(host, port)
    local instance = {
        host = host or "localhost",
        port = port or 8080,
        routes = {},
        middleware = {},
        running = false,
        connection_pool = {},
        request_count = 0
    }
    setmetatable(instance, HttpServer)
    return instance
end

-- Route registration with method and pattern matching
function HttpServer:route(method, pattern, handler)
    if not self.routes[method] then
        self.routes[method] = {}
    end

    table.insert(self.routes[method], {
        pattern = pattern,
        handler = handler,
        middleware = {}
    })
end

-- Convenience methods for common HTTP methods
function HttpServer:get(pattern, handler)
    self:route("GET", pattern, handler)
end

function HttpServer:post(pattern, handler)
    self:route("POST", pattern, handler)
end

function HttpServer:put(pattern, handler)
    self:route("PUT", pattern, handler)
end

function HttpServer:delete(pattern, handler)
    self:route("DELETE", pattern, handler)
end

-- Middleware registration
function HttpServer:use(middleware_func)
    table.insert(self.middleware, middleware_func)
end

-- Request parsing utilities
local function parse_headers(header_text)
    local headers = {}
    for line in header_text:gmatch("[^\r\n]+") do
        local key, value = line:match("^([^:]+):%s*(.*)$")
        if key and value then
            headers[key:lower()] = value
        end
    end
    return headers
end

local function parse_query_params(query_string)
    local params = {}
    if query_string then
        for pair in query_string:gmatch("[^&]+") do
            local key, value = pair:match("^([^=]+)=(.*)$")
            if key and value then
                params[url.unescape(key)] = url.unescape(value)
            end
        end
    end
    return params
end

local function parse_json_body(body, content_type)
    if content_type and content_type:find("application/json") then
        local success, data = pcall(json.decode, body)
        return success and data or {}
    end
    return {}
end

-- Request object constructor
local function create_request(method, path, headers, body, client_ip)
    local parsed_url = url.parse(path)

    local request = {
        method = method,
        path = parsed_url.path or "/",
        query = parse_query_params(parsed_url.query),
        headers = headers,
        body = body,
        json = parse_json_body(body, headers["content-type"]),
        ip = client_ip,
        params = {},
        timestamp = os.time()
    }

    return request
end

-- Response object constructor
local function create_response()
    local response = {
        status = 200,
        headers = {
            ["Content-Type"] = "text/html; charset=UTF-8",
            ["Server"] = "Lua-HttpServer/1.0"
        },
        body = ""
    }

    -- Response helper methods
    function response:json(data)
        self.headers["Content-Type"] = "application/json"
        self.body = json.encode(data)
        return self
    end

    function response:html(content)
        self.headers["Content-Type"] = "text/html; charset=UTF-8"
        self.body = content
        return self
    end

    function response:text(content)
        self.headers["Content-Type"] = "text/plain; charset=UTF-8"
        self.body = content
        return self
    end

    function response:status_code(code)
        self.status = code
        return self
    end

    function response:header(key, value)
        self.headers[key] = value
        return self
    end

    return response
end

-- Pattern matching for routes
local function match_route(pattern, path)
    local params = {}

    -- Convert route pattern to Lua pattern
    local lua_pattern = pattern:gsub(":[^/]+", "([^/]+)")

    -- Extract parameter names
    local param_names = {}
    for param in pattern:gmatch(":([^/]+)") do
        table.insert(param_names, param)
    end

    -- Match against path
    local matches = {path:match("^" .. lua_pattern .. "$")}

    if #matches > 0 then
        for i, name in ipairs(param_names) do
            params[name] = matches[i]
        end
        return params
    end

    return nil
end

-- Route handler
function HttpServer:handle_request(method, path, headers, body, client_ip)
    local request = create_request(method, path, headers, body, client_ip)
    local response = create_response()

    -- Apply middleware
    for _, middleware in ipairs(self.middleware) do
        local middleware_result = middleware(request, response)
        if middleware_result == false then
            -- Middleware blocked the request
            return response
        end
    end

    -- Find matching route
    if self.routes[method] then
        for _, route in ipairs(self.routes[method]) do
            local params = match_route(route.pattern, request.path)
            if params then
                request.params = params

                -- Execute route handler
                local success, result = pcall(route.handler, request, response)
                if not success then
                    response:status_code(500)
                    response:text("Internal Server Error: " .. tostring(result))
                end

                return response
            end
        end
    end

    -- No route found
    response:status_code(404)
    response:text("Not Found")
    return response
end

-- HTTP response formatting
local function format_response(response)
    local status_messages = {
        [200] = "OK",
        [201] = "Created",
        [400] = "Bad Request",
        [401] = "Unauthorized",
        [403] = "Forbidden",
        [404] = "Not Found",
        [500] = "Internal Server Error"
    }

    local status_text = status_messages[response.status] or "Unknown"
    local http_response = string.format("HTTP/1.1 %d %s\r\n", response.status, status_text)

    -- Add headers
    response.headers["Content-Length"] = tostring(#response.body)
    for key, value in pairs(response.headers) do
        http_response = http_response .. string.format("%s: %s\r\n", key, value)
    end

    http_response = http_response .. "\r\n" .. response.body
    return http_response
end

-- Connection handler using coroutines
function HttpServer:handle_connection(client)
    local request_data = ""
    local headers_end = false
    local content_length = 0
    local body = ""

    -- Read request
    repeat
        local line, err = client:receive()
        if err then break end

        request_data = request_data .. line .. "\r\n"

        if line == "" then
            headers_end = true
            -- Parse headers to get content length
            local content_length_match = request_data:match("\r\nContent%-Length:%s*(%d+)")
            content_length = tonumber(content_length_match) or 0
        end
    until headers_end

    -- Read body if present
    if content_length > 0 then
        body = client:receive(content_length)
    end

    -- Parse request line
    local method, path = request_data:match("^(%S+)%s+(%S+)")
    if not method or not path then
        client:close()
        return
    end

    -- Parse headers
    local headers = parse_headers(request_data)

    -- Get client IP
    local client_ip = client:getpeername()

    -- Handle request
    local response = self:handle_request(method, path, headers, body, client_ip)

    -- Send response
    client:send(format_response(response))
    client:close()

    -- Update statistics
    self.request_count = self.request_count + 1
end

-- Main server loop
function HttpServer:listen()
    local server = socket.tcp()
    server:setsockname(self.host, self.port)
    server:listen(32)
    server:settimeout(0.1) -- Non-blocking

    self.running = true

    print(string.format("Server running on http://%s:%d", self.host, self.port))

    while self.running do
        local client = server:accept()
        if client then
            -- Handle connection in coroutine for concurrency
            coroutine.wrap(function()
                self:handle_connection(client)
            end)()
        end
    end

    server:close()
end

function HttpServer:stop()
    self.running = false
end

-- Logger middleware
local function logger_middleware(request, response)
    local timestamp = os.date("%Y-%m-%d %H:%M:%S")
    print(string.format("[%s] %s %s - %s",
        timestamp, request.method, request.path, request.ip))
end

-- CORS middleware
local function cors_middleware(request, response)
    response:header("Access-Control-Allow-Origin", "*")
    response:header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
    response:header("Access-Control-Allow-Headers", "Content-Type, Authorization")

    if request.method == "OPTIONS" then
        response:status_code(200)
        return false -- Stop processing
    end
end

-- Example usage
local function create_sample_server()
    local server = HttpServer:new("localhost", 8080)

    -- Add middleware
    server:use(logger_middleware)
    server:use(cors_middleware)

    -- Define routes
    server:get("/", function(req, res)
        res:html([[
            <html>
                <head><title>Lua Web Server</title></head>
                <body>
                    <h1>Welcome to Lua HTTP Server</h1>
                    <p>Server is running successfully!</p>
                </body>
            </html>
        ]])
    end)

    server:get("/api/users/:id", function(req, res)
        res:json({
            id = req.params.id,
            name = "Sample User",
            email = "user@example.com"
        })
    end)

    server:post("/api/data", function(req, res)
        res:json({
            received = req.json,
            timestamp = os.time()
        })
    end)

    return server
end

-- Module exports
return {
    HttpServer = HttpServer,
    create_sample_server = create_sample_server
}