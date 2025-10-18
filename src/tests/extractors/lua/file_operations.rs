use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_io_and_path_operations() {
        let code = r#"
-- Basic file reading
local function readFile(filename)
    local file = io.open(filename, "r")
    if not file then
        return nil, "Could not open file: " .. filename
    end

    local content = file:read("*all")
    file:close()
    return content
end

-- File writing
local function writeFile(filename, content)
    local file = io.open(filename, "w")
    if not file then
        return false, "Could not create file: " .. filename
    end

    file:write(content)
    file:close()
    return true
end

-- Line-by-line reading
local function readLines(filename)
    local lines = {}
    local file = io.open(filename, "r")
    if not file then
        return nil, "Could not open file: " .. filename
    end

    for line in file:lines() do
        table.insert(lines, line)
    end
    file:close()
    return lines
end

-- Binary file operations
local function copyFile(src, dst)
    local srcFile = io.open(src, "rb")
    if not srcFile then
        return false, "Could not open source file"
    end

    local dstFile = io.open(dst, "wb")
    if not dstFile then
        srcFile:close()
        return false, "Could not create destination file"
    end

    local data = srcFile:read("*all")
    dstFile:write(data)

    srcFile:close()
    dstFile:close()
    return true
end

-- Path manipulation
local function getFileExtension(filename)
    return filename:match("%.([^%.]+)$")
end

local function getFilename(path)
    return path:match("([^/]+)$")
end

local function getDirectory(path)
    return path:match("(.+)/")
end

local function joinPaths(...)
    local args = {...}
    local result = ""
    for i, part in ipairs(args) do
        if i > 1 then
            result = result .. "/"
        end
        result = result .. part:gsub("^/+", ""):gsub("/+$", "")
    end
    return result
end

-- Directory operations
local function listDirectory(path)
    local files = {}
    local p = io.popen('ls -la "' .. path .. '"')
    if p then
        for line in p:lines() do
            table.insert(files, line)
        end
        p:close()
    end
    return files
end

-- Configuration file handling
local function loadConfig(filename)
    local config = {}
    local file = io.open(filename, "r")
    if not file then
        return nil, "Config file not found"
    end

    for line in file:lines() do
        local key, value = line:match("^([^=]+)=(.*)$")
        if key and value then
            config[key:match("^%s*(.-)%s*$")] = value:match("^%s*(.-)%s*$")
        end
    end
    file:close()
    return config
end

local function saveConfig(filename, config)
    local file = io.open(filename, "w")
    if not file then
        return false, "Could not write config file"
    end

    for key, value in pairs(config) do
        file:write(key .. "=" .. value .. "\n")
    end
    file:close()
    return true
end

-- File locking simulation
local function withFileLock(filename, operation)
    local lockFile = filename .. ".lock"

    -- Simple lock mechanism
    if io.open(lockFile, "r") then
        return false, "File is locked"
    end

    -- Create lock
    local lock = io.open(lockFile, "w")
    lock:write(os.time())
    lock:close()

    local success, result = pcall(operation)

    -- Remove lock
    os.remove(lockFile)

    if success then
        return true, result
    else
        return false, result
    end
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "file_operations.lua".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Test file I/O functions
        let read_file = symbols.iter().find(|s| s.name == "readFile");
        assert!(read_file.is_some());
        assert_eq!(read_file.unwrap().kind, SymbolKind::Function);

        let write_file = symbols.iter().find(|s| s.name == "writeFile");
        assert!(write_file.is_some());

        let read_lines = symbols.iter().find(|s| s.name == "readLines");
        assert!(read_lines.is_some());

        let copy_file = symbols.iter().find(|s| s.name == "copyFile");
        assert!(copy_file.is_some());

        // Test path manipulation functions
        let get_file_extension = symbols.iter().find(|s| s.name == "getFileExtension");
        assert!(get_file_extension.is_some());

        let get_filename = symbols.iter().find(|s| s.name == "getFilename");
        assert!(get_filename.is_some());

        let get_directory = symbols.iter().find(|s| s.name == "getDirectory");
        assert!(get_directory.is_some());

        let join_paths = symbols.iter().find(|s| s.name == "joinPaths");
        assert!(join_paths.is_some());

        // Test directory and config functions
        let list_directory = symbols.iter().find(|s| s.name == "listDirectory");
        assert!(list_directory.is_some());

        let load_config = symbols.iter().find(|s| s.name == "loadConfig");
        assert!(load_config.is_some());

        let save_config = symbols.iter().find(|s| s.name == "saveConfig");
        assert!(save_config.is_some());

        let with_file_lock = symbols.iter().find(|s| s.name == "withFileLock");
        assert!(with_file_lock.is_some());
    }
}
