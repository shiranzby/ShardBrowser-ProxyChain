use serde::Serialize;
use std::fmt;

/// Fixed protocol metadata from a parsed proxy response, never its raw text.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProxyFailureDetails {
    pub protocol: &'static str,
    pub phase: &'static str,
    pub response_code: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
}

/// An error safe to show in the interface or keep in a diagnostic log.
/// Never carry a proxy URL, response body, authentication header or IO source.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChainError {
    pub code: &'static str,
    pub layer: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_hop_error_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ProxyFailureDetails>,
    /// Internal typed source failures; detection exposes only its safe DTO.
    #[serde(skip)]
    pub geo_failures: Option<Vec<crate::space::proxy::GeoProviderFailure>>,
}

impl ChainError {
    pub fn new(code: &'static str, layer: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            layer,
            message: message.into(),
            first_hop_error_code: None,
            details: None,
            geo_failures: None,
        }
    }

    pub fn with_proxy_response(mut self, protocol: &'static str, phase: &'static str, response_code: u16, target: Option<&'static str>) -> Self {
        self.details=Some(ProxyFailureDetails{protocol,phase,response_code,target});
        self
    }
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ChainError {}
