const std = @import("std");

pub const TSLanguage = opaque {};

extern fn tree_sitter_wcl() *const TSLanguage;

/// Get the tree-sitter Language for WCL.
pub fn language() *const TSLanguage {
    return tree_sitter_wcl();
}

test "can load grammar" {
    const lang = language();
    // If we got here without crashing, the grammar loaded successfully.
    try std.testing.expect(@intFromPtr(lang) != 0);
}
