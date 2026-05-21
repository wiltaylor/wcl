//! WCL language library. Parses a WCL source string into a [`Document`].

mod ast;
mod error;
mod lexer;
mod parser;

pub use ast::{Block, Document, Field, Item, Span, Value};
pub use error::ParseError;

use std::path::Path;

/// Parse a WCL source string with an anonymous filename in diagnostics.
pub fn parse(source: &str) -> Result<Document, ParseError> {
    parse_named(source, "<input>")
}

/// Parse a WCL source string, attaching `file` to any diagnostics produced.
pub fn parse_named(source: &str, file: &str) -> Result<Document, ParseError> {
    parser::Parser::new(source, file).parse_document()
}

/// Read `path` from disk and parse it.
pub fn parse_file(path: &Path) -> Result<Document, ParseError> {
    let source = std::fs::read_to_string(path)?;
    parse_named(&source, &path.display().to_string())
}
