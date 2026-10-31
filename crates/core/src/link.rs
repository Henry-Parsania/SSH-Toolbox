//! Links clicked in a terminal (INTERFACES.md section 13, SEC9). Terminal output is untrusted,
//! so only plain web addresses are ever passed on to the system.

use url::Url;

use crate::error::{AppError, ErrorCode};

const MAX_LENGTH: usize = 4096;

/// Returns the address in a clean form if it is an `http` or `https` link with a host, and an
/// error for anything else (`file:`, `javascript:`, custom schemes, and so on).
pub fn checked_link(text: &str) -> Result<String, AppError> {
    let refuse = || {
        AppError::new(
            ErrorCode::InvalidInput,
            "Only web links (http and https) can be opened.",
        )
    };
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_LENGTH || text.chars().any(|c| c.is_control()) {
        return Err(refuse());
    }
    let url = Url::parse(text).map_err(|_| refuse())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(refuse());
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_links_are_allowed() {
        assert_eq!(
            checked_link("https://example.com/a?b=1#c").unwrap(),
            "https://example.com/a?b=1#c"
        );
        assert_eq!(
            checked_link("http://localhost:8080").unwrap(),
            "http://localhost:8080/"
        );
        assert_eq!(
            checked_link("  HTTPS://Example.COM  ").unwrap(),
            "https://example.com/"
        );
    }

    #[test]
    fn everything_else_is_refused() {
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "data:text/html,<script>1</script>",
            "ms-msdt:/id PCWDiagnostic",
            "vscode://something",
            "ssh://host",
            "ftp://host/file",
            "mailto:a@b.c",
            "\\\\server\\share\\x.exe",
            "C:\\Windows\\notepad.exe",
            "example.com",
            "https://",
            "",
            "   ",
        ] {
            assert!(checked_link(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn control_characters_and_huge_input_are_refused() {
        assert!(checked_link("https://example.com/\u{1b}[31m").is_err());
        assert!(checked_link("https://example.com/\nfile:///x").is_err());
        assert!(checked_link(&format!("https://example.com/{}", "a".repeat(5000))).is_err());
    }
}
