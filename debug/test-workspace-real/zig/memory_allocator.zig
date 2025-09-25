// Advanced Zig Memory Allocator Implementation
// Demonstrates complex Zig programming concepts including:
// - Custom allocators and memory management
// - Generic programming and type reflection
// - Error handling and optional types
// - Comptime evaluation and metaprogramming
// - Systems programming patterns

const std = @import("std");
const Allocator = std.mem.Allocator;
const testing = std.testing;
const assert = std.debug.assert;

/// Custom pool allocator for fixed-size blocks
pub fn PoolAllocator(comptime T: type) type {
    return struct {
        const Self = @This();
        const Node = struct {
            next: ?*Node = null,
            data: T = undefined,
        };

        backing_allocator: Allocator,
        free_list: ?*Node,
        pool: []Node,
        total_items: usize,
        allocated_items: usize,

        pub fn init(backing_allocator: Allocator, pool_size: usize) !Self {
            const pool = try backing_allocator.alloc(Node, pool_size);

            // Initialize free list
            for (pool[0..pool.len - 1]) |*node, i| {
                node.next = &pool[i + 1];
            }
            pool[pool.len - 1].next = null;

            return Self{
                .backing_allocator = backing_allocator,
                .free_list = if (pool.len > 0) &pool[0] else null,
                .pool = pool,
                .total_items = pool_size,
                .allocated_items = 0,
            };
        }

        pub fn deinit(self: *Self) void {
            self.backing_allocator.free(self.pool);
            self.* = undefined;
        }

        pub fn alloc(self: *Self) ?*T {
            if (self.free_list) |node| {
                self.free_list = node.next;
                self.allocated_items += 1;
                return &node.data;
            }
            return null;
        }

        pub fn free(self: *Self, item: *T) void {
            // Calculate node from data pointer
            const node_ptr = @fieldParentPtr(Node, "data", item);

            // Verify the pointer is within our pool
            const pool_start = @ptrToInt(&self.pool[0]);
            const pool_end = @ptrToInt(&self.pool[self.pool.len - 1]) + @sizeOf(Node);
            const node_addr = @ptrToInt(node_ptr);

            assert(node_addr >= pool_start and node_addr < pool_end);

            // Add to free list
            node_ptr.next = self.free_list;
            self.free_list = node_ptr;
            self.allocated_items -= 1;
        }

        pub fn allocator(self: *Self) Allocator {
            return Allocator{
                .ptr = self,
                .vtable = &.{
                    .alloc = allocFn,
                    .resize = Allocator.noResize,
                    .free = freeFn,
                },
            };
        }

        fn allocFn(ptr: *anyopaque, len: usize, ptr_align: u8, ret_addr: usize) ?[*]u8 {
            _ = ptr_align;
            _ = ret_addr;

            const self = @ptrCast(*Self, @alignCast(@alignOf(Self), ptr));

            if (len != @sizeOf(T) or len == 0) {
                return null;
            }

            if (self.alloc()) |item| {
                return @ptrCast([*]u8, item);
            }
            return null;
        }

        fn freeFn(ptr: *anyopaque, buf: []u8, buf_align: u8, ret_addr: usize) void {
            _ = buf_align;
            _ = ret_addr;

            const self = @ptrCast(*Self, @alignCast(@alignOf(Self), ptr));

            if (buf.len != @sizeOf(T)) {
                return;
            }

            const item = @ptrCast(*T, @alignCast(@alignOf(T), buf.ptr));
            self.free(item);
        }

        pub fn getStats(self: *const Self) struct { total: usize, allocated: usize, available: usize } {
            return .{
                .total = self.total_items,
                .allocated = self.allocated_items,
                .available = self.total_items - self.allocated_items,
            };
        }
    };
}

