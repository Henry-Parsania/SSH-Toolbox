use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformInfo {
    /// `windows`, `linux` or `macos`.
    pub os: String,
    pub wsl_available: bool,
    /// Windows build number, such as 26200. `None` on other systems.
    pub windows_build: Option<u32>,
}

/// Blocking: may start a short-lived process on Windows. Call from `spawn_blocking`.
pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS.to_string(),
        wsl_available: wsl_available(),
        windows_build: windows_build(),
    }
}

#[cfg(windows)]
fn wsl_available() -> bool {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    std::path::Path::new(&root)
        .join("System32")
        .join("wsl.exe")
        .is_file()
}

#[cfg(not(windows))]
fn wsl_available() -> bool {
    false
}

#[cfg(windows)]
fn windows_build() -> Option<u32> {
    use std::process::Command;
    let output = Command::new("cmd").args(["/C", "ver"]).output().ok()?;
    parse_windows_build(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn windows_build() -> Option<u32> {
    None
}

/// Reads the build number from the output of `ver`, for example
/// `Microsoft Windows [Version 10.0.26200.1234]` gives 26200.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_windows_build(text: &str) -> Option<u32> {
    let start = text.find("Version")? + "Version".len();
    let version: String = text[start..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    version.split('.').nth(2)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_build_from_ver_output() {
        let text = "\r\nMicrosoft Windows [Version 10.0.26200.1234]\r\n";
        assert_eq!(parse_windows_build(text), Some(26200));
    }

    #[test]
    fn rejects_unrecognised_output() {
        assert_eq!(parse_windows_build("hello"), None);
        assert_eq!(parse_windows_build("Version 10.0"), None);
    }

    #[test]
    fn reports_a_known_os() {
        let info = platform_info();
        assert!(["windows", "linux", "macos"].contains(&info.os.as_str()));
        assert_eq!(info.windows_build.is_some(), info.os == "windows");
        if info.os != "windows" {
            assert!(!info.wsl_available);
        }
    }
}
