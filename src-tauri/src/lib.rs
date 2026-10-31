mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::platform::platform_info])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
