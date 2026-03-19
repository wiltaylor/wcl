use std::fmt;
use serde::{de, ser};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("expected {expected}, got {got}")]
    TypeMismatch { expected: String, got: String },
    #[error("missing field: {0}")]
    MissingField(String),
    #[error("unknown field: {0}")]
    UnknownField(String),
    #[error("parse error: {0}")]
    Parse(String),
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}
