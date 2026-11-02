//! The network and port scanners. A scan runs until it finishes or is stopped, telling the screen
//! about each host as it goes.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::State;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use toolbox_core::AppError;
use toolbox_core::scan::engine::{self, ScanEvent, ScanRequest, Summary};

#[derive(Default)]
pub struct Scans {
    running: Mutex<HashMap<String, CancellationToken>>,
}

/// The networks this computer is on, to suggest what to scan.
#[tauri::command]
pub fn scan_local_networks() -> Vec<String> {
    engine::local_networks()
}

/// Runs a scan and returns when it is over. `on_event` hears about each host and the progress.
#[tauri::command]
pub async fn scan_run(
    scans: State<'_, Scans>,
    scan_id: String,
    request: ScanRequest,
    on_event: Channel<ScanEvent>,
) -> Result<Summary, AppError> {
    let cancel = CancellationToken::new();
    scans
        .running
        .lock()
        .unwrap()
        .insert(scan_id.clone(), cancel.clone());
    let result = engine::run(&request, cancel, move |event| {
        let _ = on_event.send(event);
    })
    .await;
    scans.running.lock().unwrap().remove(&scan_id);
    result
}

#[tauri::command]
pub fn scan_cancel(scans: State<'_, Scans>, scan_id: String) {
    if let Some(token) = scans.running.lock().unwrap().get(&scan_id) {
        token.cancel();
    }
}
