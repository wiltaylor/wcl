const std = @import("std");
const Allocator = std.mem.Allocator;
const json = std.json;
const ManagedList = std.array_list.Managed;

const c = @cImport({
    @cInclude("wcl.h");
});

// ── Error Type ───────────────────────────────────────────────────────────

pub const WclError = error{
    ParseFailed,
    ParseFileFailed,
    JsonParseFailed,
    QueryFailed,
    LibraryListFailed,
    DocumentClosed,
};

// ── Types ────────────────────────────────────────────────────────────────

pub const Diagnostic = struct {
    severity: []const u8,
    message: []const u8,
    code: ?[]const u8 = null,
};

pub const ParseOptions = struct {
    root_dir: ?[]const u8 = null,
    allow_imports: ?bool = null,
    max_import_depth: ?u32 = null,
    max_macro_depth: ?u32 = null,
    max_loop_depth: ?u32 = null,
    max_iterations: ?u32 = null,
};

// ── Document ─────────────────────────────────────────────────────────────

pub const Document = struct {
    handle: *anyopaque,
    closed: bool = false,

    /// Release the underlying Rust resources.
    /// Safe to call multiple times.
    pub fn deinit(self: *Document) void {
        if (self.closed) return;
        self.closed = true;
        c.wcl_ffi_document_free(self.handle);
    }

    /// Get the evaluated values as parsed JSON.
    pub fn values(self: *const Document, allocator: Allocator) !json.Parsed(json.Value) {
        if (self.closed) return WclError.DocumentClosed;
        const raw = c.wcl_ffi_document_values(self.handle);
        defer c.wcl_ffi_string_free(raw);
        const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
        return json.parseFromSlice(json.Value, allocator, slice, .{ .allocate = .alloc_always });
    }

    /// Get the evaluated values as a raw JSON string (caller owns the memory).
    pub fn valuesRaw(self: *const Document, allocator: Allocator) ![]const u8 {
        if (self.closed) return WclError.DocumentClosed;
        const raw = c.wcl_ffi_document_values(self.handle);
        defer c.wcl_ffi_string_free(raw);
        const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
        return allocator.dupe(u8, slice);
    }

    /// Check if the document has any error diagnostics.
    pub fn hasErrors(self: *const Document) bool {
        if (self.closed) return false;
        return c.wcl_ffi_document_has_errors(self.handle);
    }

    /// Get only the error diagnostics as parsed JSON.
    pub fn errors(self: *const Document, allocator: Allocator) !json.Parsed(json.Value) {
        if (self.closed) return WclError.DocumentClosed;
        const raw = c.wcl_ffi_document_errors(self.handle);
        defer c.wcl_ffi_string_free(raw);
        const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
        return json.parseFromSlice(json.Value, allocator, slice, .{ .allocate = .alloc_always });
    }

    /// Get all diagnostics (errors, warnings, etc.) as parsed JSON.
    pub fn diagnostics(self: *const Document, allocator: Allocator) !json.Parsed(json.Value) {
        if (self.closed) return WclError.DocumentClosed;
        const raw = c.wcl_ffi_document_diagnostics(self.handle);
        defer c.wcl_ffi_string_free(raw);
        const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
        return json.parseFromSlice(json.Value, allocator, slice, .{ .allocate = .alloc_always });
    }

    /// Execute a WCL query against the document.
    pub fn query(self: *const Document, allocator: Allocator, q: []const u8) !json.Parsed(json.Value) {
        if (self.closed) return WclError.DocumentClosed;
        const c_query = try toNullTerminated(allocator, q);
        defer allocator.free(c_query);
        const raw = c.wcl_ffi_document_query(self.handle, c_query.ptr);
        defer c.wcl_ffi_string_free(raw);
        const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));

        // Parse the wrapper: {"ok": <value>} or {"error": "message"}
        var parsed = try json.parseFromSlice(json.Value, allocator, slice, .{ .allocate = .alloc_always });

        if (parsed.value.object.get("error")) |_| {
            parsed.deinit();
            return WclError.QueryFailed;
        }

        return parsed;
    }

    /// Get all blocks as parsed JSON.
    pub fn blocks(self: *const Document, allocator: Allocator) !json.Parsed(json.Value) {
        if (self.closed) return WclError.DocumentClosed;
        const raw = c.wcl_ffi_document_blocks(self.handle);
        defer c.wcl_ffi_string_free(raw);
        const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
        return json.parseFromSlice(json.Value, allocator, slice, .{ .allocate = .alloc_always });
    }

    /// Get blocks of a specific type as parsed JSON.
    pub fn blocksOfType(self: *const Document, allocator: Allocator, kind: []const u8) !json.Parsed(json.Value) {
        if (self.closed) return WclError.DocumentClosed;
        const c_kind = try toNullTerminated(allocator, kind);
        defer allocator.free(c_kind);
        const raw = c.wcl_ffi_document_blocks_of_type(self.handle, c_kind.ptr);
        defer c.wcl_ffi_string_free(raw);
        const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
        return json.parseFromSlice(json.Value, allocator, slice, .{ .allocate = .alloc_always });
    }
};

