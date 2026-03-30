const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const ts_mod = b.addModule("tree-sitter-wcl", .{
        .root_source_file = b.path("src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    ts_mod.addCSourceFile(.{
        .file = b.path("../../src/parser.c"),
        .flags = &.{ "-std=c11", "-fPIC" },
    });
    ts_mod.addCSourceFile(.{
        .file = b.path("../../src/scanner.c"),
        .flags = &.{ "-std=c11", "-fPIC" },
    });
    ts_mod.addIncludePath(b.path("../../src"));
    ts_mod.linkSystemLibrary("c", .{});

    const lib = b.addLibrary(.{
        .linkage = .static,
        .name = "tree-sitter-wcl",
        .root_module = ts_mod,
    });
    b.installArtifact(lib);

    // Tests: use explicit target on Linux to avoid .sframe issues
    const test_target = blk: {
        const resolved = target.result;
        if (resolved.os.tag == .linux) {
            break :blk b.resolveTargetQuery(.{
                .cpu_arch = resolved.cpu.arch,
                .os_tag = .linux,
                .abi = .gnu,
            });
        }
        break :blk target;
    };

    const test_mod = b.addModule("tree-sitter-wcl-test", .{
        .root_source_file = b.path("src/root.zig"),
        .target = test_target,
        .optimize = optimize,
    });

    test_mod.addCSourceFile(.{
        .file = b.path("../../src/parser.c"),
        .flags = &.{ "-std=c11", "-fPIC" },
    });
    test_mod.addCSourceFile(.{
        .file = b.path("../../src/scanner.c"),
        .flags = &.{ "-std=c11", "-fPIC" },
    });
    test_mod.addIncludePath(b.path("../../src"));
    test_mod.linkSystemLibrary("c", .{});

    const tests = b.addTest(.{
        .root_module = test_mod,
    });

    const run_tests = b.addRunArtifact(tests);
    const test_step = b.step("test", "Run tree-sitter-wcl binding tests");
    test_step.dependOn(&run_tests.step);
}
