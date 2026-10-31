use toolbox_core::config::config_dir;
use toolbox_core::sessions::{Loaded, SessionStore, SessionsFile};
use toolbox_core::{AppError, ErrorCode};

fn store() -> Result<SessionStore, AppError> {
    config_dir()
        .map(|dir| SessionStore::new(dir.join("sessions.toml")))
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find the settings folder."))
}

fn task_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't reach the saved sessions.")
}

/// The saved sessions, never with secrets. If the file can't be used, `problem` says so.
#[tauri::command]
pub async fn sessions_get() -> Result<Loaded, AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || store.load())
        .await
        .map_err(|_| task_failed())
}

#[tauri::command]
pub async fn sessions_save(file: SessionsFile) -> Result<(), AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || store.save(&file))
        .await
        .map_err(|_| task_failed())?
}

/// The user chose to start again: the unreadable file is kept as `sessions.toml.bad`.
#[tauri::command]
pub async fn sessions_set_aside_unreadable() -> Result<(), AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || store.set_aside_unreadable())
        .await
        .map_err(|_| task_failed())
}

fn ssh_paths() -> Result<(std::path::PathBuf, std::path::PathBuf), AppError> {
    let dir = toolbox_core::ssh::hostkeys::default_known_hosts()
        .and_then(|f| f.parent().map(|p| p.to_path_buf()))
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find your .ssh folder."))?;
    Ok((dir.join("config"), dir))
}

/// What is in `~/.ssh/config`, with the hosts already saved marked. The file is only read.
#[tauri::command]
pub async fn ssh_config_preview()
-> Result<Vec<toolbox_core::ssh::config_import::PreviewItem>, AppError> {
    let (config, dir) = ssh_paths()?;
    let store = store()?;
    tokio::task::spawn_blocking(move || {
        let hosts = toolbox_core::ssh::config_import::load_config(&config, &dir);
        toolbox_core::ssh::config_import::preview(&store.load().file, &hosts)
    })
    .await
    .map_err(|_| task_failed())
}

/// Adds the chosen hosts as saved sessions. Refuses if the saved sessions file can't be read.
#[tauri::command]
pub async fn ssh_config_import(
    aliases: Vec<String>,
) -> Result<toolbox_core::ssh::config_import::ImportResult, AppError> {
    let (config, dir) = ssh_paths()?;
    let store = store()?;
    tokio::task::spawn_blocking(move || {
        let hosts = toolbox_core::ssh::config_import::load_config(&config, &dir);
        let loaded = store.load();
        if let Some(problem) = loaded.problem {
            return Err(AppError::new(ErrorCode::InvalidInput, problem));
        }
        let mut file = loaded.file;
        let result = toolbox_core::ssh::config_import::import(&mut file, &hosts, &aliases, || {
            uuid::Uuid::new_v4().to_string()
        });
        if !result.added.is_empty() {
            store.save(&file)?;
        }
        Ok(result)
    })
    .await
    .map_err(|_| task_failed())?
}
