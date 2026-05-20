//! Wasm lowering for Program IR v2.

mod lower;

pub use lower::{lower_module, LowerError};
