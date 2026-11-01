//! Remembers the main window's size, position and whether it was maximized, and puts it back at
//! the next start. The file holds nothing but those numbers.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};

const MIN_WIDTH: u32 = 400;
const MIN_HEIGHT: u32 = 300;
const MAX_SIDE: u32 = 20_000;
/// How long after the last change the state is written, so dragging doesn't write constantly.
const SAVE_AFTER: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// A screen: its top left corner and size, in pixels.
pub type Screen = (i32, i32, u32, u32);

impl WindowState {
    fn sensible(&self) -> bool {
        (MIN_WIDTH..=MAX_SIDE).contains(&self.width)
            && (MIN_HEIGHT..=MAX_SIDE).contains(&self.height)
    }

    /// Whether enough of the title bar is on some screen to grab it. A window saved on a monitor
    /// that has since been unplugged must not come back out of reach.
    pub fn reachable(&self, screens: &[Screen]) -> bool {
        const NEEDED_WIDTH: i64 = 120;
        const NEEDED_HEIGHT: i64 = 40;
        screens.iter().any(|&(sx, sy, sw, sh)| {
            let left = i64::from(self.x.max(sx));
            let right =
                (i64::from(self.x) + i64::from(self.width)).min(i64::from(sx) + i64::from(sw));
            let top = i64::from(self.y.max(sy));
            let bottom = (i64::from(self.y) + NEEDED_HEIGHT).min(i64::from(sy) + i64::from(sh));
            right - left >= NEEDED_WIDTH && bottom - top >= NEEDED_HEIGHT
        })
    }
}

pub fn load(path: &Path) -> Option<WindowState> {
    let state: WindowState = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    state.sensible().then_some(state)
}

pub fn save(path: &Path, state: &WindowState) {
    let Ok(text) = serde_json::to_vec_pretty(state) else {
        return;
    };
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, text).is_ok() && std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
}

fn screens(window: &WebviewWindow) -> Vec<Screen> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            (
                m.position().x,
                m.position().y,
                m.size().width,
                m.size().height,
            )
        })
        .collect()
}

/// The window's current normal (not maximized) position and size.
fn capture(window: &WebviewWindow, maximized: bool) -> Option<WindowState> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(WindowState {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized,
    })
}

/// Restores the saved window state, and keeps saving it as the window changes.
pub fn install(window: WebviewWindow, file: PathBuf) {
    let mut current = capture(&window, false);
    if let Some(saved) = load(&file) {
        let _ = window.set_size(PhysicalSize::new(saved.width, saved.height));
        if saved.reachable(&screens(&window)) {
            let _ = window.set_position(PhysicalPosition::new(saved.x, saved.y));
        } else {
            let _ = window.center();
        }
        if saved.maximized {
            let _ = window.maximize();
        }
        current = Some(saved);
    }

    let Some(start) = current else { return };
    let state = Arc::new(Mutex::new(start));
    let generation = Arc::new(AtomicU64::new(0));
    let watched = window.clone();
    window.on_window_event(move |event| {
        let writes_now = matches!(event, WindowEvent::CloseRequested { .. });
        if !writes_now && !matches!(event, WindowEvent::Resized(_) | WindowEvent::Moved(_)) {
            return;
        }
        let maximized = watched.is_maximized().unwrap_or(false);
        {
            let mut held = state.lock().unwrap();
            held.maximized = maximized;
            // A maximized or minimized window's own numbers would replace the normal size.
            let hidden =
                watched.is_minimized().unwrap_or(false) || watched.is_fullscreen().unwrap_or(false);
            if !maximized
                && !hidden
                && let Some(now) = capture(&watched, false)
                && now.sensible()
            {
                *held = now;
            }
        }
        if writes_now {
            save(&file, &state.lock().unwrap());
            return;
        }
        let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (state, generation, file) = (state.clone(), generation.clone(), file.clone());
        std::thread::spawn(move || {
            std::thread::sleep(SAVE_AFTER);
            if generation.load(Ordering::SeqCst) == mine {
                save(&file, &state.lock().unwrap());
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> WindowState {
        WindowState {
            x,
            y,
            width: 800,
            height: 600,
            maximized: false,
        }
    }

    const SCREEN: Screen = (0, 0, 1920, 1080);

    #[test]
    fn a_window_on_a_screen_is_reachable() {
        assert!(at(100, 100).reachable(&[SCREEN]));
        assert!(
            at(-600, 50).reachable(&[SCREEN]),
            "mostly off the left edge, but 200px remain"
        );
    }

    #[test]
    fn a_window_left_on_a_missing_monitor_is_not() {
        assert!(!at(2500, 100).reachable(&[SCREEN]));
        assert!(!at(-790, 100).reachable(&[SCREEN]));
        assert!(!at(100, 1100).reachable(&[SCREEN]));
        assert!(at(2500, 100).reachable(&[SCREEN, (1920, 0, 1920, 1080)]));
        assert!(!at(0, 0).reachable(&[]));
    }

    #[test]
    fn the_state_survives_a_round_trip_and_bad_files_are_ignored() {
        let dir = std::env::temp_dir().join(format!("sshtoolbox-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("window.json");
        let state = WindowState {
            maximized: true,
            ..at(10, 20)
        };
        save(&file, &state);
        assert_eq!(load(&file), Some(state));

        std::fs::write(&file, b"not json").unwrap();
        assert_eq!(load(&file), None);
        save(
            &file,
            &WindowState {
                width: 10,
                ..at(0, 0)
            },
        );
        assert_eq!(load(&file), None, "a tiny window would be unusable");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
