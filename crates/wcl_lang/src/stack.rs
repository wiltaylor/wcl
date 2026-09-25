//! The remaining-stack guard under the parser's and evaluator's depth
//! caps.
//!
//! The caps count levels, but what a level costs on the Rust stack
//! varies: an unoptimised build's frames are several times an optimised
//! build's, and the caps compound — a `fn` body nested 256 expressions
//! deep, called 200 calls deep, recurses through both. So the parser's
//! and evaluator's recursion points also read the stack actually left,
//! and report the same depth error they would at the cap when it runs
//! low, instead of overflowing the thread's stack and aborting the
//! process. A 2 MiB thread (Rust's default for spawned threads, and the
//! LSP's workers) is the smallest the guard is sized for.
//!
//! The guard only measures; it never grows the stack onto the heap.
//! Growing would turn a depth error into unbounded memory use, and the
//! counted caps already bound every legitimate document well inside an
//! 8 MiB main-thread stack.

/// Stack that must still be free when a guarded recursion point is
/// entered. Covers the frames between one guarded point and the next —
/// the largest, an unoptimised parse level through a `${…}` slot, is
/// about 25 KiB — plus the error path's own frames, with room to spare.
const RESERVE: usize = 256 * 1024;

/// `true` when less than [`RESERVE`] of this thread's stack is left.
/// `false` when the platform can't say how much is left.
pub(crate) fn is_low() -> bool {
    stacker::remaining_stack().is_some_and(|left| left < RESERVE)
}
