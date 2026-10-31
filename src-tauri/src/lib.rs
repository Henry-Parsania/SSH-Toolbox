mod commands;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, RunEvent};
use toolbox_core::keychain::Keychain;
use toolbox_core::prompt::PromptBroker;
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::ssh::hostkeys::default_known_hosts;
use toolbox_core::ssh::pool::ConnectionPool;

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

    let keychain = Arc::new(Keychain::system());
    let broker = PromptBroker::new();
    let config = toolbox_core::config::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let services = Arc::new(SshServices {
        keychain: keychain.clone(),
        broker: broker.clone(),
        pool: ConnectionPool::new(),
        sessions_file: config.join("sessions.toml"),
        known_hosts: default_known_hosts().unwrap_or_else(|| config.join("known_hosts")),
    });

    let app = tauri::Builder::default()
        .manage(commands::startup::Startup::now())
        .manage(keychain)
        .manage(broker)
        .manage(services)
        .manage(commands::keys::KeySetups::default())
        .manage(toolbox_core::ssh::forward::Forwards::new())
        .manage(commands::terminal::Terminals::default())
        .invoke_handler(tauri::generate_handler![
            commands::platform::platform_info,
            commands::forward::forward_list,
            commands::forward::forward_start,
            commands::forward::forward_stop,
            commands::forward::forward_stop_session,
            commands::keychain::keychain_available,
            commands::keychain::keychain_delete_session,
            commands::keychain::keychain_set,
            commands::keys::key_generate,
            commands::keys::key_list,
            commands::keys::key_setup,
            commands::keys::key_setup_cancel,
            commands::keychain::keychain_delete_secret,
            commands::layout::layout_get,
            commands::layout::layout_set,
            commands::link::open_link,
            commands::prompt::prompt_respond,
            commands::sessions::sessions_get,
            commands::sessions::ssh_config_import,
            commands::sessions::ssh_config_preview,
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
