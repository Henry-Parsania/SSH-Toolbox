mod commands;

use tauri::{Manager, RunEvent};

/// Sets the web view workaround the user chose, if any. This must run before any threads or the
/// web view exist, so it is the first thing `run` does. Linux only.
#[cfg(target_os = "linux")]
fn apply_webkit_workaround() {
    let Some(dir) = toolbox_core::config::config_dir() else {
        return;
    };
    let settings = toolbox_core::settings::load(&dir.join("settings.toml"));
    if let Some((name, value)) = settings.webkit_env() {
        // SAFETY: nothing else is running yet, so no other thread can read the environment.
        unsafe { std::env::set_var(name, value) };
    }
}

#[cfg(not(target_os = "linux"))]
fn apply_webkit_workaround() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    apply_webkit_workaround();

    let app = tauri::Builder::default()
        .manage(commands::startup::Startup::now())
        .manage(commands::terminal::Terminals::default())
        .invoke_handler(tauri::generate_handler![
            commands::platform::platform_info,
            commands::layout::layout_get,
            commands::layout::layout_set,
            commands::link::open_link,
            commands::sessions::sessions_get,
            commands::sessions::sessions_save,
            commands::sessions::sessions_set_aside_unreadable,
            commands::settings::settings_get,
            commands::startup::startup_ready,
            commands::settings::settings_set,
            commands::terminal::list_shells,
            commands::terminal::list_wsl_distros,
            commands::terminal::terminal_open,
            commands::terminal::terminal_write,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_ack,
            commands::terminal::terminal_close,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        // Close every shell when the app quits, so none are left running.
        if let RunEvent::Exit = event {
            let terminals = handle.state::<commands::terminal::Terminals>();
            tauri::async_runtime::block_on(terminals.sessions.close_all());
        }
    });
}
