//! SSH Toolbox backend logic. This crate has no Tauri dependency.

pub mod error;
pub mod secret;

pub use error::{AppError, ErrorCode};
pub use secret::Secret;
