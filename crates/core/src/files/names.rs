//! Names that come from a server can't be trusted (SEC11).

/// Checks one name from a remote listing before it is used to make a file or folder on this
/// computer. Returns what is wrong with it, or nothing if it is fine. A name that is empty, is
/// `.` or `..`, contains a path separator or a NUL character, or starts with a Windows drive
/// (`C:`) could write outside the chosen folder, so it is refused.
pub fn check_remote_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("the name is empty");
    }
    if name == "." || name == ".." {
        return Err("the name is a folder reference");
    }
    if name.contains('/') || name.contains('\\') {
        return Err("the name contains a path separator");
    }
    if name.contains('\0') {
        return Err("the name contains a NUL character");
    }
    let mut chars = name.chars();
    if let (Some(first), Some(':')) = (chars.next(), chars.next())
        && first.is_ascii_alphabetic()
    {
        return Err("the name starts with a drive letter");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_names_pass() {
        for ok in [
            "file.txt",
            "my folder",
            ".hidden",
            "a..b",
            "ünïcode",
            "…",
            "c",
            "1:2",
        ] {
            assert!(check_remote_name(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn names_that_could_escape_are_refused() {
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            "../x",
            "/etc/passwd",
            "a\\b",
            "..\\..\\x",
            "bad\0name",
            "C:",
            "c:evil",
            "Z:\\x",
        ] {
            assert!(check_remote_name(bad).is_err(), "{bad:?}");
        }
    }
}
