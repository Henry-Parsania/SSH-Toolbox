//! Files on this computer and on servers, behind one interface (INTERFACES.md section 6).

mod answers;
mod backend;
mod edit;
mod open;
mod ftp;
#[cfg(test)]
mod ftp_tests;
mod local;
#[cfg(test)]
pub(crate) mod memory;
mod names;
mod queue;
#[cfg(test)]
mod queue_tests;
mod sftp;
#[cfg(test)]
mod sftp_tests;
mod sudo;

pub use answers::PendingAnswers;
pub use edit::{EditEvent, EditInfo, EditSessions, MAX_EDIT_BYTES, UploadResult, is_risky_to_open};
pub use open::{open_default, open_with_dialog};
pub use backend::{COPY_BUFFER, FileBackend, FileCapabilities, FileEntry, FileKind, Progress};
pub use ftp::{FtpFiles, FtpOpened, FtpTarget};
pub use local::LocalFiles;
pub use names::check_remote_name;
pub use queue::{
    CopyRequest, DeletePlan, JobKind, JobSnapshot, JobState, OverwriteChoice, OverwriteResolver,
    PART_SUFFIX, TransferQueue, TreeOp, TreePlan, plan_delete, plan_tree, queue_tree, run_delete,
};
pub use sftp::SftpFiles;
