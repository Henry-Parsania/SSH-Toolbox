mod commands;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(commands::terminal::Terminals::default())
        .invoke_handler(tauri::generate_handler![
            commands::platform::platform_info,
            commands::terminal::list_shells,
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
