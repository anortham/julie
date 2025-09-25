#[cfg(test)]
mod zig_extractor_tests {
    use crate::extractors::base::{SymbolKind, Visibility};
    use crate::extractors::zig::ZigExtractor;
    use crate::tests::test_utils::init_parser;

    fn extract_symbols(code: &str) -> Vec<crate::extractors::base::Symbol> {
        let tree = init_parser(code, "zig");
        let mut extractor = ZigExtractor::new("zig".to_string(), "test.zig".to_string(), code.to_string());
        extractor.extract_symbols(&tree)
    }

    fn extract_relationships(code: &str, symbols: &[crate::extractors::base::Symbol]) -> Vec<crate::extractors::base::Relationship> {
        let tree = init_parser(code, "zig");
        let mut extractor = ZigExtractor::new("zig".to_string(), "test.zig".to_string(), code.to_string());
        extractor.extract_relationships(&tree, symbols)
    }

    #[test]
    fn test_extract_structs_unions_and_enums() {
        let zig_code = r#"
const std = @import("std");
const testing = std.testing;

// Basic struct
const Point = struct {
    x: f32,
    y: f32,

    const Self = @This();

    pub fn init(x: f32, y: f32) Self {
        return Self{ .x = x, .y = y };
    }

    pub fn distance(self: Self, other: Self) f32 {
        const dx = self.x - other.x;
        const dy = self.y - other.y;
        return @sqrt(dx * dx + dy * dy);
    }

    pub fn scale(self: *Self, factor: f32) void {
        self.x *= factor;
        self.y *= factor;
    }

    const ORIGIN = Point{ .x = 0.0, .y = 0.0 };
};

// Packed struct for memory layout control
const PackedData = packed struct {
    flags: u8,
    id: u16,
    value: u32,

    pub fn isValid(self: PackedData) bool {
        return self.flags & 0x80 != 0;
    }
};

// Generic struct
fn Vector(comptime T: type) type {
    return struct {
        items: []T,
        capacity: usize,
        allocator: std.mem.Allocator,

        const Self = @This();

        pub fn init(allocator: std.mem.Allocator) Self {
            return Self{
                .items = &[_]T{},
                .capacity = 0,
                .allocator = allocator,
            };
        }

        pub fn deinit(self: *Self) void {
            if (self.capacity > 0) {
                self.allocator.free(self.items.ptr[0..self.capacity]);
            }
        }

        pub fn append(self: *Self, item: T) !void {
            if (self.items.len == self.capacity) {
                try self.grow();
            }
            self.items.len += 1;
            self.items[self.items.len - 1] = item;
        }

        fn grow(self: *Self) !void {
            const new_capacity = if (self.capacity == 0) 8 else self.capacity * 2;
            const new_memory = try self.allocator.alloc(T, new_capacity);

            if (self.capacity > 0) {
                std.mem.copy(T, new_memory, self.items);
                self.allocator.free(self.items.ptr[0..self.capacity]);
            }

            self.items.ptr = new_memory.ptr;
            self.capacity = new_capacity;
        }
    };
}

// Union types
const Value = union(enum) {
    none: void,
    integer: i64,
    float: f64,
    string: []const u8,
    boolean: bool,

    pub fn typeString(self: Value) []const u8 {
        return switch (self) {
            .none => "none",
            .integer => "integer",
            .float => "float",
            .string => "string",
            .boolean => "boolean",
        };
    }

    pub fn asInteger(self: Value) ?i64 {
        return switch (self) {
            .integer => |val| val,
            else => null,
        };
    }
};

// Enum with explicit values
const Color = enum(u8) {
    red = 0xFF0000,
    green = 0x00FF00,
    blue = 0x0000FF,

    pub fn toRgb(self: Color) u32 {
        return @enumToInt(self);
    }
};

// Error set
const FileError = error{
    AccessDenied,
    OutOfMemory,
    FileNotFound,
    InvalidPath,
};

const AllocationError = error{
    OutOfMemory,
};

const IoError = error{
    NetworkDown,
    ConnectionRefused,
} || FileError;"#;

        let symbols = extract_symbols(zig_code);

        // Should extract structs
        let point_struct = symbols.iter().find(|s| s.name == "Point" && s.kind == SymbolKind::Class);
        assert!(point_struct.is_some());
        assert!(point_struct.unwrap().signature.as_ref().unwrap().contains("const Point = struct"));

        let packed_struct = symbols.iter().find(|s| s.name == "PackedData");
        assert!(packed_struct.is_some());
        assert!(packed_struct.unwrap().signature.as_ref().unwrap().contains("packed struct"));

        // Should extract struct fields
        let x_field = symbols.iter().find(|s| s.name == "x" && s.kind == SymbolKind::Field);
        assert!(x_field.is_some());
        assert!(x_field.unwrap().signature.as_ref().unwrap().contains("f32"));

        let flags_field = symbols.iter().find(|s| s.name == "flags");
        assert!(flags_field.is_some());
        assert!(flags_field.unwrap().signature.as_ref().unwrap().contains("u8"));

        // Should extract struct methods
        let init_method = symbols.iter().find(|s| s.name == "init" && s.kind == SymbolKind::Method);
        assert!(init_method.is_some());
        assert!(init_method.unwrap().signature.as_ref().unwrap().contains("pub fn init"));

        let distance_method = symbols.iter().find(|s| s.name == "distance");
        assert!(distance_method.is_some());
        assert!(distance_method.unwrap().signature.as_ref().unwrap().contains("f32"));

        let scale_method = symbols.iter().find(|s| s.name == "scale");
        assert!(scale_method.is_some());
        assert!(scale_method.unwrap().signature.as_ref().unwrap().contains("*Self"));

        // Should extract constants
        let origin_constant = symbols.iter().find(|s| s.name == "ORIGIN");
        assert!(origin_constant.is_some());
        assert_eq!(origin_constant.unwrap().kind, SymbolKind::Constant);

        // Should extract generic functions
        let vector_function = symbols.iter().find(|s| s.name == "Vector" && s.kind == SymbolKind::Function);
        assert!(vector_function.is_some());
        assert!(vector_function.unwrap().signature.as_ref().unwrap().contains("comptime T: type"));

        // Should extract unions
        let value_union = symbols.iter().find(|s| s.name == "Value");
        assert!(value_union.is_some());
        assert!(value_union.unwrap().signature.as_ref().unwrap().contains("union(enum)"));

        // Should extract union methods
        let type_string_method = symbols.iter().find(|s| s.name == "typeString");
        assert!(type_string_method.is_some());

        let as_integer_method = symbols.iter().find(|s| s.name == "asInteger");
        assert!(as_integer_method.is_some());
        assert!(as_integer_method.unwrap().signature.as_ref().unwrap().contains("?i64"));

        // Should extract enums
        let color_enum = symbols.iter().find(|s| s.name == "Color" && s.kind == SymbolKind::Enum);
        assert!(color_enum.is_some());
        assert!(color_enum.unwrap().signature.as_ref().unwrap().contains("enum(u8)"));

        // Should extract enum members
        let red_member = symbols.iter().find(|s| s.name == "red");
        assert!(red_member.is_some());

        let to_rgb_method = symbols.iter().find(|s| s.name == "toRgb");
        assert!(to_rgb_method.is_some());

        // Should extract error sets
        let file_error = symbols.iter().find(|s| s.name == "FileError");
        assert!(file_error.is_some());
        assert!(file_error.unwrap().signature.as_ref().unwrap().contains("error{"));

        let io_error = symbols.iter().find(|s| s.name == "IoError");
        assert!(io_error.is_some());
        assert!(io_error.unwrap().signature.as_ref().unwrap().contains("|| FileError"));
    }

