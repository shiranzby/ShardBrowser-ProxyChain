//! An explicit per-profile route. Missing fields keep the legacy policy;
//! malformed explicit data is retained as an error, never a direct route.
use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ProfileProxyRoute {
    Direct,
    System,
    Proxy,
    Chain { first: FirstHop, second: SecondHop },
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum FirstHop { Direct, System, Proxy { proxy_id: String } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SecondHop { Direct, Proxy }

impl<'de> Deserialize<'de> for ProfileProxyRoute {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire { Direct, System, Proxy, Chain { first: FirstHop, second: SecondHop } }
        let value = serde_json::Value::deserialize(d)?;
        let keys = |value: &serde_json::Value, allowed: &[&str]| value.as_object().is_some_and(|map| map.keys().all(|key| allowed.contains(&key.as_str())));
        // Serde unit variants may accept extra keys even with deny_unknown_fields.
        // Check every object ourselves so contradictory route data fails closed.
        let exact = if value.get("mode").and_then(|v| v.as_str()) == Some("chain") {
            let first = &value["first"];
            let first_keys = if first.get("mode").and_then(|v| v.as_str()) == Some("proxy") { &["mode", "proxy_id"][..] } else { &["mode"][..] };
            keys(&value, &["mode", "first", "second"]) && keys(first, first_keys) && keys(&value["second"], &["mode"])
        } else { keys(&value, &["mode"]) };
        if !exact { return Ok(Self::Invalid); }
        Ok(match serde_json::from_value::<Wire>(value) {
            Ok(Wire::Direct) => Self::Direct,
            Ok(Wire::System) => Self::System,
            Ok(Wire::Proxy) => Self::Proxy,
            Ok(Wire::Chain { first, second }) => Self::Chain { first, second },
            Err(_) => Self::Invalid,
        })
    }
}

impl ProfileProxyRoute {
    pub fn validate(&self) -> Result<()> {
        if matches!(self, Self::Invalid) {
            bail!("The profile proxy route is invalid. Choose a connection mode before saving or starting.");
        }
        if let Some(id) = self.first_proxy_id() {
            if id.trim().is_empty() || id != id.trim() || id.len() > 128 || id.chars().any(char::is_control) {
                bail!("Select a valid first-hop proxy.");
            }
        }
        Ok(())
    }

    pub fn requires_assigned(&self) -> bool {
        matches!(self, Self::Proxy | Self::Chain { second: SecondHop::Proxy, .. })
    }

    pub fn first_proxy_id(&self) -> Option<&str> {
        match self { Self::Chain { first: FirstHop::Proxy { proxy_id }, .. } => Some(proxy_id), _ => None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_routes_roundtrip_and_malformed_routes_never_become_direct() {
        for raw in [json!({"mode":"direct"}), json!({"mode":"system"}), json!({"mode":"proxy"}),
            json!({"mode":"chain","first":{"mode":"proxy","proxy_id":"first"},"second":{"mode":"proxy"}}),
            json!({"mode":"chain","first":{"mode":"direct"},"second":{"mode":"direct"}})] {
            let route: ProfileProxyRoute = serde_json::from_value(raw.clone()).unwrap();
            route.validate().unwrap();
            assert_eq!(serde_json::to_value(route).unwrap(), raw);
        }
        for raw in [json!(true), json!({}), json!({"mode":"future"}), json!({"mode":"direct","first":{"mode":"system"}}),
            json!({"mode":"chain","first":{"mode":"direct","proxy_id":"ignored"},"second":{"mode":"direct"}}),
            json!({"mode":"chain","first":{"mode":"system"},"second":{"mode":"direct","proxy_id":"ignored"}}),
            json!({"mode":"chain","first":{"mode":"proxy","proxy_id":""},"second":{"mode":"proxy"}}),
            json!({"mode":"chain","first":{"mode":"system"}})] {
            let route: ProfileProxyRoute = serde_json::from_value(raw).unwrap();
            assert!(route.validate().is_err(), "unexpected valid route: {route:?}");
        }
    }
}
