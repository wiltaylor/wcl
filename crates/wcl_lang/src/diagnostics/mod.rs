//! What the language reports *about* a run, as opposed to what it
//! computes from one.
//!
//! Two kinds of report, split by when they can happen:
//!
//! - [`parse`] — the source never became a tree. Returned by
//!   [`Document::open`](crate::Document::open) and
//!   [`parse_for_edit`](crate::parse_for_edit).
//! - [`eval`] / [`kinds`] — the document exists and a read of it failed.
//!   One error type, with the distinctions a tool branches on carried as
//!   structured kinds rather than as prose it would have to parse.
//!
//! And one report that is not a failure at all:
//!
//! - [`profile`](self::profile) — the opt-in evaluation profiler: where
//!   a document spent its time, as a tree a host can render. Nothing in
//!   it is consulted by evaluation — a document opened without the
//!   `*_profiled` constructors carries none of it, and every hook costs
//!   one `Option::is_some` check.
//!
//! Rendering is `miette`'s: each error type derives `Diagnostic`, so a
//! host prints it with the offending span underlined and this crate
//! holds no formatting of its own.

mod eval;
mod kinds;
mod parse;
mod profile;

pub use eval::EvalError;
pub use kinds::{ArithmeticFault, SchemaViolationKind};
pub use parse::{ParseError, SyntaxError};
pub use profile::{Profile, ProfileKey, ProfileNode};
pub(crate) use profile::{ProfileGuard, ProfileState};
