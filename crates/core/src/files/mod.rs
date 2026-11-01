//! Files on this computer and on servers, behind one interface (INTERFACES.md section 6).

mod backend;
mod local;
#[cfg(test)]
pub(crate) mod memory;
mod names;
mod queue;
#[cfg(test)]
mod queue_tests;

pub use backend::{COPY_BUFFER, FileBackend, FileCapabilities, FileEntry, FileKind, Progress};
pub use local::LocalFiles;
pub use names::check_remote_name;
pub use queue::{
    CopyRequest, DeletePlan, JobKind, JobSnapshot, JobState, OverwriteChoice, OverwriteResolver,
    PART_SUFFIX, TransferQueue, TreeOp, TreePlan, plan_delete, plan_tree, queue_tree, run_delete,
};
