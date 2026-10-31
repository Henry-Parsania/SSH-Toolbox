use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use tauri::State;
use toolbox_core::config::config_dir;

/// When the app began, for measuring Q3 (time from launch to a usable window).
pub struct Startup {
    began: Instant,
    reported: AtomicBool,
}

impl Startup {
    pub fn now() -> Self {
        Self {
            began: Instant::now(),
            reported: AtomicBool::new(false),
        }
    }
}

/// Called once, when the first terminal shows its first output. The time is printed and added
/// to `startup.log` in the config folder, one line per launch.
#[tauri::command]
pub async fn startup_ready(startup: State<'_, Startup>) -> Result<(), ()> {
    if startup.reported.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let millis = startup.began.elapsed().as_millis();
    eprintln!("startup: usable after {millis} ms");
    if let Some(dir) = config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("startup.log"))
        {
            let _ = writeln!(file, "usable after {millis} ms");
        }
    }
    Ok(())
}
