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
    remaining().is_some_and(|left| left < RESERVE)
}

#[cfg(not(target_os = "linux"))]
fn remaining() -> Option<usize> {
    stacker::remaining_stack()
}

/// On Linux the main thread is measured against `RLIMIT_STACK` below the
/// top of its stack mapping. stacker asks `pthread_getattr_np`, and musl's
/// answers for the main thread with only the part of the stack mapped so
/// far — about 128 KiB at startup, under [`RESERVE`] — although the kernel
/// grows the mapping on demand up to the rlimit. Every other thread, and a
/// main thread whose bounds `/proc` doesn't give, falls back to stacker.
#[cfg(target_os = "linux")]
fn remaining() -> Option<usize> {
    let probe = 0u8;
    let sp = std::hint::black_box(&probe) as *const u8 as usize;
    match linux::main_stack() {
        Some(main) if main.contains(sp) => main.remaining(sp),
        _ => stacker::remaining_stack(),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::OnceLock;

    /// The main thread's stack: the top of its mapping and the soft
    /// `RLIMIT_STACK`, `None` when the limit is unlimited.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) struct MainStack {
        pub(super) top: usize,
        pub(super) limit: Option<usize>,
    }

    /// Read once per process; neither the mapping's top nor the soft
    /// limit the kernel grows the stack against moves after startup.
    pub(super) fn main_stack() -> Option<MainStack> {
        static MAIN: OnceLock<Option<MainStack>> = OnceLock::new();
        *MAIN.get_or_init(|| {
            let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
            let limits = std::fs::read_to_string("/proc/self/limits").ok()?;
            Some(MainStack {
                top: stack_top(&maps)?,
                limit: stack_limit(&limits)?,
            })
        })
    }

    /// How far below `top` an unlimited main stack is taken to reach, for
    /// telling the main thread's stack pointer from another thread's.
    const MAX_SPAN: usize = 1 << 30;

    impl MainStack {
        /// Whether `sp` is on this stack rather than a spawned thread's.
        pub(super) fn contains(self, sp: usize) -> bool {
            let span = self.limit.unwrap_or(MAX_SPAN);
            sp <= self.top && sp > self.top.saturating_sub(span)
        }

        /// Bytes left below `sp`; `None` when the stack is unlimited.
        pub(super) fn remaining(self, sp: usize) -> Option<usize> {
            let bottom = self.top.saturating_sub(self.limit?);
            Some(sp.saturating_sub(bottom))
        }
    }

    /// The end address of the `[stack]` line in `/proc/self/maps`.
    pub(super) fn stack_top(maps: &str) -> Option<usize> {
        let line = maps.lines().find(|l| l.ends_with("[stack]"))?;
        let range = line.split_whitespace().next()?;
        let (_, end) = range.split_once('-')?;
        usize::from_str_radix(end, 16).ok()
    }

    /// The soft limit on the `Max stack size` line of `/proc/self/limits`:
    /// `Some(None)` for `unlimited`, `None` when the line is missing.
    pub(super) fn stack_limit(limits: &str) -> Option<Option<usize>> {
        let line = limits.lines().find(|l| l.starts_with("Max stack size"))?;
        let soft = line["Max stack size".len()..].split_whitespace().next()?;
        if soft == "unlimited" {
            return Some(None);
        }
        soft.parse().ok().map(Some)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const MAPS: &str = "\
55d0c0a00000-55d0c0a21000 r--p 00000000 00:1f 123 /usr/bin/wcl
7ffd3a5e0000-7ffd3a601000 rw-p 00000000 00:00 0                          [stack]
7ffd3a7f2000-7ffd3a7f6000 r--p 00000000 00:00 0                          [vvar]
";

        const LIMITS: &str = "\
Limit                     Soft Limit           Hard Limit           Units
Max file size             unlimited            unlimited            bytes
Max stack size            8388608              unlimited            bytes
Max core file size        unlimited            unlimited            bytes
";

        #[test]
        fn reads_the_top_of_the_stack_mapping() {
            assert_eq!(stack_top(MAPS), Some(0x7ffd3a601000));
        }

        #[test]
        fn reads_the_soft_stack_limit() {
            assert_eq!(stack_limit(LIMITS), Some(Some(8 << 20)));
            let unlimited = LIMITS.replace("8388608  ", "unlimited");
            assert_eq!(stack_limit(&unlimited), Some(None));
            assert_eq!(stack_limit("Limit  Soft Limit\n"), None);
        }

        #[test]
        fn measures_the_main_stack_against_the_rlimit() {
            let main = MainStack {
                top: 0x7ffd3a601000,
                limit: Some(8 << 20),
            };
            let sp = main.top - (100 << 10);
            assert!(main.contains(sp));
            assert_eq!(main.remaining(sp), Some((8 << 20) - (100 << 10)));
            assert!(!main.contains(main.top - (9 << 20)));
            assert!(!main.contains(main.top + 1));
        }

        #[test]
        fn an_unlimited_main_stack_is_not_measured() {
            let main = MainStack {
                top: 0x7ffd3a601000,
                limit: None,
            };
            assert!(main.contains(main.top - (100 << 10)));
            assert_eq!(main.remaining(main.top - (100 << 10)), None);
        }

        #[test]
        fn this_process_reports_its_main_stack() {
            assert!(main_stack().is_some_and(|m| m.top > 0));
        }
    }
}