/// Arena allocator with automatic cleanup
pub const ArenaAllocator = struct {
    const Self = @This();
    const BufNode = struct {
        data: []u8,
        next: ?*BufNode,
    };

    backing_allocator: Allocator,
    buffer_list: ?*BufNode,
    end_index: usize,
    bytes_allocated: usize,

    pub fn init(backing_allocator: Allocator) Self {
        return Self{
            .backing_allocator = backing_allocator,
            .buffer_list = null,
            .end_index = 0,
            .bytes_allocated = 0,
        };
    }

    pub fn deinit(self: *Self) void {
        var maybe_node = self.buffer_list;
        while (maybe_node) |node| {
            const next = node.next;
            self.backing_allocator.free(node.data);
            self.backing_allocator.destroy(node);
            maybe_node = next;
        }
        self.* = undefined;
    }

    const default_buffer_size = 64 * 1024; // 64KB

    pub fn allocator(self: *Self) Allocator {
        return Allocator{
            .ptr = self,
            .vtable = &.{
                .alloc = alloc,
                .resize = resize,
                .free = free,
            },
        };
    }

    fn alloc(ptr: *anyopaque, len: usize, ptr_align: u8, ret_addr: usize) ?[*]u8 {
        _ = ret_addr;

        const self = @ptrCast(*Self, @alignCast(@alignOf(Self), ptr));
        const aligned_len = std.mem.alignForward(len, ptr_align);

        if (self.buffer_list) |current_node| {
            const remaining = current_node.data.len - self.end_index;
            if (remaining >= aligned_len) {
                const result = current_node.data.ptr + self.end_index;
                self.end_index += aligned_len;
                self.bytes_allocated += aligned_len;
                return result;
            }
        }

        // Need a new buffer
        const new_buffer_size = std.math.max(default_buffer_size, aligned_len);
        const new_buf = self.backing_allocator.alloc(u8, new_buffer_size) catch return null;

        const new_node = self.backing_allocator.create(BufNode) catch {
            self.backing_allocator.free(new_buf);
            return null;
        };

        new_node.* = BufNode{
            .data = new_buf,
            .next = self.buffer_list,
        };

        self.buffer_list = new_node;
        self.end_index = aligned_len;
        self.bytes_allocated += aligned_len;

        return new_buf.ptr;
    }

    fn resize(ptr: *anyopaque, buf: []u8, buf_align: u8, new_len: usize, ret_addr: usize) bool {
        _ = ptr;
        _ = buf;
        _ = buf_align;
        _ = new_len;
        _ = ret_addr;
        return false; // Arena allocator doesn't support resize
    }

    fn free(ptr: *anyopaque, buf: []u8, buf_align: u8, ret_addr: usize) void {
        _ = ptr;
        _ = buf;
        _ = buf_align;
        _ = ret_addr;
        // Arena allocator doesn't free individual allocations
    }

    pub fn getTotalBytesAllocated(self: *const Self) usize {
        return self.bytes_allocated;
    }
};

/// Stack allocator for LIFO allocation patterns
pub fn StackAllocator(comptime buffer_size: usize) type {
    return struct {
        const Self = @This();

        buffer: [buffer_size]u8,
        pos: usize,

        pub fn init() Self {
            return Self{
                .buffer = undefined,
                .pos = 0,
            };
        }

        pub fn allocator(self: *Self) Allocator {
            return Allocator{
                .ptr = self,
                .vtable = &.{
                    .alloc = alloc,
                    .resize = resize,
                    .free = free,
                },
            };
        }

        fn alloc(ptr: *anyopaque, len: usize, ptr_align: u8, ret_addr: usize) ?[*]u8 {
            _ = ret_addr;

            const self = @ptrCast(*Self, @alignCast(@alignOf(Self), ptr));
            const aligned_pos = std.mem.alignForward(self.pos, ptr_align);

            if (aligned_pos + len > buffer_size) {
                return null;
            }

            const result = &self.buffer[aligned_pos];
            self.pos = aligned_pos + len;
            return result;
        }

        fn resize(ptr: *anyopaque, buf: []u8, buf_align: u8, new_len: usize, ret_addr: usize) bool {
            _ = buf_align;
            _ = ret_addr;

            const self = @ptrCast(*Self, @alignCast(@alignOf(Self), ptr));
            const buf_start = @ptrToInt(buf.ptr);
            const buffer_start = @ptrToInt(&self.buffer[0]);
            const offset = buf_start - buffer_start;

            // Can only resize if it's the most recent allocation
            if (offset + buf.len == self.pos) {
                if (offset + new_len <= buffer_size) {
                    self.pos = offset + new_len;
                    return true;
                }
            }
            return false;
        }

        fn free(ptr: *anyopaque, buf: []u8, buf_align: u8, ret_addr: usize) void {
            _ = buf_align;
            _ = ret_addr;

            const self = @ptrCast(*Self, @alignCast(@alignOf(Self), ptr));
            const buf_start = @ptrToInt(buf.ptr);
            const buffer_start = @ptrToInt(&self.buffer[0]);
            const offset = buf_start - buffer_start;

            // Can only free if it's the most recent allocation (LIFO)
            if (offset + buf.len == self.pos) {
                self.pos = offset;
            }
        }

        pub fn reset(self: *Self) void {
            self.pos = 0;
        }

        pub fn getBytesUsed(self: *const Self) usize {
            return self.pos;
        }

        pub fn getBytesRemaining(self: *const Self) usize {
            return buffer_size - self.pos;
        }
    };
}

/// Generic data structure using custom allocators
pub fn ArrayList(comptime T: type, comptime allocator_type: type) type {
    return struct {
        const Self = @This();

        items: []T,
        capacity: usize,
        allocator: Allocator,

        pub fn init(allocator: Allocator) Self {
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
            try self.ensureCapacity(self.items.len + 1);
            self.items.ptr[self.items.len] = item;
            self.items.len += 1;
        }

        pub fn pop(self: *Self) ?T {
            if (self.items.len == 0) return null;
            self.items.len -= 1;
            return self.items[self.items.len];
        }

        fn ensureCapacity(self: *Self, new_capacity: usize) !void {
            if (new_capacity <= self.capacity) return;

            const new_cap = std.math.max(new_capacity, self.capacity * 2);
            const new_memory = try self.allocator.alloc(T, new_cap);

            if (self.items.len > 0) {
                std.mem.copy(T, new_memory[0..self.items.len], self.items);
                self.allocator.free(self.items.ptr[0..self.capacity]);
            }

            self.items = new_memory[0..self.items.len];
            self.capacity = new_cap;
        }

        pub fn toSlice(self: *Self) []T {
            return self.items;
        }
    };
}