    #[test]
    fn test_extract_functions_with_error_handling_and_optionals() {
        let zig_code = r#"
const std = @import("std");
const Allocator = std.mem.Allocator;

// Function with error union return type
fn parseInteger(input: []const u8) !i32 {
    if (input.len == 0) return error.EmptyInput;

    var result: i32 = 0;
    var negative = false;
    var start_idx: usize = 0;

    if (input[0] == '-') {
        negative = true;
        start_idx = 1;
        if (input.len == 1) return error.InvalidFormat;
    }

    for (input[start_idx..]) |char| {
        if (char < '0' or char > '9') return error.InvalidCharacter;

        const digit = char - '0';
        const new_result = std.math.mul(i32, result, 10) catch return error.Overflow;
        result = std.math.add(i32, new_result, digit) catch return error.Overflow;
    }

    return if (negative) -result else result;
}

// Function with optional return type
fn findChar(haystack: []const u8, needle: u8) ?usize {
    for (haystack, 0..) |char, index| {
        if (char == needle) return index;
    }
    return null;
}

// Generic function with multiple type parameters
fn swap(comptime T: type, a: *T, b: *T) void {
    const temp = a.*;
    a.* = b.*;
    b.* = temp;
}

// Function with allocator parameter
fn duplicateString(allocator: Allocator, input: []const u8) ![]u8 {
    const result = try allocator.alloc(u8, input.len);
    std.mem.copy(u8, result, input);
    return result;
}

// Async function
fn fetchData(url: []const u8) ![]u8 {
    var client = std.http.Client{ .allocator = std.heap.page_allocator };
    defer client.deinit();

    const response = try client.fetch(.{
        .location = .{ .url = url },
        .method = .GET,
    });

    return response.body orelse error.EmptyResponse;
}

// Function with comptime parameters
fn createArray(comptime T: type, comptime size: usize, value: T) [size]T {
    var array: [size]T = undefined;
    for (&array) |*item| {
        item.* = value;
    }
    return array;
}

// Inline function
inline fn min(a: anytype, b: @TypeOf(a)) @TypeOf(a) {
    return if (a < b) a else b;
}

// Export function (C ABI)
export fn add_numbers(a: c_int, b: c_int) c_int {
    return a + b;
}

// Function with varargs
fn printf(comptime fmt: []const u8, args: anytype) void {
    std.debug.print(fmt, args);
}

// Function pointer type
const BinaryOp = fn (a: i32, b: i32) i32;

fn applyOperation(a: i32, b: i32, op: BinaryOp) i32 {
    return op(a, b);
}

// Closure-like behavior with struct
const Counter = struct {
    value: i32 = 0,

    pub fn increment(self: *Counter) i32 {
        self.value += 1;
        return self.value;
    }

    pub fn reset(self: *Counter) void {
        self.value = 0;
    }
};"#;

        let symbols = extract_symbols(zig_code);

        // Should extract functions with error union returns
        let parse_integer_fn = symbols.iter().find(|s| s.name == "parseInteger" && s.kind == SymbolKind::Function);
        assert!(parse_integer_fn.is_some());
        assert!(parse_integer_fn.unwrap().signature.as_ref().unwrap().contains("!i32"));

        // Should extract functions with optional returns
        let find_char_fn = symbols.iter().find(|s| s.name == "findChar");
        assert!(find_char_fn.is_some());
        assert!(find_char_fn.unwrap().signature.as_ref().unwrap().contains("?usize"));

        // Should extract generic functions
        let swap_fn = symbols.iter().find(|s| s.name == "swap");
        assert!(swap_fn.is_some());
        assert!(swap_fn.unwrap().signature.as_ref().unwrap().contains("comptime T: type"));

        // Should extract functions with allocator parameters
        let duplicate_string_fn = symbols.iter().find(|s| s.name == "duplicateString");
        assert!(duplicate_string_fn.is_some());
        assert!(duplicate_string_fn.unwrap().signature.as_ref().unwrap().contains("Allocator"));

        // Should extract async functions
        let fetch_data_fn = symbols.iter().find(|s| s.name == "fetchData");
        assert!(fetch_data_fn.is_some());

        // Should extract comptime functions
        let create_array_fn = symbols.iter().find(|s| s.name == "createArray");
        assert!(create_array_fn.is_some());
        assert!(create_array_fn.unwrap().signature.as_ref().unwrap().contains("comptime size: usize"));

        // Should extract inline functions
        let min_fn = symbols.iter().find(|s| s.name == "min");
        assert!(min_fn.is_some());
        assert!(min_fn.unwrap().signature.as_ref().unwrap().contains("inline fn"));

        // Should extract export functions
        let add_numbers_fn = symbols.iter().find(|s| s.name == "add_numbers");
        assert!(add_numbers_fn.is_some());
        assert!(add_numbers_fn.unwrap().signature.as_ref().unwrap().contains("export fn"));
        assert!(add_numbers_fn.unwrap().signature.as_ref().unwrap().contains("c_int"));

        // Should extract varargs functions
        let printf_fn = symbols.iter().find(|s| s.name == "printf");
        assert!(printf_fn.is_some());
        assert!(printf_fn.unwrap().signature.as_ref().unwrap().contains("anytype"));

        // Should extract function types
        let binary_op_type = symbols.iter().find(|s| s.name == "BinaryOp");
        assert!(binary_op_type.is_some());
        assert!(binary_op_type.unwrap().signature.as_ref().unwrap().contains("fn ("));

        let apply_op_fn = symbols.iter().find(|s| s.name == "applyOperation");
        assert!(apply_op_fn.is_some());
        assert!(apply_op_fn.unwrap().signature.as_ref().unwrap().contains("BinaryOp"));

        // Should extract counter struct and methods
        let counter_struct = symbols.iter().find(|s| s.name == "Counter");
        assert!(counter_struct.is_some());

        let increment_method = symbols.iter().find(|s| s.name == "increment");
        assert!(increment_method.is_some());

        let reset_method = symbols.iter().find(|s| s.name == "reset");
        assert!(reset_method.is_some());
    }

