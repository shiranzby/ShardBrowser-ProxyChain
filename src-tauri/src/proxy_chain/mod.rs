//! Windows system proxy -> original profile proxy. Never fall back to direct.
pub mod config;
pub mod connector;
pub mod credentials;
pub mod detect_windows;
pub mod error;
#[cfg(test)]
mod tests;
pub mod tunnel;

use crate::space::proxy::{self, ProxyEntry, ProxyKind};
use config::ProxyChainSettings;
use connector::{ProxyHop, Target};
use error::ChainError;
use serde::Serialize;
use std::time::Instant;
use tunnel::Tunnel;

pub struct PreparedProxy {
    pub effective: ProxyEntry,
    pub tunnel: Tunnel,
}

pub fn saved_settings() -> anyhow::Result<crate::settings::Settings> {
    let settings = crate::settings::load()?;
    if crate::settings::load_error().is_some() {
        anyhow::bail!("Settings could not be read. Repair them before starting or testing a profile; proxy-chain protection cannot be determined.");
    }
    Ok(settings)
}

pub async fn prepare(
    entry: Option<&ProxyEntry>,
    config: &ProxyChainSettings,
) -> anyhow::Result<Option<PreparedProxy>> {
    if !config.enabled {
        return Ok(None);
    }
    #[cfg(not(windows))]
    anyhow::bail!("Proxy chaining is supported on Windows only.");
    let entry = entry.ok_or_else(|| {
        ChainError::new(
            "proxy_chain_profile_required",
            "profile",
            "This profile has no upstream proxy. Assign a SOCKS5 proxy or disable proxy chaining.",
        )
    })?;
    let hop = config.hop(config)?;
    Ok(Some(prepare_with_hop(entry, hop).await?))
}

async fn prepare_with_hop(entry: &ProxyEntry, hop: ProxyHop) -> anyhow::Result<PreparedProxy> {
    if entry.kind != ProxyKind::Socks5 {
        return Err(ChainError::new("proxy_chain_profile_kind_unsupported", "profile", "Proxy-chain V1 requires a SOCKS5 profile proxy. HTTP/HTTPS profile data is preserved; disable chaining to use its original connection.").into());
    }
    let target = Target::new(&entry.host, entry.port)?;
    // Proves the first hop can reach the original upstream before binding.
    // It is not proof of second-hop authentication; callers also run probe.
    drop(connector::connect(&hop, &target).await?);
    let tunnel = Tunnel::bind(hop, target).await?;
    let mut effective = entry.clone();
    effective.host = "127.0.0.1".into();
    effective.port = tunnel.addr().port();
    // Never persist an ephemeral loopback address or use direct-route history.
    effective.id.clear();
    Ok(PreparedProxy { effective, tunnel })
}

pub fn validate_args(raw: &str) -> anyhow::Result<()> {
    for token in crate::settings::parse_extra_args(raw) {
        let arg = token.trim();
        let Some(body) = arg
            .strip_prefix("--")
            .or_else(|| arg.strip_prefix('-'))
            .or_else(|| arg.strip_prefix('/'))
        else {
            continue;
        };
        let name = body.split('=').next().unwrap_or("").to_ascii_lowercase();
        if name.starts_with("proxy-")
            || name.starts_with("remote-debugging-")
            || name.starts_with("shardx-webrtc-")
            || matches!(
                name.as_str(),
                "no-proxy-server"
                    | "enable-quic"
                    | "disable-quic"
                    | "force-webrtc-ip-handling-policy"
                    | "webrtc-ip-handling-policy"
                    | "fingerprint-profile"
                    | "user-data-dir"
                    | "shardx-automation"
            )
        {
            anyhow::bail!("Extra argument --{name} conflicts with proxy-chain protection. Remove it before starting.");
        }
    }
    Ok(())
}

pub fn profile_error(tunnel: &Tunnel) -> ChainError {
    tunnel.last_error().unwrap_or_else(|| ChainError::new("proxy_chain_profile_failed", "profile", "The profile SOCKS5 proxy rejected authentication or could not reach the test target through the system proxy."))
}

