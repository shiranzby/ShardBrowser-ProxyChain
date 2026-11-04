use serde::Serialize;
use std::fmt;

/// An error safe to show in the interface or keep in a diagnostic log.
/// Never carry a proxy URL, response body, authentication header or IO source.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChainError {
    pub code: &'static str,
    pub layer: &'static str,
    pub message: String,
}

impl ChainError {
    pub fn new(code: &'static str, layer: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            layer,
            message: message.into(),
        }
    }
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ChainError {}