// ── Public API ───────────────────────────────────────────────────────────

/// Parse a WCL source string and return a Document.
pub fn parse(allocator: Allocator, source: []const u8, opts: ?ParseOptions) !Document {
    const c_source = try toNullTerminated(allocator, source);
    defer allocator.free(c_source);

    var c_opts: ?[:0]const u8 = null;
    defer if (c_opts) |o| allocator.free(o);
    if (opts) |o| {
        c_opts = try marshalOptions(allocator, o);
    }

    const c_opts_ptr: ?[*:0]const u8 = if (c_opts) |o| o.ptr else null;
    const ptr = c.wcl_ffi_parse(c_source.ptr, c_opts_ptr);
    if (ptr == null) return WclError.ParseFailed;
    return Document{ .handle = ptr.? };
}

/// Parse a WCL file and return a Document.
pub fn parseFile(allocator: Allocator, path: []const u8, opts: ?ParseOptions) !Document {
    const c_path = try toNullTerminated(allocator, path);
    defer allocator.free(c_path);

    var c_opts: ?[:0]const u8 = null;
    defer if (c_opts) |o| allocator.free(o);
    if (opts) |o| {
        c_opts = try marshalOptions(allocator, o);
    }

    const c_opts_ptr: ?[*:0]const u8 = if (c_opts) |o| o.ptr else null;
    const ptr = c.wcl_ffi_parse_file(c_path.ptr, c_opts_ptr);
    if (ptr == null) return WclError.ParseFileFailed;
    return Document{ .handle = ptr.? };
}

/// List installed WCL libraries as parsed JSON.
pub fn listLibraries(allocator: Allocator) !json.Parsed(json.Value) {
    const raw = c.wcl_ffi_list_libraries();
    defer c.wcl_ffi_string_free(raw);
    const slice = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));

    var parsed = try json.parseFromSlice(json.Value, allocator, slice, .{ .allocate = .alloc_always });

    if (parsed.value.object.get("error")) |_| {
        parsed.deinit();
        return WclError.LibraryListFailed;
    }

    return parsed;
}

// ── Callback Support ─────────────────────────────────────────────────────

const CallbackEntry = struct {
    func: *const fn (Allocator, json.Value) anyerror!json.Value,
    allocator: Allocator,
};

var callback_mutex: std.Thread.Mutex = .{};
var callback_map: ?std.AutoHashMap(usize, CallbackEntry) = null;
var callback_next_id: usize = 1;

fn getCallbackMap() *std.AutoHashMap(usize, CallbackEntry) {
    if (callback_map == null) {
        callback_map = std.AutoHashMap(usize, CallbackEntry).init(std.heap.page_allocator);
    }
    return &callback_map.?;
}

fn registerCallback(entry: CallbackEntry) usize {
    callback_mutex.lock();
    defer callback_mutex.unlock();
    const id = callback_next_id;
    callback_next_id += 1;
    getCallbackMap().put(id, entry) catch @panic("OOM in callback registry");
    return id;
}

fn unregisterCallback(id: usize) void {
    callback_mutex.lock();
    defer callback_mutex.unlock();
    if (callback_map) |*map| {
        _ = map.remove(id);
    }
}

fn lookupCallback(id: usize) ?CallbackEntry {
    callback_mutex.lock();
    defer callback_mutex.unlock();
    if (callback_map) |*map| {
        return map.get(id);
    }
    return null;
}

