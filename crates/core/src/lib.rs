//! SSH Toolbox backend logic. This crate has no Tauri dependency.

pub mod config;
pub mod error;
pub mod platform;
pub mod secret;
pub mod settings;
pub mod terminal;

pub use error::{AppError, ErrorCode};
pub use platform::PlatformInfo;
pub use secret::Secret;