    #[test]
    fn test_extract_memory_management_and_c_interop() {
        let zig_code = r#"
const std = @import("std");
const c = @cImport({
    @cInclude("stdio.h");
    @cInclude("stdlib.h");
    @cInclude("string.h");
});

// Allocator wrapper
const ArenaAllocator = struct {
    arena: std.heap.ArenaAllocator,

    pub fn init(backing_allocator: std.mem.Allocator) ArenaAllocator {
        return ArenaAllocator{
            .arena = std.heap.ArenaAllocator.init(backing_allocator),
        };
    }

    pub fn allocator(self: *ArenaAllocator) std.mem.Allocator {
        return self.arena.allocator();
    }

    pub fn deinit(self: *ArenaAllocator) void {
        self.arena.deinit();
    }

    pub fn reset(self: *ArenaAllocator, mode: std.heap.ArenaAllocator.ResetMode) void {
        _ = self.arena.reset(mode);
    }
};

// C interop structure
const CString = extern struct {
    data: [*:0]u8,
    length: c_size_t,

    pub fn fromSlice(allocator: std.mem.Allocator, slice: []const u8) !CString {
        const data = try allocator.allocSentinel(u8, slice.len, 0);
        std.mem.copy(u8, data, slice);
        return CString{
            .data = data,
            .length = slice.len,
        };
    }

    pub fn toSlice(self: CString) []const u8 {
        return self.data[0..self.length];
    }

    pub fn deinit(self: CString, allocator: std.mem.Allocator) void {
        allocator.free(self.data[0..self.length + 1]);
    }
};

// C function declarations
extern "c" fn malloc(size: c_size_t) ?*anyopaque;
extern "c" fn free(ptr: *anyopaque) void;
extern "c" fn printf(format: [*:0]const u8, ...) c_int;

// C callback function type
const CallbackFn = fn (data: ?*anyopaque, result: c_int) callconv(.C) void;

// Library with C bindings
const MathLib = struct {
    // Function that calls C math functions
    pub fn fastSqrt(value: f64) f64 {
        return @sqrt(value);
    }

    // Wrapper for C malloc/free
    pub fn cAlloc(size: usize) ?[]u8 {
        const ptr = malloc(size) orelse return null;
        return @ptrCast([*]u8, ptr)[0..size];
    }

    pub fn cFree(memory: []u8) void {
        free(memory.ptr);
    }
};

// Smart pointer pattern
fn UniquePtr(comptime T: type) type {
    return struct {
        ptr: ?*T,
        allocator: std.mem.Allocator,

        const Self = @This();

        pub fn init(allocator: std.mem.Allocator, value: T) !Self {
            const ptr = try allocator.create(T);
            ptr.* = value;
            return Self{
                .ptr = ptr,
                .allocator = allocator,
            };
        }

        pub fn deinit(self: *Self) void {
            if (self.ptr) |ptr| {
                self.allocator.destroy(ptr);
                self.ptr = null;
            }
        }

        pub fn get(self: Self) ?*T {
            return self.ptr;
        }

        pub fn release(self: *Self) ?*T {
            const ptr = self.ptr;
            self.ptr = null;
            return ptr;
        }

        pub fn reset(self: *Self, new_value: ?T) !void {
            self.deinit();
            if (new_value) |value| {
                const ptr = try self.allocator.create(T);
                ptr.* = value;
                self.ptr = ptr;
            }
        }
    };
}

// RAII pattern
const FileHandle = struct {
    file: ?*c.FILE = null,

    const Self = @This();

    pub fn open(path: [*:0]const u8, mode: [*:0]const u8) !Self {
        const file = c.fopen(path, mode) orelse return error.CannotOpenFile;
        return Self{ .file = file };
    }

    pub fn close(self: *Self) void {
        if (self.file) |file| {
            _ = c.fclose(file);
            self.file = null;
        }
    }

    pub fn write(self: Self, data: []const u8) !usize {
        const file = self.file orelse return error.FileNotOpen;
        const written = c.fwrite(data.ptr, 1, data.len, file);
        if (written != data.len) return error.WriteError;
        return written;
    }

    pub fn read(self: Self, buffer: []u8) !usize {
        const file = self.file orelse return error.FileNotOpen;
        return c.fread(buffer.ptr, 1, buffer.len, file);
    }
};"#;

        let symbols = extract_symbols(zig_code);

        // Should extract allocator wrapper
        let arena_allocator = symbols.iter().find(|s| s.name == "ArenaAllocator");
        assert!(arena_allocator.is_some());

        let allocator_method = symbols.iter().find(|s| s.name == "allocator");
        assert!(allocator_method.is_some());

        // Should extract C interop structures
        let c_string_struct = symbols.iter().find(|s| s.name == "CString");
        assert!(c_string_struct.is_some());
        assert!(c_string_struct.unwrap().signature.as_ref().unwrap().contains("extern struct"));

        let from_slice_method = symbols.iter().find(|s| s.name == "fromSlice");
        assert!(from_slice_method.is_some());

        // Should extract extern C functions
        let malloc_fn = symbols.iter().find(|s| s.name == "malloc");
        assert!(malloc_fn.is_some());
        assert!(malloc_fn.unwrap().signature.as_ref().unwrap().contains("extern \"c\""));

        let free_fn = symbols.iter().find(|s| s.name == "free");
        assert!(free_fn.is_some());

        let printf_fn = symbols.iter().find(|s| s.name == "printf");
        assert!(printf_fn.is_some());
        assert!(printf_fn.unwrap().signature.as_ref().unwrap().contains("..."));

        // Should extract callback function types
        let callback_type = symbols.iter().find(|s| s.name == "CallbackFn");
        assert!(callback_type.is_some());
        assert!(callback_type.unwrap().signature.as_ref().unwrap().contains("callconv(.C)"));

        // Should extract math library
        let math_lib = symbols.iter().find(|s| s.name == "MathLib");
        assert!(math_lib.is_some());

        let fast_sqrt_fn = symbols.iter().find(|s| s.name == "fastSqrt");
        assert!(fast_sqrt_fn.is_some());

        let c_alloc_fn = symbols.iter().find(|s| s.name == "cAlloc");
        assert!(c_alloc_fn.is_some());

        // Should extract smart pointer pattern
        let unique_ptr_fn = symbols.iter().find(|s| s.name == "UniquePtr");
        assert!(unique_ptr_fn.is_some());

        // Should extract RAII pattern
        let file_handle = symbols.iter().find(|s| s.name == "FileHandle");
        assert!(file_handle.is_some());

        let open_method = symbols.iter().find(|s| s.name == "open");
        assert!(open_method.is_some());

        let close_method = symbols.iter().find(|s| s.name == "close");
        assert!(close_method.is_some());

        let write_method = symbols.iter().find(|s| s.name == "write");
        assert!(write_method.is_some());

        let read_method = symbols.iter().find(|s| s.name == "read");
        assert!(read_method.is_some());
    }

