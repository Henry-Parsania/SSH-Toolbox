use std::fmt;

use serde::{Deserialize, Deserializer};
use zeroize::Zeroizing;

/// Holds a password or passphrase. It wipes itself on drop, prints as `[redacted]`,
/// and has no `Display`, `Clone` or `Serialize`.
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Secret::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_contain_the_value() {
        let secret = Secret::new("hunter2-very-secret");
        let text = format!("{:?}", secret);
        assert_eq!(text, "[redacted]");
        assert!(!text.contains("hunter2"));
    }

    #[test]
    fn debug_of_a_containing_struct_does_not_leak() {
        #[derive(Debug)]
        struct Login {
            #[allow(dead_code)]
            password: Secret,
        }
        let text = format!(
            "{:#?}",
            Login {
                password: Secret::new("hunter2-very-secret")
            }
        );
        assert!(!text.contains("hunter2"));
    }

    #[test]
    fn expose_returns_the_value() {
        assert_eq!(Secret::new("abc").expose(), "abc");
    }

    #[test]
    fn deserializes_from_a_string() {
        let secret: Secret = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(secret.expose(), "abc");
    }
}
