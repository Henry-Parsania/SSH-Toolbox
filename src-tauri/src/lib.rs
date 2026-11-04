mod commands;
mod tray;
mod window_state;

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

/// (tray icon on, closing hides to the tray), from the saved settings.
fn tray_wanted() -> (bool, bool) {
    let Some(dir) = toolbox_core::config::config_dir() else {
        return (false, false);
    };
    let settings = toolbox_core::settings::load(&dir.join("settings.toml"));
    (
        settings.tray_icon,
        settings.tray_icon && settings.close_to_tray,
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    apply_webkit_workaround();

    let keychain = Arc::new(Keychain::system());
    let broker = PromptBroker::new();
    let config = toolbox_core::config::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let x11 = Arc::new(toolbox_core::x11::X11Local::new(config.join("x11")));
    let services = Arc::new(SshServices {
        keychain: keychain.clone(),
        broker: broker.clone(),
        pool: ConnectionPool::new(),
        sessions_file: config.join("sessions.toml"),
        known_hosts: default_known_hosts().unwrap_or_else(|| config.join("known_hosts")),
        x11: x11.clone(),
    });

    let app = tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                // Closing the main window ends the app, and with it any other windows.
                let handle = app.handle().clone();
                let hide_target = window.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::Destroyed => handle.exit(0),
                    // With the tray icon and "keep running" on, closing only hides the window.
                    tauri::WindowEvent::CloseRequested { api, .. } if tray_wanted().1 => {
                        api.prevent_close();
                        let _ = hide_target.hide();
                    }
                    _ => {}
                });
                // The title names the exact build, so it is clear which one is running.
                let _ = window.set_title(&format!(
                    "SSH Toolbox {}",
                    commands::platform::version_line()
                ));
                // The window starts hidden (tauri.conf.json) so it appears once, already at its
                // saved size and position, instead of opening and then jumping.
                if let Some(dir) = toolbox_core::config::config_dir() {
                    window_state::install(window.clone(), dir.join("window.json"));
                }
                let _ = window.show();
            }
            if tray_wanted().0 {
                tray::install(app)?;
            }
            Ok(())
        })
        .manage(x11)
        .manage(commands::startup::Startup::now())
        .manage(keychain)
        .manage(broker)
        .manage(services)
        .manage(commands::keys::KeySetups::default())
        .manage(commands::files::Files::default())
        .manage(commands::status::StatsState::default())
        .manage(commands::docker::Engines::default())
        .manage(commands::scan::Scans::default())
        .manage(commands::edit::Edits::new())
        .manage(commands::agent::Agent::new(&config.join("providers.json")))
        .manage(toolbox_core::ssh::forward::Forwards::new())
        .manage(commands::terminal::Terminals::default())
        .invoke_handler(tauri::generate_handler![
            commands::platform::platform_info,
            commands::platform::app_version,
            commands::files::files_read_text,
            commands::files::files_write_text,
            commands::edit::edit_open,
            commands::edit::edit_upload,
            commands::edit::edit_reopen,
            commands::edit::edit_stop,
            commands::edit::open_local_file,
            commands::x11::x11_status,
            commands::x11::x11_start,
            commands::x11::x11_stop,
            commands::scan::scan_local_networks,
            commands::scan::scan_run,
            commands::scan::scan_cancel,
            commands::agent::agent_providers,
            commands::agent::agent_provider_save,
            commands::agent::agent_provider_delete,
            commands::agent::agent_complete,
            commands::agent::agent_cancel,
            commands::agent::agent_classify,
            commands::agent::agent_redact,
            commands::agent::agent_exec,
            commands::agent::agent_read_file,
            commands::agent::agent_write_file,
            commands::docker::docker_action,
            commands::docker::docker_close,
            commands::docker::docker_connect,
            commands::docker::docker_inspect,
            commands::docker::docker_list,
            commands::docker::docker_remove_resource,
            commands::docker::docker_rename,
            commands::docker::docker_resource_details,
            commands::docker::docker_prune,
            commands::docker::docker_pull,
            commands::docker::docker_run,
            commands::docker::docker_create_resource,
            commands::docker::docker_resources,
            commands::docker::docker_stats,
            commands::terminal::serial_list_ports,
            commands::terminal::serial_send_break,
            commands::terminal::serial_set_control,
            commands::status::reach_check,
            commands::status::stats_local,
            commands::status::stats_remote,
            commands::files::files_chmod,
            commands::files::files_close,
            commands::files::files_delete,
            commands::files::files_delete_as_root,
            commands::files::files_delete_plan,
            commands::files::files_list,
            commands::files::files_mkdir,
            commands::files::files_open_ftp,
            commands::files::files_open_local,
            commands::files::files_open_sftp,
            commands::files::files_open_wsl,
            commands::files::files_rename,
            commands::files::transfer_add,
            commands::files::transfer_answer,
            commands::files::transfer_cancel,
            commands::files::transfer_cancel_all,
            commands::files::transfer_clear,
            commands::files::transfers_subscribe,
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
            commands::snippets::snippets_get,
            commands::snippets::snippets_set,
            commands::link::open_link,
            commands::prompt::prompt_respond,
            commands::sessions::sessions_get,
            commands::sessions::ssh_config_import,
            commands::sessions::ssh_config_preview,
            commands::sessions::sessions_save,
            commands::keys::known_hosts_list,
            commands::keys::known_hosts_remove,
            commands::sessions::sessions_export,
            commands::sessions::sessions_import,
            commands::sessions::sessions_set_aside_unreadable,
            commands::settings::settings_get,
            commands::startup::startup_ready,
            commands::settings::settings_set,
            commands::terminal::list_shells,
            commands::terminal::list_wsl_distros,
            commands::terminal::terminal_open,
            commands::terminal::terminal_write,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_save_output,
            commands::terminal::terminal_ack,
            commands::terminal::terminal_attach,
            commands::window::window_open,
            commands::terminal::terminal_close,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        // Close every shell when the app quits, so none are left running.
        if let RunEvent::Exit = event {
            let terminals = handle.state::<commands::terminal::Terminals>();
            tauri::async_runtime::block_on(terminals.sessions.close_all());
            // An X server the app started goes with it.
            handle
                .state::<Arc<toolbox_core::x11::X11Local>>()
                .stop_now();
            // Local copies of server files being edited are deleted.
            handle.state::<commands::edit::Edits>().0.stop_all();
        }
    });
}
