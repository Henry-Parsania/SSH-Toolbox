//! Files on this computer and on servers, behind one interface (INTERFACES.md section 6).

mod backend;
mod local;
mod names;

pub use backend::{COPY_BUFFER, FileBackend, FileCapabilities, FileEntry, FileKind, Progress};
pub use local::LocalFiles;
pub use names::check_remote_name;