/// C-callable trampoline that bridges FFI callbacks to Zig functions.
export fn zigCallbackTrampoline(ctx: ?*anyopaque, args_json: [*c]const u8) callconv(.c) [*c]u8 {
    const id: usize = @intFromPtr(ctx);
    const entry = lookupCallback(id) orelse return toCString("ERR:callback not found");

    const args_slice = std.mem.span(@as([*:0]const u8, @ptrCast(args_json)));
    var parsed = json.parseFromSlice(json.Value, entry.allocator, args_slice, .{ .allocate = .alloc_always }) catch
        return toCString("ERR:failed to parse args JSON");
    defer parsed.deinit();

    const result = entry.func(entry.allocator, parsed.value) catch
        return toCString("ERR:callback error");

    // Serialize result to JSON
    const serialized = json.Stringify.valueAlloc(entry.allocator, result, .{}) catch
        return toCString("ERR:failed to serialize result");
    defer entry.allocator.free(serialized);

    const out = std.heap.c_allocator.alloc(u8, serialized.len + 1) catch
        return toCString("ERR:OOM");
    @memcpy(out[0..serialized.len], serialized);
    out[serialized.len] = 0;
    return @ptrCast(out.ptr);
}

fn toCString(comptime s: []const u8) [*c]u8 {
    const buf = std.heap.c_allocator.alloc(u8, s.len + 1) catch return null;
    @memcpy(buf[0..s.len], s);
    buf[s.len] = 0;
    return @ptrCast(buf.ptr);
}

/// Parse a WCL source string with custom callback functions.
pub fn parseWithFunctions(
    allocator: Allocator,
    source: []const u8,
    opts: ?ParseOptions,
    functions: std.StringHashMap(*const fn (Allocator, json.Value) anyerror!json.Value),
) !Document {
    const count = functions.count();
    if (count == 0) return parse(allocator, source, opts);

    const c_source = try toNullTerminated(allocator, source);
    defer allocator.free(c_source);

    var c_opts: ?[:0]const u8 = null;
    defer if (c_opts) |o| allocator.free(o);
    if (opts) |o| {
        c_opts = try marshalOptions(allocator, o);
    }

    var names = try allocator.alloc([*c]const u8, count);
    defer allocator.free(names);
    var name_strs = try allocator.alloc([:0]const u8, count);
    defer {
        for (name_strs[0..count]) |s| allocator.free(s);
        allocator.free(name_strs);
    }
    var callbacks = try allocator.alloc(c.WclCallbackFn, count);
    defer allocator.free(callbacks);
    var contexts = try allocator.alloc(usize, count);
    defer allocator.free(contexts);

    var i: usize = 0;
    var iter = functions.iterator();
    while (iter.next()) |entry| {
        name_strs[i] = try toNullTerminated(allocator, entry.key_ptr.*);
        names[i] = name_strs[i].ptr;
        callbacks[i] = @ptrCast(&zigCallbackTrampoline);
        contexts[i] = registerCallback(.{ .func = entry.value_ptr.*, .allocator = allocator });
        i += 1;
    }

    const c_opts_ptr: ?[*:0]const u8 = if (c_opts) |o| o.ptr else null;
    const ptr = c.wcl_ffi_parse_with_functions(
        c_source.ptr,
        c_opts_ptr,
        @ptrCast(names.ptr),
        @ptrCast(callbacks.ptr),
        @ptrCast(contexts.ptr),
        count,
    );

    if (ptr == null) {
        for (contexts[0..count]) |ctx_id| unregisterCallback(ctx_id);
        return WclError.ParseFailed;
    }

    return Document{ .handle = ptr.? };
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn toNullTerminated(allocator: Allocator, slice: []const u8) ![:0]const u8 {
    return allocator.dupeZ(u8, slice);
}

fn marshalOptions(allocator: Allocator, opts: ParseOptions) ![:0]const u8 {
    var buf = ManagedList(u8).init(allocator);
    defer buf.deinit();

    try buf.appendSlice("{");
    var first = true;

    if (opts.root_dir) |v| {
        try appendJsonField(&buf, &first, "rootDir", v);
    }
    if (opts.allow_imports) |v| {
        try appendJsonBoolField(&buf, &first, "allowImports", v);
    }
    if (opts.max_import_depth) |v| {
        try appendJsonIntField(&buf, &first, "maxImportDepth", v);
    }
    if (opts.max_macro_depth) |v| {
        try appendJsonIntField(&buf, &first, "maxMacroDepth", v);
    }
    if (opts.max_loop_depth) |v| {
        try appendJsonIntField(&buf, &first, "maxLoopDepth", v);
    }
    if (opts.max_iterations) |v| {
        try appendJsonIntField(&buf, &first, "maxIterations", v);
    }

    try buf.appendSlice("}");

    const owned = try buf.toOwnedSlice();
    const result = try allocator.realloc(owned, owned.len + 1);
    result[owned.len] = 0;
    return result[0..owned.len :0];
}

fn appendJsonField(buf: *ManagedList(u8), first: *bool, key: []const u8, value: []const u8) !void {
    if (!first.*) try buf.appendSlice(",");
    first.* = false;
    try buf.appendSlice("\"");
    try buf.appendSlice(key);
    try buf.appendSlice("\":\"");
    try buf.appendSlice(value);
    try buf.appendSlice("\"");
}

fn appendJsonBoolField(buf: *ManagedList(u8), first: *bool, key: []const u8, value: bool) !void {
    if (!first.*) try buf.appendSlice(",");
    first.* = false;
    try buf.appendSlice("\"");
    try buf.appendSlice(key);
    try buf.appendSlice("\":");
    try buf.appendSlice(if (value) "true" else "false");
}

fn appendJsonIntField(buf: *ManagedList(u8), first: *bool, key: []const u8, value: u32) !void {
    if (!first.*) try buf.appendSlice(",");
    first.* = false;
    try buf.appendSlice("\"");
    try buf.appendSlice(key);
    try buf.appendSlice("\":");
    var num_buf: [10]u8 = undefined;
    const num_str = std.fmt.bufPrint(&num_buf, "{d}", .{value}) catch unreachable;
    try buf.appendSlice(num_str);
}

// ── Tests ────────────────────────────────────────────────────────────────

test "parse simple string" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator, "name = \"hello\"\nport = 8080", null);
    defer doc.deinit();

    var vals = try doc.values(allocator);
    defer vals.deinit();

    const obj = vals.value.object;
    try std.testing.expectEqualStrings("hello", obj.get("name").?.string);
    try std.testing.expectEqual(@as(i64, 8080), obj.get("port").?.integer);
}

