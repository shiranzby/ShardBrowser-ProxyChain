use super::{
    connector::{self, ProxyHop, ProxyKind, Target},
    credentials, detect_windows,
    error::ChainError,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Missing means keep, null means clear. Only a newly entered string is sent.
#[derive(Clone, Default)]
pub enum PasswordUpdate {
    #[default]
    Keep,
    Clear,
    Replace(String),
}
impl PasswordUpdate {
    fn keep(&self) -> bool {
        matches!(self, Self::Keep)
    }
}
impl fmt::Debug for PasswordUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}
impl<'de> Deserialize<'de> for PasswordUpdate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Option::<String>::deserialize(d)? {
            Some(s) if !s.is_empty() => Self::Replace(s),
            _ => Self::Clear,
        })
    }
}
impl Serialize for PasswordUpdate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Write-only IPC input. Neither persistence nor a response may reveal it.
        s.serialize_none()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyChainSettings {
    pub enabled: bool,
    pub mode: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password_saved: bool,
    #[serde(skip_serializing_if = "PasswordUpdate::keep")]
    pub password: PasswordUpdate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protected_password: Option<String>,
    /// Preserves a malformed chain as an error instead of defaulting to direct.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub invalid: bool,
    /// Runtime-only permission derived from a profile/proxy policy. Never
    /// accepted from the global settings JSON or written back to settings.
    #[serde(skip)]
    pub(crate) allow_assigned_fallback: bool,
}
impl Default for ProxyChainSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "auto".into(),
            host: "127.0.0.1".into(),
            port: 7890,
            username: String::new(),
            password_saved: false,
            password: PasswordUpdate::Keep,
            protected_password: None,
            invalid: false,
            allow_assigned_fallback: false,
        }
    }
}
impl fmt::Debug for ProxyChainSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyChainSettings")
            .field("enabled", &self.enabled)
            .field("mode", &self.mode)
            .field("invalid", &self.invalid)
            .finish_non_exhaustive()
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ProxyChainSettings, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(
        serde_json::from_value(value.clone()).unwrap_or_else(|_| ProxyChainSettings {
            enabled: value
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            invalid: true,
            ..Default::default()
        }),
    )
}

impl ProxyChainSettings {
    pub fn public(&self) -> Self {
        Self {
            password_saved: self.protected_password.is_some(),
            protected_password: None,
            password: PasswordUpdate::Keep,
            ..self.clone()
        }
    }
    pub fn apply_update(&mut self, saved: &Self) -> Result<(), ChainError> {
        // The client never owns stored ciphertext or password_saved.
        self.protected_password = match std::mem::take(&mut self.password) {
            PasswordUpdate::Keep => saved.protected_password.clone(),
            PasswordUpdate::Clear => None,
            PasswordUpdate::Replace(p) => Some(credentials::protect(&p)?),
        };
        self.password_saved = self.protected_password.is_some();
        if self.enabled {
            self.validate()?;
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), ChainError> {
        if self.invalid || !matches!(self.mode.as_str(), "auto" | "http" | "socks5") {
            return Err(ChainError::new(
                "proxy_chain_invalid_config",
                "configuration",
                "The proxy-chain settings are invalid. Configure the system proxy before starting.",
            ));
        }
        if self.mode != "auto" {
            let hop = ProxyHop {
                kind: if self.mode == "http" {
                    ProxyKind::Http
                } else {
                    ProxyKind::Socks5
                },
                host: self.host.clone(),
                port: self.port,
                username: self.username.clone(),
                password: String::new(),
            };
            connector::validate(&hop, &Target::new("example.com", 443)?)?;
        }
        Ok(())
    }
    pub fn hop(&self, saved: &Self) -> Result<ProxyHop, ChainError> {
        self.validate()?;
        if self.mode == "auto" {
            return detect_windows::resolve();
        }
        let password = match &self.password {
            PasswordUpdate::Replace(p) => p.clone(),
            PasswordUpdate::Clear => String::new(),
            PasswordUpdate::Keep => saved
                .protected_password
                .as_deref()
                .map(credentials::unprotect)
                .transpose()?
                .unwrap_or_default(),
        };
        let hop = ProxyHop {
            kind: if self.mode == "http" {
                ProxyKind::Http
            } else {
                ProxyKind::Socks5
            },
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            password,
        };
        connector::validate(&hop, &Target::new("example.com", 443)?)?;
        Ok(hop)
    }

    pub async fn resolve_for(
        &self,
        saved: &Self,
        target: &Target,
    ) -> Result<detect_windows::ResolvedProxy, ChainError> {
        self.validate()?;
        if self.mode == "auto" {
            detect_windows::resolve_for(target).await
        } else {
            Ok(detect_windows::ResolvedProxy::manual(self.hop(saved)?))
        }
    }

    pub async fn hop_for(&self, saved: &Self, target: &Target) -> Result<ProxyHop, ChainError> {
        self.resolve_for(saved, target)
            .await
            .map(|resolved| resolved.hop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Old {
        #[serde(default, deserialize_with = "deserialize")]
        proxy_chain: ProxyChainSettings,
    }
    #[test]
    fn fallback_permission_cannot_be_injected_through_settings_json() {
        let config: ProxyChainSettings = serde_json::from_str(r#"{"enabled":true,"mode":"http","host":"127.0.0.1","port":7890,"allow_assigned_fallback":true}"#).unwrap();
        assert!(!config.allow_assigned_fallback);
        let mut runtime = config.clone(); runtime.allow_assigned_fallback = true;
        assert!(serde_json::to_value(runtime).unwrap().get("allow_assigned_fallback").is_none());
    }

    #[test]
    fn old_settings_default_off_but_malformed_enabled_stays_closed() {
        assert!(
            !serde_json::from_str::<Old>("{}")
                .unwrap()
                .proxy_chain
                .enabled
        );
        let s = serde_json::from_str::<Old>(r#"{"proxy_chain":{"enabled":true,"port":"bad"}}"#)
            .unwrap();
        assert!(s.proxy_chain.enabled && s.proxy_chain.validate().is_err());
        assert!(
            serde_json::from_str::<Old>(r#"{"proxy_chain":null}"#)
                .unwrap()
                .proxy_chain
                .enabled
        );
    }
    #[test]
    fn missing_and_null_password_are_distinct_and_public_is_redacted() {
        let a: ProxyChainSettings = serde_json::from_str("{}").unwrap();
        let b: ProxyChainSettings = serde_json::from_str(r#"{"password":null}"#).unwrap();
        assert!(matches!(a.password, PasswordUpdate::Keep));
        assert!(matches!(b.password, PasswordUpdate::Clear));
        let s = ProxyChainSettings {
            password: PasswordUpdate::Replace("synthetic-secret".into()),
            protected_password: Some("synthetic-cipher".into()),
            ..a
        };
        let public = serde_json::to_string(&s.public()).unwrap();
        assert!(!public.contains("synthetic"));
        assert!(!format!("{s:?}").contains("synthetic"));
    }
}
