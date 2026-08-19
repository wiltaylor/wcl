//! What the language reports *about* a run, as opposed to what it
//! computes from one.
//!
//! [`profile`](self::profile) is the opt-in evaluation profiler: where a document spent
//! its time, as a tree a host can render. Nothing in here is consulted
//! by evaluation itself — a document opened without the `*_profiled`
//! constructors carries none of it, and every hook costs one
//! `Option::is_some` check.

mod profile;

pub use profile::{Profile, ProfileKey, ProfileNode};
pub(crate) use profile::{ProfileGuard, ProfileState};