pub fn profile_failure(tunnel: &Tunnel, cause: &anyhow::Error) -> ChainError {
    if let Some(error) = tunnel.last_error() { return error; }
    // Classify our own probe labels; never return its raw error/URL to the UI.
    let text = cause.to_string().to_ascii_lowercase();
    if text.contains("username") || text.contains("password") || text.contains("login method") {
        ChainError::new("proxy_chain_profile_auth_failed", "profile", "The profile SOCKS5 proxy rejected its username or password. System proxy credentials are separate.")
    } else if text.contains("time") || text.contains("never answered") {
        ChainError::new("proxy_chain_profile_timeout", "profile", "The profile SOCKS5 proxy did not finish its handshake or CONNECT in time.")
    } else {
        ChainError::new("proxy_chain_profile_connect_failed", "profile", "The profile SOCKS5 proxy refused or could not complete the destination CONNECT request.")
    }
}

#[derive(Serialize)]
pub struct Stage {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}
impl Stage {
    fn pending() -> Self {
        Self {
            state: "not_tested",
            latency_ms: None,
            error_code: None,
            error_message: None,
        }
    }
    fn ok(start: Instant) -> Self {
        Self {
            state: "ok",
            latency_ms: Some(start.elapsed().as_millis()),
            error_code: None,
            error_message: None,
        }
    }
    fn failed(e: ChainError) -> Self {
        Self {
            state: "failed",
            latency_ms: None,
            error_code: Some(e.code),
            error_message: Some(e.message),
        }
    }
}
#[derive(Serialize)]
pub struct TestResult {
    kind: String,
    proxy_kind: Option<&'static str>,
    proxy_address: Option<String>,
    source: Option<&'static str>,
    system_proxy: Stage,
    profile_proxy: Stage,
    local_network: Option<Stage>,
    exit_ip: Option<String>,
    exit_geo: Option<proxy::GeoInfo>,
    exit_error: Option<String>,
    exit_error_code: Option<&'static str>,
    total_ms: u128,
    tested_at: String,
}

impl TestResult {
    fn set_exit(&mut self, geo: anyhow::Result<proxy::GeoInfo>) {
        match geo {
            Ok(geo) => {
                self.exit_ip = Some(geo.ip.clone());
                self.exit_geo = Some(geo);
            }
            Err(_) => {
                let (code, message) = match self.kind.as_str() {
                    "local" => (
                        "proxy_chain_local_geo_failed",
                        "The local network test returned no live exit location.",
                    ),
                    "system" => (
                        "proxy_chain_system_geo_failed",
                        "The system proxy connected, but no live exit location was returned through it.",
                    ),
                    _ => (
                        "proxy_chain_profile_geo_failed",
                        "The full chain connected, but no live exit IP was returned.",
                    ),
                };
                self.exit_error_code = Some(code);
                self.exit_error = Some(message.into());
            }
        }
    }
}

/// The already resolved first hop is one explicit route, never a saved proxy
/// or an input to the profile-chain wrapper. Preserve its own credentials.
fn system_entry(hop: &ProxyHop) -> ProxyEntry {
    let host = if hop.host.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{}]", hop.host)
    } else {
        hop.host.clone()
    };
    ProxyEntry {
        id: String::new(),
        name: String::new(),
        kind: match hop.kind {
            connector::ProxyKind::Http => ProxyKind::Http,
            connector::ProxyKind::Socks5 => ProxyKind::Socks5,
        },
        host,
        port: hop.port,
        username: hop.username.clone(),
        password: hop.password.clone(),
        country: String::new(),
        notes: String::new(),
    }
}

