//! Transform error types.

use std::fmt;

/// Errors that can occur during transform execution.
#[derive(Debug)]
pub enum TransformError {
    /// I/O error during read or write.
    Io(std::io::Error),
    /// Codec error (parse or emit failure).
    Codec(String),
    /// Expression evaluation error during field mapping.
    Eval(String),
    /// Schema/type mismatch.
    TypeMismatch { expected: String, got: String },
    /// Missing required field in output.
    MissingField(String),
    /// Unknown codec name.
    UnknownCodec(String),
    /// Generic error.
    Other(String),
}

impl fmt::Display for TransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformError::Io(e) => write!(f, "I/O error: {}", e),
            TransformError::Codec(msg) => write!(f, "codec error: {}", msg),
            TransformError::Eval(msg) => write!(f, "evaluation error: {}", msg),
            TransformError::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {}, got {}", expected, got)
            }
            TransformError::MissingField(name) => write!(f, "missing required field: {}", name),
            TransformError::UnknownCodec(name) => write!(f, "unknown codec: {}", name),
            TransformError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for TransformError {}

impl From<std::io::Error> for TransformError {
    fn from(e: std::io::Error) -> Self {
        TransformError::Io(e)
    }
}
