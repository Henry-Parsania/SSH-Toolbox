//! SSH Toolbox backend logic. This crate has no Tauri dependency.

pub mod config;
pub mod error;
pub mod files;
pub mod keychain;
pub mod layout;
pub mod link;
pub mod platform;
pub mod prompt;
pub mod secret;
pub mod sessions;
pub mod settings;
pub mod ssh;
pub mod terminal;

pub use error::{AppError, ErrorCode};
pub use platform::PlatformInfo;
pub use secret::Secret;