    #[test]
    fn test_extract_test_functions_and_build_configurations() {
        let zig_code = r#"
const std = @import("std");
const testing = std.testing;
const expect = testing.expect;
const expectEqual = testing.expectEqual;

// Test functions
test "basic arithmetic" {
    try expect(2 + 2 == 4);
    try expect(10 - 5 == 5);
    try expect(3 * 4 == 12);
    try expect(8 / 2 == 4);
}

test "string operations" {
    const str = "Hello, World!";
    try expect(str.len == 13);
    try expect(std.mem.eql(u8, str[0..5], "Hello"));
}

test "memory allocation" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();

    const allocator = arena.allocator();
    const memory = try allocator.alloc(u8, 100);
    try expect(memory.len == 100);
}

test "error handling" {
    const result = parseNumber("123");
    try expectEqual(@as(i32, 123), result);

    const error_result = parseNumber("abc");
    try testing.expectError(error.InvalidCharacter, error_result);
}

// Benchmark test
test "performance benchmark" {
    const iterations = 1000000;
    var sum: u64 = 0;

    const start = std.time.nanoTimestamp();
    for (0..iterations) |i| {
        sum += i;
    }
    const end = std.time.nanoTimestamp();

    const duration = end - start;
    std.debug.print("Sum calculation took {} ns\\n", .{duration});

    try expect(sum == (iterations * (iterations - 1)) / 2);
}

// Helper function for tests
fn parseNumber(input: []const u8) !i32 {
    return std.fmt.parseInt(i32, input, 10);
}

// Build configuration
pub const BuildConfig = struct {
    target: std.Target = .{},
    optimize: std.builtin.OptimizeMode = .Debug,
    linkage: std.builtin.LinkMode = .Dynamic,

    pub fn create(b: *std.Build) BuildConfig {
        return BuildConfig{
            .target = b.standardTargetOptions(.{}),
            .optimize = b.standardOptimizeOption(.{}),
        };
    }
};

// Compile-time constants and functions
const VERSION_MAJOR = 1;
const VERSION_MINOR = 0;
const VERSION_PATCH = 0;

comptime {
    if (VERSION_MAJOR < 1) {
        @compileError("Version major must be at least 1");
    }
}

fn versionString() []const u8 {
    return std.fmt.comptimePrint("{}.{}.{}", .{ VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH });
}

// Conditional compilation
const features = struct {
    const debug_mode = @import("builtin").mode == .Debug;
    const target_os = @import("builtin").target.os.tag;
    const is_windows = target_os == .windows;
    const is_linux = target_os == .linux;
    const is_macos = target_os == .macos;
};

// Platform-specific code
const PlatformApi = switch (features.target_os) {
    .windows => struct {
        pub fn getCurrentDirectory() ![]u8 {
            // Windows implementation
            return error.NotImplemented;
        }
    },
    .linux, .macos => struct {
        pub fn getCurrentDirectory() ![]u8 {
            // Unix implementation
            return error.NotImplemented;
        }
    },
    else => struct {
        pub fn getCurrentDirectory() ![]u8 {
            return error.UnsupportedPlatform;
        }
    },
};"#;

