use super::{config::ProxyChainSettings, error::ChainError};
use serde::{Deserialize, Deserializer, Serialize};

/// Missing fields inherit. Unrecognised data is retained as a refusal, never
/// silently converted to an unchained route by a legacy/defaulting reader.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainPolicy {
    #[default]
    Inherit,
    Enabled,
    Disabled,
    Invalid,
}

impl<'de> Deserialize<'de> for ChainPolicy {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        Ok(match value.as_str() {
            Some("inherit") => Self::Inherit,
            Some("enabled") => Self::Enabled,
            Some("disabled") => Self::Disabled,
            _ => Self::Invalid,
        })
    }
}

impl ChainPolicy {
    pub fn is_inherit(&self) -> bool {
        *self == Self::Inherit
    }

    pub fn validate(self) -> Result<(), ChainError> {
        if self == Self::Invalid {
            return Err(ChainError::new("proxy_chain_policy_invalid", "configuration", "The proxy-chain choice is invalid. Choose inherit, enabled or disabled before saving or starting."));
        }
        Ok(())
    }
}

/// Profile > assigned proxy > this computer's default. Disabling only skips
/// the first hop; it never removes the original assigned/inline proxy.
pub fn effective(
    config: &ProxyChainSettings,
    profile: ChainPolicy,
    proxy: ChainPolicy,
) -> Result<ProxyChainSettings, ChainError> {
    profile.validate()?;
    proxy.validate()?;
    // A damaged enabled protection setting must be repaired explicitly, not
    // hidden by a per-object override.
    if config.invalid || config.enabled {
        config.validate()?;
    }
    let mut resolved = config.clone();
    resolved.enabled = match if profile.is_inherit() { proxy } else { profile } {
        ChainPolicy::Inherit => config.enabled,
        ChainPolicy::Enabled => true,
        ChainPolicy::Disabled => false,
        ChainPolicy::Invalid => unreachable!("validated above"),
    };
    if resolved.enabled {
        resolved.validate()?;
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_covers_every_profile_proxy_global_combination() {
        use ChainPolicy::*;
        for global in [false, true] {
            let config = ProxyChainSettings {
                enabled: global,
                ..Default::default()
            };
            for profile in [Inherit, Enabled, Disabled] {
                for proxy in [Inherit, Enabled, Disabled] {
                    let expected = match (profile, proxy) {
                        (Enabled, _) | (Inherit, Enabled) => true,
                        (Disabled, _) | (Inherit, Disabled) => false,
                        _ => global,
                    };
                    assert_eq!(
                        effective(&config, profile, proxy).unwrap().enabled,
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn malformed_choices_and_enabled_settings_fail_closed() {
        for raw in ["null", "true", "12", "{}", "\"future\""] {
            let policy: ChainPolicy = serde_json::from_str(raw).unwrap();
            assert!(policy.validate().is_err());
            assert!(effective(&Default::default(), ChainPolicy::Disabled, policy).is_err());
        }
        let broken = ProxyChainSettings {
            enabled: true,
            invalid: true,
            ..Default::default()
        };
        assert!(effective(&broken, ChainPolicy::Disabled, ChainPolicy::Inherit).is_err());
        let disabled = ProxyChainSettings {
            mode: "unknown".into(),
            ..Default::default()
        };
        assert!(effective(&disabled, ChainPolicy::Enabled, ChainPolicy::Inherit).is_err());
    }
}
