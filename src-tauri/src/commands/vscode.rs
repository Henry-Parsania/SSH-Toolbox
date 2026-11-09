//! Finds the colour themes that VS Code has on this computer, so they can be imported.
//!
//! Only two things are offered: the list of themes, and reading a theme file that lies inside one
//! of VS Code's own folders. Turning a theme into app colours happens in the page.

use std::path::{Path, PathBuf};

use serde::Serialize;
use toolbox_core::{AppError, ErrorCode};

#[derive(Serialize)]
pub struct VsCodeTheme {
    pub label: String,
    /// `vs`, `vs-dark`, `hc-black` or `hc-light`.
    pub ui_theme: String,
    /// Full path of the theme file; pass it back to `vscode_theme_read`.
    pub path: String,
    /// The extension it comes from, or "VS Code" for the built-in ones.
    pub source: String,
}

/// Where an install of VS Code keeps its built-in extensions. Newer versions put everything one
/// folder down, under a folder named after the build.
fn install_extensions(base: PathBuf, out: &mut Vec<PathBuf>) {
    out.push(base.join("resources/app/extensions"));
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            out.push(entry.path().join("resources/app/extensions"));
        }
    }
}

/// The folders that hold themes: the user's installed extensions and VS Code's built-in ones.
fn roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dirs) = directories::BaseDirs::new() {
        let home = dirs.home_dir();
        for name in [".vscode", ".vscode-insiders", ".vscode-oss"] {
            out.push(home.join(name).join("extensions"));
        }
        #[cfg(windows)]
        {
            let local = dirs.data_local_dir();
            for name in ["Microsoft VS Code", "Microsoft VS Code Insiders"] {
                install_extensions(local.join("Programs").join(name), &mut out);
            }
        }
    }
    #[cfg(windows)]
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(var) {
            install_extensions(PathBuf::from(base).join("Microsoft VS Code"), &mut out);
        }
    }
    #[cfg(target_os = "linux")]
    for base in [
        "/usr/share/code",
        "/usr/lib/code",
        "/opt/visual-studio-code",
        "/snap/code/current/usr/share/code",
        "/var/lib/flatpak/app/com.visualstudio.code/current/active/files/extra/vscode",
    ] {
        install_extensions(PathBuf::from(base), &mut out);
    }
    #[cfg(target_os = "macos")]
    out.push(PathBuf::from(
        "/Applications/Visual Studio Code.app/Contents/Resources/app/extensions",
    ));
    out.into_iter().filter(|p| p.is_dir()).collect()
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

/// Names like `%displayName%` are looked up in the extension's `package.nls.json`.
fn localized(dir: &Path, text: &str) -> String {
    if let Some(key) = text.strip_prefix('%').and_then(|t| t.strip_suffix('%')) {
        if let Some(nls) = read_json(&dir.join("package.nls.json")) {
            match nls.get(key) {
                Some(serde_json::Value::String(s)) => return s.clone(),
                Some(v) => {
                    if let Some(s) = v.get("message").and_then(|m| m.as_str()) {
                        return s.to_string();
                    }
                }
                None => {}
            }
        }
        return key.to_string();
    }
    text.to_string()
}

fn scan() -> Vec<VsCodeTheme> {
    let mut out = Vec::new();
    for root in roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            let Some(package) = read_json(&dir.join("package.json")) else {
                continue;
            };
            let Some(themes) = package
                .pointer("/contributes/themes")
                .and_then(|t| t.as_array())
            else {
                continue;
            };
            let source = package
                .get("displayName")
                .and_then(|n| n.as_str())
                .map(|n| localized(&dir, n))
                .unwrap_or_else(|| "VS Code".to_string());
            for theme in themes {
                let Some(rel) = theme.get("path").and_then(|p| p.as_str()) else {
                    continue;
                };
                let file = dir.join(rel);
                if !file.is_file() {
                    continue;
                }
                let label = theme
                    .get("label")
                    .or_else(|| theme.get("id"))
                    .and_then(|l| l.as_str())
                    .map(|l| localized(&dir, l))
                    .unwrap_or_else(|| rel.to_string());
                out.push(VsCodeTheme {
                    label,
                    ui_theme: theme
                        .get("uiTheme")
                        .and_then(|u| u.as_str())
                        .unwrap_or("vs-dark")
                        .to_string(),
                    path: file.to_string_lossy().into_owned(),
                    source: source.clone(),
                });
            }
        }
    }
    out.sort_by_key(|a| a.label.to_lowercase());
    out
}

/// The theme the person has chosen in VS Code's own settings, if it can be found. Settings files
/// allow comments, so this only looks for the one line it needs.
fn chosen_name() -> Option<String> {
    let dirs = directories::BaseDirs::new()?;
    let file = dirs.config_dir().join("Code/User/settings.json");
    let text = std::fs::read_to_string(file).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("\"workbench.colorTheme\"") {
            let value = rest.trim_start().strip_prefix(':')?.trim();
            let value = value.trim_end_matches(',').trim();
            return serde_json::from_str::<String>(value).ok();
        }
    }
    None
}

#[derive(Serialize)]
pub struct VsCodeThemes {
    pub themes: Vec<VsCodeTheme>,
    /// The name of the theme VS Code is using, when it could be read.
    pub current: Option<String>,
}

#[tauri::command]
pub async fn vscode_themes() -> Result<VsCodeThemes, AppError> {
    tokio::task::spawn_blocking(|| VsCodeThemes {
        themes: scan(),
        current: chosen_name(),
    })
    .await
    .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't look for VS Code themes."))
}

/// Reads a theme file, but only one that lies inside a VS Code folder.
#[tauri::command]
pub async fn vscode_theme_read(path: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let wanted = std::fs::canonicalize(&path).map_err(|_| {
            AppError::new(ErrorCode::NotFound, "That theme file isn't there any more.")
        })?;
        let allowed = roots()
            .iter()
            .filter_map(|r| std::fs::canonicalize(r).ok())
            .any(|r| wanted.starts_with(r));
        if !allowed
            || wanted
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| !e.eq_ignore_ascii_case("json") && !e.eq_ignore_ascii_case("jsonc"))
        {
            return Err(AppError::new(
                ErrorCode::PermissionDenied,
                "That isn't a VS Code theme file.",
            ));
        }
        let meta = std::fs::metadata(&wanted).map_err(|_| {
            AppError::new(ErrorCode::NotFound, "That theme file isn't there any more.")
        })?;
        if meta.len() > 4 * 1024 * 1024 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "That theme file is too big.",
            ));
        }
        std::fs::read_to_string(&wanted)
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read that theme file."))
    })
    .await
    .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read that theme file."))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_themes_and_refuses_files_outside_vscode_folders() {
        let found = scan();
        eprintln!("{} themes, current: {:?}", found.len(), chosen_name());
        for t in found.iter().take(3) {
            eprintln!("{} | {} | {} | {}", t.label, t.ui_theme, t.source, t.path);
        }
        // Whatever is installed, every listed file exists and can be read through the guard.
        for t in found.iter().step_by(25) {
            let text = tauri::async_runtime::block_on(vscode_theme_read(t.path.clone())).unwrap();
            assert!(text.contains("colors"));
        }
        let outside = std::env::current_exe().unwrap().with_extension("json");
        assert!(
            tauri::async_runtime::block_on(vscode_theme_read(
                outside.to_string_lossy().into_owned()
            ))
            .is_err()
        );
        assert!(
            tauri::async_runtime::block_on(vscode_theme_read("C:/Windows/win.ini".into())).is_err()
        );
    }
}