        let symbols = extract_symbols(zig_code);

        // Should extract test functions
        let basic_arithmetic_test = symbols.iter().find(|s| s.name == "basic arithmetic");
        assert!(basic_arithmetic_test.is_some());
        assert!(basic_arithmetic_test.unwrap().signature.as_ref().unwrap().contains("test \"basic arithmetic\""));

        let string_ops_test = symbols.iter().find(|s| s.name == "string operations");
        assert!(string_ops_test.is_some());

        let memory_test = symbols.iter().find(|s| s.name == "memory allocation");
        assert!(memory_test.is_some());

        let error_test = symbols.iter().find(|s| s.name == "error handling");
        assert!(error_test.is_some());

        let benchmark_test = symbols.iter().find(|s| s.name == "performance benchmark");
        assert!(benchmark_test.is_some());

        // Should extract helper functions
        let parse_number_fn = symbols.iter().find(|s| s.name == "parseNumber");
        assert!(parse_number_fn.is_some());

        // Should extract build configuration
        let build_config = symbols.iter().find(|s| s.name == "BuildConfig");
        assert!(build_config.is_some());

        let create_method = symbols.iter().find(|s| s.name == "create");
        assert!(create_method.is_some());

        // Should extract compile-time constants
        let version_major = symbols.iter().find(|s| s.name == "VERSION_MAJOR");
        assert!(version_major.is_some());
        assert_eq!(version_major.unwrap().kind, SymbolKind::Constant);

