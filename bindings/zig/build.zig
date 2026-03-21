const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Resolve the correct prebuilt static library for the host platform.
    const lib_path = resolveLibPath(target) orelse
        @panic("unsupported target for prebuilt wcl_ffi library");

    const wcl_mod = b.addModule("wcl", .{
        .root_source_file = b.path("src/wcl.zig"),
        .target = target,
        .optimize = optimize,
    });

    wcl_mod.addObjectFile(b.path(lib_path));
    wcl_mod.addIncludePath(b.path("."));
    linkSystemDeps(wcl_mod, target);

    // For tests on Linux, use an explicit target so Zig uses its bundled libc
    // instead of the system crt objects (avoids .sframe relocation errors with
    // newer GCC toolchains).
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

    const test_mod = b.addModule("wcl_test", .{
        .root_source_file = b.path("src/wcl.zig"),
        .target = test_target,
        .optimize = optimize,
    });
    test_mod.addObjectFile(b.path(lib_path));
    test_mod.addIncludePath(b.path("."));
    linkSystemDeps(test_mod, test_target);

    const tests = b.addTest(.{
        .root_module = test_mod,
    });

    const run_tests = b.addRunArtifact(tests);
    const test_step = b.step("test", "Run WCL binding tests");
    test_step.dependOn(&run_tests.step);
}

fn linkSystemDeps(mod: *std.Build.Module, target: std.Build.ResolvedTarget) void {
    const resolved = target.result;
    if (resolved.os.tag == .linux) {
        mod.linkSystemLibrary("m", .{});
        mod.linkSystemLibrary("dl", .{});
        mod.linkSystemLibrary("pthread", .{});
        mod.linkSystemLibrary("unwind", .{});
        mod.linkSystemLibrary("gcc_s", .{});
    } else if (resolved.os.tag == .macos) {
        mod.linkSystemLibrary("m", .{});
        mod.linkSystemLibrary("dl", .{});
        mod.linkSystemLibrary("pthread", .{});
        mod.linkFramework("Security", .{});
    } else if (resolved.os.tag == .windows) {
        mod.linkSystemLibrary("ws2_32", .{});
        mod.linkSystemLibrary("bcrypt", .{});
        mod.linkSystemLibrary("userenv", .{});
    }
}

fn resolveLibPath(target: std.Build.ResolvedTarget) ?[]const u8 {
    const resolved = target.result;
    return switch (resolved.os.tag) {
        .linux => switch (resolved.cpu.arch) {
            .x86_64 => "lib/linux_amd64/libwcl_ffi.a",
            .aarch64 => "lib/linux_arm64/libwcl_ffi.a",
            else => null,
        },
        .macos => switch (resolved.cpu.arch) {
            .x86_64 => "lib/darwin_amd64/libwcl_ffi.a",
            .aarch64 => "lib/darwin_arm64/libwcl_ffi.a",
            else => null,
        },
        .windows => switch (resolved.cpu.arch) {
            .x86_64 => "lib/windows_amd64/wcl_ffi.lib",
            else => null,
        },
        else => null,
    };
}
