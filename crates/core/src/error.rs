use serde::Serialize;

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    /// Plain English, shown to the user. Never contains secrets.
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    PermissionDenied,
    ConnectionFailed,
    ConnectionLost,
    AuthFailed,
    HostKeyChanged,
    HostKeyRejected,
    CertificateRejected,
    KeychainUnavailable,
    AlreadyExists,
    Cancelled,
    Unsupported,
    InvalidInput,
    Internal,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_only_the_message() {
        let err = AppError::new(ErrorCode::NotFound, "That folder doesn't exist.");
        assert_eq!(err.to_string(), "That folder doesn't exist.");
    }

    #[test]
    fn serializes_code_in_snake_case() {
        let err = AppError::new(ErrorCode::HostKeyChanged, "The server's key changed.");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "host_key_changed");
        assert_eq!(json["message"], "The server's key changed.");
    }
}