        let version_minor = symbols.iter().find(|s| s.name == "VERSION_MINOR");
        assert!(version_minor.is_some());

        let version_patch = symbols.iter().find(|s| s.name == "VERSION_PATCH");
        assert!(version_patch.is_some());

        // Should extract comptime functions
        let version_string_fn = symbols.iter().find(|s| s.name == "versionString");
        assert!(version_string_fn.is_some());

        // Should extract conditional compilation structures
        let features_struct = symbols.iter().find(|s| s.name == "features");
        assert!(features_struct.is_some());

        let debug_mode = symbols.iter().find(|s| s.name == "debug_mode");
        assert!(debug_mode.is_some());

        let target_os = symbols.iter().find(|s| s.name == "target_os");
        assert!(target_os.is_some());

        // Should extract platform-specific API
        let platform_api = symbols.iter().find(|s| s.name == "PlatformApi");
        assert!(platform_api.is_some());
        assert!(platform_api.unwrap().signature.as_ref().unwrap().contains("switch"));

        let get_current_dir_fn = symbols.iter().find(|s| s.name == "getCurrentDirectory");
        assert!(get_current_dir_fn.is_some());
    }

    #[test]
    fn test_infer_types_and_extract_relationships() {
        let zig_code = r#"
const std = @import("std");

const BaseShape = struct {
    x: f32,
    y: f32,

    pub fn area(self: BaseShape) f32 {
        _ = self;
        return 0.0;
    }
};

const Rectangle = struct {
    base: BaseShape,
    width: f32,
    height: f32,

    pub fn init(x: f32, y: f32, width: f32, height: f32) Rectangle {
        return Rectangle{
            .base = BaseShape{ .x = x, .y = y },
            .width = width,
            .height = height,
        };
    }

    pub fn area(self: Rectangle) f32 {
        return self.width * self.height;
    }
};

const Circle = struct {
    base: BaseShape,
    radius: f32,

    pub fn init(x: f32, y: f32, radius: f32) Circle {
        return Circle{
            .base = BaseShape{ .x = x, .y = y },
            .radius = radius,
        };
    }

    pub fn area(self: Circle) f32 {
        return std.math.pi * self.radius * self.radius;
    }
};

// Type alias
const ShapeList = std.ArrayList(BaseShape);

// Function that works with multiple types
fn calculateTotalArea(shapes: []const BaseShape) f32 {
    var total: f32 = 0.0;
    for (shapes) |shape| {
        total += shape.area();
    }
    return total;
}

// Generic container
const Container(comptime T: type) = struct {
    data: []T,
    allocator: std.mem.Allocator,

    const Self = @This();

    pub fn init(allocator: std.mem.Allocator) Self {
        return Self{
            .data = &[_]T{},
            .allocator = allocator,
        };
    }

    pub fn add(self: *Self, item: T) !void {
        // Implementation
    }
};"#;

        let symbols = extract_symbols(zig_code);
        let relationships = extract_relationships(zig_code, &symbols);

        // Should extract composition relationships
        assert!(relationships.len() > 0);

        let rectangle_composition = relationships.iter().find(|r| {
            r.kind.to_string() == "composition" &&
            symbols.iter().find(|s| s.id == r.from_symbol_id).map(|s| &s.name) == Some(&"Rectangle".to_string())
        });
        assert!(rectangle_composition.is_some());

        let circle_composition = relationships.iter().find(|r| {
            r.kind.to_string() == "composition" &&
            symbols.iter().find(|s| s.id == r.from_symbol_id).map(|s| &s.name) == Some(&"Circle".to_string())
        });
        assert!(circle_composition.is_some());

        // Should extract type aliases
        let shape_list_type = symbols.iter().find(|s| s.name == "ShapeList");
        assert!(shape_list_type.is_some());
        assert!(shape_list_type.unwrap().signature.as_ref().unwrap().contains("std.ArrayList(BaseShape)"));

        // Should extract generic types
        let container_type = symbols.iter().find(|s| s.name == "Container");
        assert!(container_type.is_some());
        assert!(container_type.unwrap().signature.as_ref().unwrap().contains("comptime T: type"));

        // Should handle polymorphic function calls
        let calculate_area_fn = symbols.iter().find(|s| s.name == "calculateTotalArea");
        assert!(calculate_area_fn.is_some());
        assert!(calculate_area_fn.unwrap().signature.as_ref().unwrap().contains("[]const BaseShape"));
    }
}