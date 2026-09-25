//! The per-request analysis context: what every handler needs beyond
//! its own arguments.

use crate::convert::{LineIndex, PositionEncoding};

/// Settings and snapshots shared by one request's analysis.
pub(crate) struct Ctx {
    /// Unit LSP `character` values count in, negotiated at `initialize`.
    pub encoding: PositionEncoding,
}

impl Ctx {
    /// A context converting positions in `encoding`.
    pub(crate) fn new(encoding: PositionEncoding) -> Self {
        Self { encoding }
    }

    /// Index `text` for span ↔ position conversion in this context's
    /// encoding.
    pub(crate) fn index<'a>(&self, text: &'a str) -> LineIndex<'a> {
        LineIndex::new(text, self.encoding)
    }
}