test "parse with errors" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator,
        \\server web {
        \\    port = "not_a_number"
        \\}
        \\schema "server" {
        \\    port: int
        \\}
    , null);
    defer doc.deinit();

    try std.testing.expect(doc.hasErrors());

    var errs = try doc.errors(allocator);
    defer errs.deinit();
    try std.testing.expect(errs.value.array.items.len > 0);
}

test "query blocks" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator,
        \\server svc-api {
        \\    port = 8080
        \\}
        \\server svc-admin {
        \\    port = 9090
        \\}
    , null);
    defer doc.deinit();

    var result = try doc.query(allocator, "server | .port");
    defer result.deinit();

    const ok_val = result.value.object.get("ok").?;
    try std.testing.expectEqual(@as(usize, 2), ok_val.array.items.len);
}

test "blocks and blocksOfType" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator,
        \\server web {
        \\    port = 8080
        \\}
        \\database main-db {
        \\    port = 5432
        \\}
    , null);
    defer doc.deinit();

    var all = try doc.blocks(allocator);
    defer all.deinit();
    try std.testing.expectEqual(@as(usize, 2), all.value.array.items.len);

    var servers = try doc.blocksOfType(allocator, "server");
    defer servers.deinit();
    try std.testing.expectEqual(@as(usize, 1), servers.value.array.items.len);
}

test "values raw" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator, "x = 42", null);
    defer doc.deinit();

    const raw = try doc.valuesRaw(allocator);
    defer allocator.free(raw);
    try std.testing.expect(std.mem.indexOf(u8, raw, "42") != null);
}

test "document deinit safety" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator, "x = 1", null);
    doc.deinit();
    doc.deinit();
    try std.testing.expect(doc.closed);
}

test "parse with options" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator, "x = 1", ParseOptions{
        .allow_imports = false,
        .max_iterations = 100,
    });
    defer doc.deinit();

    try std.testing.expect(!doc.hasErrors());
}

test "diagnostics" {
    const allocator = std.testing.allocator;
    var doc = try parse(allocator, "x = 1", null);
    defer doc.deinit();

    var diags = try doc.diagnostics(allocator);
    defer diags.deinit();
    try std.testing.expectEqual(@as(usize, 0), diags.value.array.items.len);
}
