//! WSL distributions (INTERFACES.md section 3, "WSL details").

use super::local::ShellSpec;
use crate::error::{AppError, ErrorCode};

/// Distributions that Docker Desktop adds for itself. They are not for people to open.
const HIDDEN: [&str; 2] = ["docker-desktop", "docker-desktop-data"];

/// Reads the output of `wsl.exe --list --quiet`, which is UTF-16LE. A byte order mark and stray
/// NUL characters are dropped, and so are blank lines and Docker's own distributions.
pub(crate) fn parse_distro_list(output: &[u8]) -> Vec<String> {
    let units: Vec<u16> = output
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    String::from_utf16_lossy(&units)
        .lines()
        .map(|line| line.replace(['\0', '\u{feff}'], "").trim().to_string())
        .filter(|name| !name.is_empty())
        .filter(|name| !HIDDEN.iter().any(|h| name.eq_ignore_ascii_case(h)))
        .collect()
}

/// The distributions installed, or none if WSL is missing or fails. That isn't an error.
#[cfg(windows)]
pub fn list_wsl_distros() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    match std::process::Command::new("wsl.exe")
        .args(["--list", "--quiet"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(out) if out.status.success() => parse_distro_list(&out.stdout),
        _ => Vec::new(),
    }
}

#[cfg(not(windows))]
pub fn list_wsl_distros() -> Vec<String> {
    Vec::new()
}

/// How to open a distribution. The name is its own argument, never part of a shell string, and
/// a name that could be read as an option is refused.
pub fn wsl_spec(distro: &str) -> Result<ShellSpec, AppError> {
    let bad = distro.is_empty()
        || distro.starts_with('-')
        || distro.chars().any(|c| c.is_control())
        || distro.len() > 256;
    if bad {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That isn't a valid WSL distribution name.",
        ));
    }
    Ok(ShellSpec::new("wsl.exe", &["-d", distro, "--cd", "~"]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str, bom: bool) -> Vec<u8> {
        let mut units: Vec<u16> = Vec::new();
        if bom {
            units.push(0xfeff);
        }
        units.extend(text.encode_utf16());
        units.iter().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn reads_utf16_output_and_hides_docker() {
        let out = utf16(
            "Ubuntu\r\ndocker-desktop\r\nDocker-Desktop-Data\r\nDebian\r\n",
            false,
        );
        assert_eq!(parse_distro_list(&out), vec!["Ubuntu", "Debian"]);
    }

    #[test]
    fn copes_with_a_byte_order_mark_blank_lines_and_stray_nuls() {
        let out = utf16("\r\nUbuntu-26.04\0\r\n\r\n  Alpine  \r\n", true);
        assert_eq!(parse_distro_list(&out), vec!["Ubuntu-26.04", "Alpine"]);
    }

    #[test]
    fn empty_or_odd_output_gives_no_distros() {
        assert!(parse_distro_list(b"").is_empty());
        assert!(parse_distro_list(b"x").is_empty());
    }

    #[test]
    fn the_distro_is_its_own_argument() {
        let spec = wsl_spec("Ubuntu").unwrap();
        assert_eq!(spec.program, "wsl.exe");
        assert_eq!(spec.args, ["-d", "Ubuntu", "--cd", "~"]);
        let odd = wsl_spec("My Distro; calc").unwrap();
        assert_eq!(odd.args[1], "My Distro; calc");
    }

    #[test]
    fn refuses_names_that_could_be_options_or_are_broken() {
        for bad in ["", "-e", "--exec", "a\nb", "a\0b"] {
            assert!(wsl_spec(bad).is_err(), "{bad:?}");
        }
        assert!(wsl_spec(&"x".repeat(300)).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn no_distros_off_windows() {
        assert!(list_wsl_distros().is_empty());
    }
}
