//! `settings.toml` (INTERFACES.md section 9).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::{set_aside, write_atomic};
use crate::error::{AppError, ErrorCode};

pub const MIN_SCROLLBACK: u32 = 10_000;
pub const MAX_SCROLLBACK: u32 = 1_000_000;
pub const MIN_FONT_SIZE: u16 = 8;
pub const MAX_FONT_SIZE: u16 = 48;
/// Seconds between reachability checks. 0 means only at start and when the list changes.
pub const MIN_CHECK_INTERVAL: u32 = 5;
pub const MAX_CHECK_INTERVAL: u32 = 3600;
pub const MIN_STATS_INTERVAL: u32 = 2;
pub const MAX_STATS_INTERVAL: u32 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Renderer {
    Auto,
    Dom,
    Webgl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebkitWorkaround {
    Off,
    DisableDmabuf,
    DisableCompositing,
}

/// Any setting missing from the file takes its default, so an older file keeps working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub font_family: String,
    pub font_size: u16,
    pub theme: Theme,
    pub scrollback: u32,
    pub renderer: Renderer,
    pub webkit_workaround: WebkitWorkaround,
    pub default_shell: String,
    pub restore_layout: bool,
    /// Selecting text in a terminal copies it at once.
    pub copy_on_select: bool,
    /// A folder to save a text copy of every terminal session in, or empty for no logging.
    pub log_folder: String,
    /// Show an icon in the system tray (takes effect after a restart).
    pub tray_icon: bool,
    /// Closing the window hides it to the tray instead of quitting. Only with the tray icon on.
    pub close_to_tray: bool,
    /// Start an SSH session again by itself when its connection drops (a few tries, then it asks).
    pub auto_reconnect: bool,
    /// Whether the saved sessions are checked for being reachable (a quick TCP connect).
    pub reachability_enabled: bool,
    /// Seconds between checks, or 0 to check only at start and when the list changes.
    pub reachability_interval_secs: u32,
    /// Whether the footer shows the load of the computer the active tab is on.
    pub stats_enabled: bool,
    pub stats_interval_secs: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            font_family: "monospace".into(),
            font_size: 14,
            theme: Theme::Dark,
            scrollback: MIN_SCROLLBACK,
            renderer: Renderer::Auto,
            webkit_workaround: WebkitWorkaround::Off,
            default_shell: String::new(),
            restore_layout: true,
            copy_on_select: true,
            log_folder: String::new(),
            tray_icon: false,
            close_to_tray: false,
            auto_reconnect: true,
            reachability_enabled: true,
            reachability_interval_secs: 60,
            stats_enabled: true,
            stats_interval_secs: 5,
        }
    }
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

impl Settings {
    /// Checks values coming from the interface.
    pub fn validate(&self) -> Result<(), AppError> {
        if !(MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&self.font_size) {
            return Err(invalid("The font size must be between 8 and 48."));
        }
        if !(MIN_SCROLLBACK..=MAX_SCROLLBACK).contains(&self.scrollback) {
            return Err(invalid(
                "Scrollback must be between 10,000 and 1,000,000 lines.",
            ));
        }
        let interval = self.reachability_interval_secs;
        if interval != 0 && !(MIN_CHECK_INTERVAL..=MAX_CHECK_INTERVAL).contains(&interval) {
            return Err(invalid(
                "Check for reachable servers every 5 to 3600 seconds, or 0 to check only at start.",
            ));
        }
        if !(MIN_STATS_INTERVAL..=MAX_STATS_INTERVAL).contains(&self.stats_interval_secs) {
            return Err(invalid("Refresh the footer every 2 to 300 seconds."));
        }
        if self.log_folder.len() > 500 || self.log_folder.chars().any(|c| c.is_control()) {
            return Err(invalid("That log folder isn't valid."));
        }
        if self.font_family.len() > 200 || self.font_family.chars().any(|c| c.is_control()) {
            return Err(invalid("That font name isn't valid."));
        }
        Ok(())
    }

    /// Pulls values from a hand-edited file back into range, rather than refusing to start.
    fn clamped(mut self) -> Self {
        self.font_size = self.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.scrollback = self.scrollback.clamp(MIN_SCROLLBACK, MAX_SCROLLBACK);
        if self.reachability_interval_secs != 0 {
            self.reachability_interval_secs = self
                .reachability_interval_secs
                .clamp(MIN_CHECK_INTERVAL, MAX_CHECK_INTERVAL);
        }
        self.stats_interval_secs = self
            .stats_interval_secs
            .clamp(MIN_STATS_INTERVAL, MAX_STATS_INTERVAL);
        if self.log_folder.len() > 500 || self.log_folder.chars().any(|c| c.is_control()) {
            self.log_folder.clear();
        }
        if self.font_family.trim().is_empty() || self.font_family.chars().any(|c| c.is_control()) {
            self.font_family = Settings::default().font_family;
        }
        self
    }

