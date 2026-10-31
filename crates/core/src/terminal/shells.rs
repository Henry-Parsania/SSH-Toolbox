//! Finding the local shells a user can open (INTERFACES.md section 3, "Local shell details").

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::local::ShellSpec;
use crate::error::{AppError, ErrorCode};

/// A shell offered in the "new terminal" menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shell {
    pub id: String,
    pub name: String,
}

/// A shell and how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalShellInfo {
    pub shell: Shell,
    pub spec: ShellSpec,
}

fn info(id: &str, name: &str, program: &Path, args: &[&str]) -> LocalShellInfo {
    LocalShellInfo {
        shell: Shell {
            id: id.into(),
            name: name.into(),
        },
        spec: ShellSpec::new(program.to_string_lossy(), args),
    }
}

fn find_in_dirs(name: &str, dirs: &[PathBuf], exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(name)).find(|p| exists(p))
}

/// The Windows shells that are present, in menu order.
///
/// Git Bash is looked for at the usual place, then next to `git.exe` on the path (`git.exe` is
/// in a `cmd` folder, and `bash.exe` is in the `bin` folder beside it). `bash.exe` on the path
/// is never used: `C:\Windows\System32\bash.exe` launches WSL, not Git Bash.
pub(crate) fn windows_shells(
    path_dirs: &[PathBuf],
    system_root: &Path,
    program_files: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Vec<LocalShellInfo> {
    let mut found = Vec::new();

    let system32 = system_root.join("System32");
    let powershell = system32
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let powershell = if exists(&powershell) {
        Some(powershell)
    } else {
        find_in_dirs("powershell.exe", path_dirs, exists)
    };
    if let Some(path) = powershell {
        found.push(info("powershell", "PowerShell", &path, &["-NoLogo"]));
    }

    if let Some(path) = find_in_dirs("pwsh.exe", path_dirs, exists) {
        found.push(info("pwsh", "PowerShell 7", &path, &["-NoLogo"]));
    }

    let cmd = system32.join("cmd.exe");
    let cmd = if exists(&cmd) {
        Some(cmd)
    } else {
        find_in_dirs("cmd.exe", path_dirs, exists)
    };
    if let Some(path) = cmd {
        found.push(info("cmd", "Command Prompt", &path, &[]));
    }

    let usual = program_files.join("Git").join("bin").join("bash.exe");
    let git_bash = if exists(&usual) {
        Some(usual)
    } else {
        find_in_dirs("git.exe", path_dirs, exists)
            .and_then(|git| {
                git.parent()?
                    .parent()
                    .map(|root| root.join("bin").join("bash.exe"))
            })
            .filter(|bash| exists(bash))
    };
    if let Some(path) = git_bash {
        found.push(info("git-bash", "Git Bash", &path, &["--login", "-i"]));
    }

    found
}

/// The one shell offered on Linux and macOS: `$SHELL` if it exists, otherwise `/bin/bash`.
pub(crate) fn unix_shell(
    shell_var: Option<&str>,
    exists: &dyn Fn(&Path) -> bool,
) -> LocalShellInfo {
    let program = shell_var
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| exists(p))
        .unwrap_or_else(|| PathBuf::from("/bin/bash"));
    let name = program
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "bash".into());
    info("default", &name, &program, &["-l"])
}

fn all_local_shells() -> Vec<LocalShellInfo> {
    let exists = |p: &Path| p.is_file();
    if cfg!(windows) {
        let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let program_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        windows_shells(&path_dirs, &system_root, &program_files, &exists)
    } else {
        let shell_var = std::env::var("SHELL").ok();
        vec![unix_shell(shell_var.as_deref(), &exists)]
    }
}

/// The shells present on this computer. Blocking: it looks at the file system.
pub fn list_shells() -> Vec<Shell> {
    all_local_shells().into_iter().map(|s| s.shell).collect()
}

