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

/// Largest backup file that will be read.
const MAX_BACKUP: u64 = 5 * 1024 * 1024;

/// Writes the saved sessions to a new file in Downloads and returns where it went. The file has
/// no passwords or key contents: the sessions file never holds any (keys are only named by path).
#[tauri::command]
pub async fn sessions_export() -> Result<String, AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let loaded = store.load();
        if let Some(problem) = loaded.problem {
            return Err(AppError::new(ErrorCode::InvalidInput, problem));
        }
        let text = loaded.file.to_toml()?;
        let dirs = directories::UserDirs::new().ok_or_else(|| {
            AppError::new(ErrorCode::NotFound, "Couldn't find your Downloads folder.")
        })?;
        let folder = dirs
            .download_dir()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| dirs.home_dir().to_path_buf());
        let stamp = toolbox_core::terminal::log::utc_stamp();
        for attempt in 0..100u32 {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let path = folder.join(format!("ssh-toolbox-sessions-{stamp}{suffix}.toml"));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    file.write_all(text.as_bytes()).map_err(|_| {
                        AppError::new(ErrorCode::Internal, "Couldn't write the backup file.")
                    })?;
                    return Ok(path.display().to_string());
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        "Couldn't write the backup file.",
                    ));
                }
            }
        }
        Err(AppError::new(
            ErrorCode::AlreadyExists,
            "Couldn't find a free file name for the backup.",
        ))
    })
    .await
    .map_err(|_| task_failed())?
}

/// Adds the sessions of a backup file to the saved ones. What is already here is kept as it is.
#[tauri::command]
pub async fn sessions_import(
    path: String,
) -> Result<toolbox_core::sessions::MergeResult, AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || {
        let path = std::path::PathBuf::from(path.trim().trim_matches('"'));
        let meta = std::fs::metadata(&path)
            .map_err(|_| AppError::new(ErrorCode::NotFound, "That file couldn't be found."))?;
        if !meta.is_file() || meta.len() > MAX_BACKUP {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "That isn't a backup file of sessions.",
            ));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|_| AppError::new(ErrorCode::InvalidInput, "That file couldn't be read."))?;
        let backup = SessionsFile::from_toml(&text)?;
        let loaded = store.load();
        if let Some(problem) = loaded.problem {
            return Err(AppError::new(ErrorCode::InvalidInput, problem));
        }
        let mut file = loaded.file;
        let result = file.merge(backup);
        store.save(&file)?;
        Ok(result)
    })
    .await
    .map_err(|_| task_failed())?
}
