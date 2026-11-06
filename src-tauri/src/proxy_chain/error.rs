use serde::Serialize;
use std::fmt;

/// An error safe to show in the interface or keep in a diagnostic log.
/// Never carry a proxy URL, response body, authentication header or IO source.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChainError {
    pub code: &'static str,
    pub layer: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_hop_error_code: Option<&'static str>,
}

impl ChainError {
    pub fn new(code: &'static str, layer: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            layer,
            message: message.into(),
            first_hop_error_code: None,
        }
    }
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ChainError {}