/// How to start the shell with this ID. Blocking: it looks at the file system.
pub fn shell_spec(id: &str) -> Result<ShellSpec, AppError> {
    all_local_shells()
        .into_iter()
        .find(|s| s.shell.id == id)
        .map(|s| s.spec)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                "That shell isn't installed on this computer.",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn files(paths: &[&str]) -> HashSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn ids(shells: &[LocalShellInfo]) -> Vec<&str> {
        shells.iter().map(|s| s.shell.id.as_str()).collect()
    }

    #[test]
    fn windows_lists_only_what_is_present() {
        let present = files(&["C:/Windows/System32/cmd.exe"]);
        let shells = windows_shells(
            &[],
            Path::new("C:/Windows"),
            Path::new("C:/Program Files"),
            &|p| present.contains(p),
        );
        assert_eq!(ids(&shells), vec!["cmd"]);
    }

    #[test]
    fn windows_lists_everything_in_menu_order() {
        let present = files(&[
            "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
            "C:/Tools/pwsh.exe",
            "C:/Windows/System32/cmd.exe",
            "C:/Program Files/Git/bin/bash.exe",
        ]);
        let shells = windows_shells(
            &[PathBuf::from("C:/Tools")],
            Path::new("C:/Windows"),
            Path::new("C:/Program Files"),
            &|p| present.contains(p),
        );
        assert_eq!(ids(&shells), vec!["powershell", "pwsh", "cmd", "git-bash"]);
        let bash = shells.iter().find(|s| s.shell.id == "git-bash").unwrap();
        assert_eq!(bash.spec.args, vec!["--login", "-i"]);
        assert!(bash.spec.program.ends_with("bash.exe"));
    }

    #[test]
    fn git_bash_is_found_next_to_git_on_the_path() {
        let present = files(&["D:/Dev/Git/cmd/git.exe", "D:/Dev/Git/bin/bash.exe"]);
        let shells = windows_shells(
            &[PathBuf::from("D:/Dev/Git/cmd")],
            Path::new("C:/Windows"),
            Path::new("C:/Program Files"),
            &|p| present.contains(p),
        );
        assert_eq!(ids(&shells), vec!["git-bash"]);
        let expected = PathBuf::from("D:/Dev/Git").join("bin").join("bash.exe");
        assert_eq!(Path::new(&shells[0].spec.program), expected);
    }

    #[test]
    fn the_system32_bash_that_launches_wsl_is_never_used() {
        // `bash.exe` sits in System32 and on the path, but there's no Git for Windows.
        let present = files(&[
            "C:/Windows/System32/bash.exe",
            "C:/Windows/System32/cmd.exe",
        ]);
        let shells = windows_shells(
            &[PathBuf::from("C:/Windows/System32")],
            Path::new("C:/Windows"),
            Path::new("C:/Program Files"),
            &|p| present.contains(p),
        );
        assert_eq!(ids(&shells), vec!["cmd"]);
    }

    #[test]
    fn git_without_a_bash_beside_it_gives_no_git_bash() {
        let present = files(&["D:/Dev/Git/cmd/git.exe"]);
        let shells = windows_shells(
            &[PathBuf::from("D:/Dev/Git/cmd")],
            Path::new("C:/Windows"),
            Path::new("C:/Program Files"),
            &|p| present.contains(p),
        );
        assert!(shells.is_empty());
    }

    #[test]
    fn unix_uses_the_shell_variable_with_a_login_flag() {
        let present = files(&["/usr/bin/zsh"]);
        let shell = unix_shell(Some("/usr/bin/zsh"), &|p| present.contains(p));
        assert_eq!(shell.shell.id, "default");
        assert_eq!(shell.shell.name, "zsh");
        assert_eq!(shell.spec, ShellSpec::new("/usr/bin/zsh", &["-l"]));
    }

    #[test]
    fn unix_falls_back_to_bash_when_the_variable_is_missing_empty_or_wrong() {
        let nothing = |_: &Path| false;
        for var in [None, Some(""), Some("/no/such/shell")] {
            let shell = unix_shell(var, &nothing);
            assert_eq!(shell.spec, ShellSpec::new("/bin/bash", &["-l"]), "{var:?}");
            assert_eq!(shell.shell.name, "bash");
        }
    }

    #[test]
    fn an_unknown_shell_id_is_not_found() {
        let error = shell_spec("no-such-shell").unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[test]
    fn this_computer_has_at_least_one_shell_and_each_can_be_resolved() {
        let shells = list_shells();
        assert!(!shells.is_empty());
        for shell in shells {
            assert!(shell_spec(&shell.id).is_ok(), "{}", shell.id);
        }
    }
}
