//! WCL — Wil's Configuration Language (CLI crate)
//!
//! This crate provides the CLI binary and re-exports the language library
//! from `wcl_lang` for backward compatibility.
#![allow(clippy::result_large_err)]

// Re-export the entire language library
pub use wcl_lang::*;

// CLI-only modules
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
