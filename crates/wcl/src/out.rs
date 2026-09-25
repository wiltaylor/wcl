//! Writing to stdout and stderr without panicking on a closed stream.
//!
//! `print!` and `eprint!` panic when the stream they write to has gone:
//! `wcl parse big.wcl | head -1` used to die with "failed printing to
//! stdout: Broken pipe" and exit 101. Everything `wcl` writes to either
//! stream goes through the macros here instead ([`out!`], [`outln!`],
//! [`err!`], [`errln!`]), which write to the locked stream and handle a
//! failure in one place:
//!
//! - A closed stream (`ErrorKind::BrokenPipe`) is not an error. Whoever
//!   was reading has stopped, so `wcl` stops too: it flushes what it can
//!   and exits [`EXIT_OK`]. The same on every platform — `wcl` never
//!   dies by `SIGPIPE`, so a pipeline never sees 141.
//! - Any other write failure (a full disk behind `> file`) exits
//!   [`EXIT_IO`], after trying to say why on stderr.
//!
//! Exiting skips destructors. A caller holding something whose `Drop`
//! matters — a temp dir — releases it before writing.
//!
//! `wcl lsp` does not come through here: its protocol stream is tokio's
//! stdout, owned by `wcl_lsp`.

use std::fmt;
use std::io::{self, Write};

use crate::{EXIT_IO, EXIT_OK};

/// Like `print!`, but a closed stdout ends the process quietly instead of
/// panicking. See the [module docs](self).
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::out::write_stdout(format_args!($($arg)*))
    };
}

/// Like `println!`, but a closed stdout ends the process quietly instead
/// of panicking. See the [module docs](self).
macro_rules! outln {
    () => {
        $crate::out::write_stdout(format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::out::write_stdout(format_args!("{}\n", format_args!($($arg)*)))
    };
}

/// Like `eprint!`, but a closed stderr ends the process quietly instead of
/// panicking. See the [module docs](self).
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::out::write_stderr(format_args!($($arg)*))
    };
}

/// Like `eprintln!`, but a closed stderr ends the process quietly instead
/// of panicking. See the [module docs](self).
macro_rules! errln {
    () => {
        $crate::out::write_stderr(format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::out::write_stderr(format_args!("{}\n", format_args!($($arg)*)))
    };
}

pub(crate) use {err, errln, out, outln};

/// Write `args` to the locked stdout. Called through [`out!`] and
/// [`outln!`].
pub(crate) fn write_stdout(args: fmt::Arguments<'_>) {
    if let Err(e) = io::stdout().lock().write_fmt(args) {
        stop(&e);
    }
}

/// Write `args` to the locked stderr. Called through [`err!`] and
/// [`errln!`].
pub(crate) fn write_stderr(args: fmt::Arguments<'_>) {
    if let Err(e) = io::stderr().lock().write_fmt(args) {
        stop(&e);
    }
}

/// Flush stdout, for a prompt written without a trailing newline.
pub(crate) fn flush_stdout() {
    if let Err(e) = io::stdout().lock().flush() {
        stop(&e);
    }
}

/// Flush stderr, for a prompt written without a trailing newline.
pub(crate) fn flush_stderr() {
    if let Err(e) = io::stderr().lock().flush() {
        stop(&e);
    }
}

/// End the process after a failed write: [`EXIT_OK`] for a closed stream,
/// [`EXIT_IO`] for anything else.
fn stop(e: &io::Error) -> ! {
    if e.kind() == io::ErrorKind::BrokenPipe {
        // Push out whatever either stream still buffers. One of them is
        // the closed one, so their results say nothing worth acting on.
        let _ = io::stdout().lock().flush();
        let _ = io::stderr().lock().flush();
        std::process::exit(EXIT_OK.into());
    }
    // stderr may be the stream that failed; there is nowhere else to say it.
    let _ = writeln!(io::stderr().lock(), "error: cannot write output: {e}");
    std::process::exit(EXIT_IO.into());
}