pub async fn test(
    kind: String,
    config: ProxyChainSettings,
    profile_id: Option<String>,
) -> anyhow::Result<TestResult> {
    if !matches!(kind.as_str(), "local" | "system" | "profile") {
        anyhow::bail!("Unknown proxy test kind.");
    }
    let start = Instant::now();
    let mut result = TestResult {
        kind: kind.clone(),
        proxy_kind: None,
        proxy_address: None,
        source: None,
        system_proxy: Stage::pending(),
        profile_proxy: Stage::pending(),
        local_network: None,
        exit_ip: None,
        exit_geo: None,
        exit_error: None,
        exit_error_code: None,
        total_ms: 0,
        tested_at: format!(
            "@{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
        ),
    };
    if kind == "local" {
        match proxy::geo_check_on_route(None, None).await {
            Ok(geo) => {
                result.local_network = Some(Stage::ok(start));
                result.set_exit(Ok(geo));
            }
            Err(error) => {
                result.local_network = Some(Stage::failed(ChainError::new(
                    "proxy_chain_local_network_failed",
                    "local",
                    "The local network test did not complete.",
                )));
                result.set_exit(Err(error));
            }
        }
    } else {
        let saved = saved_settings()?;
        match config.hop(&saved.proxy_chain) {
            Err(e) => result.system_proxy = Stage::failed(e),
            Ok(hop) => {
                result.proxy_kind = Some(if hop.kind == connector::ProxyKind::Http {
                    "http"
                } else {
                    "socks5"
                });
                result.proxy_address = Some(format!("{}:{}", hop.host, hop.port));
                result.source = Some(if config.mode == "auto" {
                    "windows_current_user_internet_settings"
                } else {
                    "manual"
                });
                let entry = if kind == "profile" {
                    Some(crate::launch::proxy_for_test(
                        profile_id
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("Select a profile to test."))?,
                    )?)
                } else {
                    None
                };
                let target = if let Some(entry) = &entry {
                    Target::new(&entry.host, entry.port)?
                } else {
                    Target::new("example.com", 443)?
                };
                match connector::connect(&hop, &target).await {
                    Err(e) => result.system_proxy = Stage::failed(e),
                    Ok(stream) => {
                        drop(stream);
                        result.system_proxy = Stage::ok(start);
                        if let Some(entry) = entry {
                            match prepare_with_hop(&entry, hop).await {
                                Err(e) => {
                                    result.profile_proxy = Stage::failed(
                                        e.downcast::<ChainError>().unwrap_or_else(|_| {
                                            ChainError::new(
                                                "proxy_chain_profile_failed",
                                                "profile",
                                                "Could not prepare this profile proxy.",
                                            )
                                        }),
                                    )
                                }
                                Ok(prepared) => {
                                    let phase = Instant::now();
                                    if let Err(error) = proxy::probe_on_route(&prepared.effective).await {
                                        result.profile_proxy = Stage::failed(profile_failure(&prepared.tunnel, &error));
                                    } else {
                                        result.profile_proxy = Stage::ok(phase);
                                        result.set_exit(proxy::geo_check_on_route(Some(&prepared.effective), None).await);
                                    }
                                    prepared.tunnel.shutdown().await;
                                }
                            }
                        } else {
                            let entry = system_entry(&hop);
                            result.set_exit(proxy::geo_check_on_route(Some(&entry), None).await);
                        }
                    }
                }
            }
        }
    }
    result.total_ms = start.elapsed().as_millis();
    Ok(result)
}

#[cfg(test)]
mod arg_tests {
    #[test]
    fn conflicting_switches_cannot_override_the_chain() {
        for input in [
            "--proxy-server=x",
            "/NO-PROXY-SERVER",
            "-enable-quic",
            "--proxy-auto-detect",
            "--proxy-pac-url x",
            "--shardx-webrtc-policy=auto",
        ] {
            assert!(super::validate_args(input).is_err());
        }
        assert!(super::validate_args("--window-size=1280,800").is_ok());
    }
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod detection_tests;
#[cfg(all(test, windows))]
mod live_tests;
#[cfg(all(test, windows))]
mod live_geo_tests;
