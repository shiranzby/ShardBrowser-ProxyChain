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
pub enum SecondHop { Direct, Proxy, System }

/// Runtime choices are independent of the saved legacy wire representation.
/// In particular, a first-only library proxy must keep selecting that proxy,
/// rather than replacing the profile's inactive assigned-proxy binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxySource { Assigned, SelectedFirst }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstSource { System, SelectedFirst }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePlan {
    Direct,
    System,
    Single { source: ProxySource },
    Chain { first: FirstSource },
}

impl RoutePlan {
    pub fn requires_assigned(self) -> bool {
        matches!(self, Self::Single { source: ProxySource::Assigned } | Self::Chain { .. })
    }

    pub fn uses_selected_first(self) -> bool {
        matches!(self, Self::Single { source: ProxySource::SelectedFirst } | Self::Chain { first: FirstSource::SelectedFirst })
    }

    pub fn uses_first(self) -> bool {
        self.first_only() || matches!(self, Self::Chain { .. })
    }

    /// A selected single proxy is the exit proxy, even in the old first-only wire.
    pub fn first_only(self) -> bool {
        matches!(self, Self::System)
    }
}

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
        if matches!(self,Self::Chain{first:FirstHop::System|FirstHop::Proxy{..},second:SecondHop::System}) {
            bail!("A system proxy as the second choice requires a direct first hop.");
        }
        if let Some(id) = self.first_proxy_id() {
            if id.trim().is_empty() || id != id.trim() || id.len() > 128 || id.chars().any(char::is_control) {
                bail!("Select a valid first-hop proxy.");
            }
        }
        Ok(())
    }

    /// Compile all old empty-layer aliases into the same four runtime choices.
    /// This does not rewrite saved routes, proxy ids, or credentials.
    pub fn plan(&self) -> Result<RoutePlan> {
        self.validate()?;
        Ok(match self {
            Self::Direct | Self::Chain { first: FirstHop::Direct, second: SecondHop::Direct } => RoutePlan::Direct,
            Self::System | Self::Chain { first: FirstHop::Direct, second: SecondHop::System }
                | Self::Chain { first: FirstHop::System, second: SecondHop::Direct } => RoutePlan::System,
            Self::Proxy | Self::Chain { first: FirstHop::Direct, second: SecondHop::Proxy } => RoutePlan::Single { source: ProxySource::Assigned },
            Self::Chain { first: FirstHop::Proxy { .. }, second: SecondHop::Direct } => RoutePlan::Single { source: ProxySource::SelectedFirst },
            Self::Chain { first: FirstHop::System, second: SecondHop::Proxy } => RoutePlan::Chain { first: FirstSource::System },
            Self::Chain { first: FirstHop::Proxy { .. }, second: SecondHop::Proxy } => RoutePlan::Chain { first: FirstSource::SelectedFirst },
            Self::Invalid | Self::Chain { second: SecondHop::System, .. } => unreachable!("validated route"),
        })
    }

    pub fn requires_assigned(&self) -> bool {
        self.plan().is_ok_and(RoutePlan::requires_assigned)
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

    #[test]
    fn five_choices_and_legacy_empty_layers_share_the_same_execution_plans() {
        use {FirstHop as F, ProfileProxyRoute as R, SecondHop as S};
        let selected = F::Proxy { proxy_id: "selected-first".into() };
        let assigned = RoutePlan::Single { source: ProxySource::Assigned };
        let first = RoutePlan::Single { source: ProxySource::SelectedFirst };
        let choices = [
            (R::Direct, RoutePlan::Direct),
            (R::System, RoutePlan::System),
            (R::Proxy, assigned),
            (R::Chain { first: F::System, second: S::Proxy }, RoutePlan::Chain { first: FirstSource::System }),
            (R::Chain { first: selected.clone(), second: S::Proxy }, RoutePlan::Chain { first: FirstSource::SelectedFirst }),
            (R::Chain { first: F::Direct, second: S::Direct }, RoutePlan::Direct),
            (R::Chain { first: F::Direct, second: S::System }, RoutePlan::System),
            (R::Chain { first: F::System, second: S::Direct }, RoutePlan::System),
            (R::Chain { first: F::Direct, second: S::Proxy }, assigned),
            (R::Chain { first: selected.clone(), second: S::Direct }, first),
        ];
        for (route, expected) in choices {
            let saved = serde_json::to_value(&route).unwrap();
            assert_eq!(route.plan().unwrap(), expected);
            assert_eq!(route.requires_assigned(), expected.requires_assigned());
            assert_eq!(serde_json::to_value(&route).unwrap(), saved, "planning must not rewrite old bindings");
            assert_eq!(serde_json::from_value::<R>(saved).unwrap(), route);
        }
        let legacy_first = R::Chain { first: selected.clone(), second: S::Direct };
        assert_eq!(legacy_first.first_proxy_id(), Some("selected-first"));
        assert!(!legacy_first.requires_assigned());
        assert!(legacy_first.plan().unwrap().uses_selected_first());
        for first in [FirstHop::System, selected] {
            let route=ProfileProxyRoute::Chain{first,second:SecondHop::System};
            assert!(route.validate().is_err());
            assert!(route.plan().is_err());
            let restored:ProfileProxyRoute=serde_json::from_value(serde_json::to_value(&route).unwrap()).unwrap();
            assert!(restored.validate().is_err(),"invalid persisted routes must not become direct");
        }
        let unknown:ProfileProxyRoute=serde_json::from_value(json!({"mode":"chain","first":{"mode":"direct"},"second":{"mode":"future"}})).unwrap();
        assert!(unknown.validate().is_err());
    }
}
