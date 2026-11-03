use std::path::Path;

use tauri::State;
use tauri::ipc::Channel;
use toolbox_core::AppError;
use toolbox_core::files::{
    EditEvent, EditInfo, EditSessions, UploadResult, is_risky_to_open, open_default,
    open_with_dialog,
};

use super::files::Files;

/// The server files being edited on this computer.
pub struct Edits(pub EditSessions);

impl Edits {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join("ssh-toolbox-edit");
        let edits = EditSessions::new(root);
        // Copies left by a run that didn't end normally.
        edits.clean_leftovers();
        Self(edits)
    }
}

/// Opens a file in the program the computer uses for it. Programs and scripts, which opening
/// could run, are opened by asking which program to use instead.
fn open_safely(path: &Path, choose: bool) -> Result<(), AppError> {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    if choose || is_risky_to_open(&name) {
        open_with_dialog(path)
    } else {
        open_default(path)
    }
}

/// Downloads a server file, opens it here, and starts watching it for saves.
#[tauri::command]
pub async fn edit_open(
    files: State<'_, Files>,
    edits: State<'_, Edits>,
    file_id: String,
    path: String,
    choose: bool,
    on_event: Channel<EditEvent>,
) -> Result<EditInfo, AppError> {
    let backend = files.backend(&file_id)?;
    let info = edits
        .0
        .open(backend, &path, move |event| {
            let _ = on_event.send(event);
        })
        .await?;
    if let Err(error) = open_safely(Path::new(&info.local_path), choose) {
        edits.0.stop(&info.id);
        return Err(error);
    }
    Ok(info)
}

/// Sends the edited copy back to the server.
#[tauri::command]
pub async fn edit_upload(
    edits: State<'_, Edits>,
    id: String,
    force: bool,
) -> Result<UploadResult, AppError> {
    edits.0.upload(&id, force).await
}

/// Opens the local copy again, or asks which program to open it with.
#[tauri::command]
pub async fn edit_reopen(edits: State<'_, Edits>, id: String, choose: bool) -> Result<(), AppError> {
    let path = edits.0.local_path(&id)?;
    open_safely(&path, choose)
}

/// Stops watching and deletes the local copy.
#[tauri::command]
pub fn edit_stop(edits: State<'_, Edits>, id: String) {
    edits.0.stop(&id);
}

/// Opens a file on this computer in its usual program (the same safety rule as above).
#[tauri::command]
pub fn open_local_file(path: String, choose: bool) -> Result<(), AppError> {
    open_safely(Path::new(&path), choose)
}