    /// The environment variable to set before the web view starts, if one is chosen. Linux only
    /// (the caller decides), and it takes effect after a restart.
    pub fn webkit_env(&self) -> Option<(&'static str, &'static str)> {
        match self.webkit_workaround {
            WebkitWorkaround::Off => None,
            WebkitWorkaround::DisableDmabuf => Some(("WEBKIT_DISABLE_DMABUF_RENDERER", "1")),
            WebkitWorkaround::DisableCompositing => Some(("WEBKIT_DISABLE_COMPOSITING_MODE", "1")),
        }
    }
}

/// Reads the settings. A missing file gives the defaults. A file that can't be understood is
/// moved aside to `settings.toml.bad` and the defaults are used, so the user's text is kept.
pub fn load(path: &Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<Settings>(&text) {
            Ok(settings) => settings.clamped(),
            Err(_) => {
                set_aside(path);
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), AppError> {
    settings.validate()?;
    let text = toml::to_string_pretty(settings)
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't save the settings."))?;
    write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        (dir, path)
    }

    #[test]
    fn check_intervals_are_kept_in_range() {
        let mut s = Settings::default();
        assert!(s.validate().is_ok());
        s.reachability_interval_secs = 0;
        assert!(s.validate().is_ok(), "0 means only at start");
        s.reachability_interval_secs = 2;
        assert!(s.validate().is_err());
        s.reachability_interval_secs = 60;
        s.stats_interval_secs = 1;
        assert!(s.validate().is_err());

        let (_dir, path) = path();
        std::fs::write(
            &path,
            "reachability_interval_secs = 1
stats_interval_secs = 9999
",
        )
        .unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.reachability_interval_secs, MIN_CHECK_INTERVAL);
        assert_eq!(loaded.stats_interval_secs, MAX_STATS_INTERVAL);
    }

    #[test]
    fn a_missing_file_gives_defaults() {
        let (_dir, path) = path();
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn saves_and_reloads() {
        let (_dir, path) = path();
        let s = Settings {
            font_size: 18,
            theme: Theme::Light,
            renderer: Renderer::Webgl,
            webkit_workaround: WebkitWorkaround::DisableDmabuf,
            scrollback: 50_000,
            restore_layout: false,
            ..Settings::default()
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("renderer = \"webgl\""), "{text}");
        assert!(
            text.contains("webkit_workaround = \"disable_dmabuf\""),
            "{text}"
        );
    }

    #[test]
    fn missing_keys_take_defaults() {
        let (_dir, path) = path();
        std::fs::write(&path, "font_size = 20\n").unwrap();
        let s = load(&path);
        assert_eq!(s.font_size, 20);
        assert_eq!(s.scrollback, MIN_SCROLLBACK);
        assert_eq!(s.renderer, Renderer::Auto);
    }

    #[test]
    fn a_corrupt_file_is_kept_and_defaults_are_used() {
        let (dir, path) = path();
        std::fs::write(&path, "font_size = [oops").unwrap();
        assert_eq!(load(&path), Settings::default());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("settings.toml.bad")).unwrap(),
            "font_size = [oops"
        );
    }

    #[test]
    fn a_wrong_value_counts_as_corrupt() {
        let (dir, path) = path();
        std::fs::write(&path, "renderer = \"fast\"\n").unwrap();
        assert_eq!(load(&path), Settings::default());
        assert!(dir.path().join("settings.toml.bad").exists());
    }

    #[test]
    fn hand_edited_numbers_are_pulled_into_range() {
        let (_dir, path) = path();
        std::fs::write(&path, "scrollback = 5\nfont_size = 400\n").unwrap();
        let s = load(&path);
        assert_eq!(s.scrollback, MIN_SCROLLBACK);
        assert_eq!(s.font_size, MAX_FONT_SIZE);
    }

    #[test]
    fn saving_refuses_bad_values_and_writes_nothing() {
        let (_dir, path) = path();
        for bad in [
            Settings {
                scrollback: 100,
                ..Settings::default()
            },
            Settings {
                font_size: 2,
                ..Settings::default()
            },
            Settings {
                font_family: "a\nb".into(),
                ..Settings::default()
            },
        ] {
            assert_eq!(save(&path, &bad).unwrap_err().code, ErrorCode::InvalidInput);
        }
        assert!(!path.exists());
    }

    #[test]
    fn the_webkit_workaround_picks_the_matching_variable() {
        let mut s = Settings::default();
        assert_eq!(s.webkit_env(), None);
        s.webkit_workaround = WebkitWorkaround::DisableDmabuf;
        assert_eq!(
            s.webkit_env(),
            Some(("WEBKIT_DISABLE_DMABUF_RENDERER", "1"))
        );
        s.webkit_workaround = WebkitWorkaround::DisableCompositing;
        assert_eq!(
            s.webkit_env(),
            Some(("WEBKIT_DISABLE_COMPOSITING_MODE", "1"))
        );
    }
}