// Example usage and tests
const Point = struct {
    x: f32,
    y: f32,

    pub fn init(x: f32, y: f32) Point {
        return Point{ .x = x, .y = y };
    }

    pub fn distance(self: Point, other: Point) f32 {
        const dx = self.x - other.x;
        const dy = self.y - other.y;
        return std.math.sqrt(dx * dx + dy * dy);
    }
};

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    std.debug.print("=== Zig Advanced Memory Management Demo ===\n");

    // Pool allocator demo
    {
        var pool = try PoolAllocator(Point).init(allocator, 1000);
        defer pool.deinit();

        const pool_alloc = pool.allocator();

        std.debug.print("\n--- Pool Allocator Demo ---\n");

        var points: [10]*Point = undefined;
        for (points) |*point, i| {
            point.* = try pool_alloc.create(Point);
            point.*.* = Point.init(@intToFloat(f32, i), @intToFloat(f32, i * 2));
        }

        const stats = pool.getStats();
        std.debug.print("Pool stats: {}/{} allocated\n", .{ stats.allocated, stats.total });

        for (points) |point| {
            pool_alloc.destroy(point);
        }

        const final_stats = pool.getStats();
        std.debug.print("After cleanup: {}/{} allocated\n", .{ final_stats.allocated, final_stats.total });
    }

    // Arena allocator demo
    {
        var arena = ArenaAllocator.init(allocator);
        defer arena.deinit();

        const arena_alloc = arena.allocator();

        std.debug.print("\n--- Arena Allocator Demo ---\n");

        var list = ArrayList(i32, ArenaAllocator).init(arena_alloc);

        for (0..100) |i| {
            try list.append(@intCast(i32, i));
        }

        std.debug.print("Arena allocated {} bytes for {} items\n", .{ arena.getTotalBytesAllocated(), list.items.len });

        const sum = blk: {
            var s: i32 = 0;
            for (list.toSlice()) |item| {
                s += item;
            }
            break :blk s;
        };

        std.debug.print("Sum of items: {}\n", .{sum});
    }

    // Stack allocator demo
    {
        var stack = StackAllocator(4096).init();
        const stack_alloc = stack.allocator();

        std.debug.print("\n--- Stack Allocator Demo ---\n");

        const buffer1 = try stack_alloc.alloc(u8, 100);
        std.debug.print("Allocated 100 bytes, {} bytes used\n", .{stack.getBytesUsed()});

        const buffer2 = try stack_alloc.alloc(u8, 200);
        std.debug.print("Allocated 200 more bytes, {} bytes used\n", .{stack.getBytesUsed()});

        // Fill buffers with test data
        for (buffer1) |*byte, i| {
            byte.* = @truncate(u8, i);
        }

        for (buffer2) |*byte, i| {
            byte.* = @truncate(u8, i + 100);
        }

        // Free in LIFO order
        stack_alloc.free(buffer2);
        std.debug.print("Freed buffer2, {} bytes used\n", .{stack.getBytesUsed()});

        stack_alloc.free(buffer1);
        std.debug.print("Freed buffer1, {} bytes used\n", .{stack.getBytesUsed()});

        assert(stack.getBytesUsed() == 0);
    }

    std.debug.print("\n=== All demos completed successfully! ===\n");
}

// Unit tests
test "pool allocator basic operations" {
    var pool = try PoolAllocator(i32).init(testing.allocator, 10);
    defer pool.deinit();

    const item1 = pool.alloc().?;
    const item2 = pool.alloc().?;

    item1.* = 42;
    item2.* = 24;

    try testing.expect(item1.* == 42);
    try testing.expect(item2.* == 24);

    pool.free(item1);
    pool.free(item2);

    const stats = pool.getStats();
    try testing.expect(stats.allocated == 0);
}

test "arena allocator" {
    var arena = ArenaAllocator.init(testing.allocator);
    defer arena.deinit();

    const alloc = arena.allocator();

    const slice1 = try alloc.alloc(u8, 100);
    const slice2 = try alloc.alloc(i32, 25);

    try testing.expect(slice1.len == 100);
    try testing.expect(slice2.len == 25);
    try testing.expect(arena.getTotalBytesAllocated() >= 100 + 25 * 4);
}

test "stack allocator LIFO behavior" {
    var stack = StackAllocator(1024).init();
    const alloc = stack.allocator();

    const buf1 = try alloc.alloc(u8, 100);
    const buf2 = try alloc.alloc(u8, 200);

    try testing.expect(stack.getBytesUsed() >= 300);

    // Can only free in LIFO order
    alloc.free(buf2);
    alloc.free(buf1);

    try testing.expect(stack.getBytesUsed() == 0);
}