//! Windows system proxy -> original profile proxy. Never fall back to direct.
pub mod config;
pub mod connector;
pub mod credentials;
pub mod detect_windows;
pub mod error;
pub mod policy;
pub mod route;
pub mod routing;
#[cfg(test)]
mod tests;
pub mod tunnel;
pub mod tls;

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
    pub route: Route,
    pub first_hop_error: Option<ChainError>,
    pub verified_geo: Option<proxy::GeoInfo>,
    pub verified_tcp_ms: Option<u128>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Route { FirstHop, AssignedProxyFallback, SameProxyEndpoint, ExplicitDirect, ExplicitSystem, ExplicitProxy, ExplicitChain }
impl Route {
    pub fn as_str(self) -> &'static str { match self { Self::FirstHop => "first_hop", Self::AssignedProxyFallback => "assigned_proxy_fallback", Self::SameProxyEndpoint => "same_proxy_endpoint", Self::ExplicitDirect => "explicit_direct", Self::ExplicitSystem => "explicit_system", Self::ExplicitProxy => "explicit_proxy", Self::ExplicitChain => "explicit_chain" } }
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
    prepare_with_saved(entry, config, config).await
}

async fn prepare_with_saved(entry: Option<&ProxyEntry>, config: &ProxyChainSettings, saved: &ProxyChainSettings) -> anyhow::Result<Option<PreparedProxy>> {
    if !config.enabled {
        return Ok(None);
    }
    #[cfg(not(windows))]
    anyhow::bail!("Proxy chaining is supported on Windows only.");
    let entry = entry.ok_or_else(|| {
        ChainError::new(
            "proxy_chain_profile_required",
            "profile",
            "This profile has no upstream proxy. Assign a proxy before starting the proxy chain.",
        )
    })?;
    let target = Target::new(&entry.host, entry.port)?;
    config.validate()?;
    let resolved = config.hop_for(saved, &target).await;
    if !config.allow_assigned_fallback {
        return Ok(Some(prepare_with_hop(entry, resolved?).await?));
    }
    let first_error = match resolved {
        Ok(hop) => {
            if same_endpoint(entry, &hop)? {
                return assigned_route(entry, Route::SameProxyEndpoint, None).await.map(Some);
            }
            match prepare_with_hop(entry, hop).await {
                Ok(mut prepared) => match verify_prepared(&mut prepared).await {
                    Ok(()) => return Ok(Some(prepared)),
                    Err(error) => { prepared.tunnel.shutdown().await; error }
                },
                Err(error) => safe_prepare_error(error),
            }
        }
        Err(error) => {
            // Corrupt or unreadable protection data is never a route failure.
            if matches!(error.code, "proxy_chain_credentials_unavailable" | "proxy_chain_invalid_config" | "proxy_chain_windows_required") { return Err(error.into()); }
            error
        }
    };
    assigned_route(entry, Route::AssignedProxyFallback, Some(first_error)).await.map(Some)
}

fn safe_prepare_error(error: anyhow::Error) -> ChainError {
    error.downcast::<ChainError>().unwrap_or_else(|_| ChainError::new("proxy_chain_profile_failed", "profile", "The first-hop route to the assigned proxy could not be prepared."))
}

fn same_endpoint(entry: &ProxyEntry, hop: &ProxyHop) -> Result<bool, ChainError> {
    let same_protocol = matches!((&entry.kind, hop.kind), (ProxyKind::Http, connector::ProxyKind::Http) | (ProxyKind::Https, connector::ProxyKind::Https) | (ProxyKind::Socks5, connector::ProxyKind::Socks5));
    Ok(same_protocol && Target::new(&entry.host, entry.port)? == Target::new(&hop.host, hop.port)?
        && entry.username == hop.username && entry.password == hop.password)
}

fn effective_entry(entry: &ProxyEntry, tunnel: &Tunnel) -> ProxyEntry {
    let mut effective = entry.clone();
    if effective.kind == ProxyKind::Https { effective.kind = ProxyKind::Http; }
    effective.host = "127.0.0.1".into();
    effective.port = tunnel.addr().port();
    effective.id.clear();
    effective
}

async fn verify_prepared(prepared: &mut PreparedProxy) -> Result<(), ChainError> {
    let tcp_ms = proxy::probe_on_route(&prepared.effective).await.map_err(|e| profile_failure(&prepared.tunnel, &e))?;
    let geo = proxy::geo_check_on_route(Some(&prepared.effective), None).await.map_err(|_| ChainError::new(
        "proxy_chain_profile_geo_failed", "exit", "The assigned proxy connected, but its live exit IP could not be verified on this route."))?;
    prepared.verified_tcp_ms = Some(tcp_ms);
    prepared.verified_geo = Some(geo);
    Ok(())
}

async fn assigned_route(entry: &ProxyEntry, route: Route, first_hop_error: Option<ChainError>) -> anyhow::Result<PreparedProxy> {
    let target = Target::new(&entry.host, entry.port)?;
    let tls = if entry.kind == ProxyKind::Https { Some(tls::client()?) } else { None };
    let tunnel = Tunnel::bind_assigned(target, tls).await?;
    let mut prepared = PreparedProxy { effective: effective_entry(entry, &tunnel), tunnel, route, first_hop_error, verified_geo: None, verified_tcp_ms: None };
    if verify_prepared(&mut prepared).await.is_err() {
        let first_hop_error_code = prepared.first_hop_error.as_ref().map(|e| e.code);
        prepared.tunnel.shutdown().await;
        let mut error = if route == Route::SameProxyEndpoint {
            ChainError::new("proxy_chain_same_proxy_failed", "profile", "The identical proxy endpoint was used once, but the assigned proxy handshake or live exit could not be verified. No website direct connection was attempted.")
        } else {
            ChainError::new("proxy_chain_routes_failed", "profile", "The first-hop route and the direct connection to the assigned proxy could not be verified. The assigned proxy was kept; no website direct connection was attempted.")
        };
        error.first_hop_error_code = first_hop_error_code;
        return Err(error.into());
    }
    Ok(prepared)
}

async fn prepare_with_hop(entry: &ProxyEntry, hop: ProxyHop) -> anyhow::Result<PreparedProxy> {
    let target = Target::new(&entry.host, entry.port)?;
    // Proves the first hop can reach the original upstream before binding.
    // It is not proof of second-hop authentication; callers also run probe.
    let tunnel = if entry.kind == ProxyKind::Https {
        let tls = tls::client()?;
        drop(tls::connect(&hop, &target, tls.clone(), connector::CONNECT_TIMEOUT).await?);
        Tunnel::bind_tls(hop, target, tls).await?
    } else {
        drop(connector::connect(&hop, &target).await?);
        Tunnel::bind(hop, target).await?
    };
    let effective = effective_entry(entry, &tunnel);
    Ok(PreparedProxy { effective, tunnel, route: Route::FirstHop, first_hop_error: None, verified_geo: None, verified_tcp_ms: None })
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
    tunnel.last_error().unwrap_or_else(|| ChainError::new("proxy_chain_profile_failed", "profile", "The profile proxy rejected authentication or could not reach the test target through the system proxy."))
}

pub fn profile_failure(tunnel: &Tunnel, cause: &anyhow::Error) -> ChainError {
    if let Some(error) = cause.downcast_ref::<proxy::HttpProbeError>() {
        return ChainError::new(if error.0 == 407 { "proxy_chain_profile_auth_failed" } else { "proxy_chain_profile_connect_failed" }, "profile", format!("The profile HTTP proxy refused CONNECT (status {}). Check its credentials and source restrictions.", error.0));
    }
    // A reply received on THIS probe is authoritative. Another concurrent
    // Geo stream's close/reset must not overwrite a definite second-hop reply.
    if let Some(error) = cause.downcast_ref::<proxy::Socks5ProbeError>() {
        return match error {
            proxy::Socks5ProbeError::AuthenticationRejected(status) => ChainError::new(
                "proxy_chain_profile_auth_failed", "profile",
                format!("The profile SOCKS5 proxy rejected authentication (status {status}). Check its credentials and allowed source IPs; system proxy credentials are separate."),
            ),
            proxy::Socks5ProbeError::ConnectRejected(status) => ChainError::new(
                "proxy_chain_profile_connect_failed", "profile",
                format!("The profile SOCKS5 proxy refused the destination CONNECT request (reply {status})."),
            ),
        };
    }
    // Classify our own probe labels; never return its raw error/URL to the UI.
    let text = cause.to_string().to_ascii_lowercase();
    if !text.contains("rejected the username") && !text.contains("login method") {
        if let Some(error) = tunnel.last_error() { return error; }
    }
    if text.contains("time") || text.contains("never answered") || text.contains("no answer") {
        ChainError::new("proxy_chain_profile_timeout", "profile", "The profile proxy did not finish its handshake or CONNECT in time.")
    } else if text.contains("username") || text.contains("password") || text.contains("login method") {
        ChainError::new("proxy_chain_profile_auth_failed", "profile", "The profile proxy rejected authentication. Check its credentials and allowed source IPs; system proxy credentials are separate.")
    } else {
        ChainError::new("proxy_chain_profile_connect_failed", "profile", "The profile proxy refused or could not complete the destination CONNECT request.")
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
    resolved_for: Option<String>,
    profile_proxy_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<Route>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_hop_error_code: Option<&'static str>,
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
        proxy_chain_policy: Default::default(),
        id: String::new(),
        name: String::new(),
        kind: match hop.kind {
            connector::ProxyKind::Http => ProxyKind::Http,
            connector::ProxyKind::Https => ProxyKind::Https,
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
        resolved_for: None,
        profile_proxy_address: None,
        route: None,
        first_hop_error_code: None,
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
        let entry = if kind == "profile" || profile_id.is_some() {
            Some(crate::launch::proxy_for_test(
                profile_id.as_deref().ok_or_else(|| anyhow::anyhow!("Select a profile to test."))?,
            )?)
        } else {
            None
        };
        if kind == "profile" {
            let policy = entry.as_ref().map(|p| p.proxy_chain_policy).unwrap_or_default();
            let effective = policy::effective(&config, Default::default(), policy)?;
            if effective.allow_assigned_fallback {
                let entry = entry.as_ref().expect("profile entry loaded");
                result.profile_proxy_address = Some(format!("{}:{}", entry.host, entry.port));
                match prepare_with_saved(Some(entry), &effective, &saved.proxy_chain).await {
                    Ok(Some(prepared)) => {
                        result.route = Some(prepared.route);
                        result.source = Some(match prepared.route { Route::FirstHop => "preferred_first_hop", route => route.as_str() });
                        result.first_hop_error_code = prepared.first_hop_error.as_ref().map(|e| e.code);
                        result.system_proxy = match prepared.first_hop_error.clone() { Some(error) => Stage::failed(error), None if prepared.route == Route::FirstHop => Stage::ok(start), None => Stage::pending() };
                        result.profile_proxy = Stage::ok(start);
                        result.set_exit(Ok(prepared.verified_geo.clone().expect("preferred routes are verified")));
                        prepared.tunnel.shutdown().await;
                    },
                    Err(error) => {
                        let error = safe_prepare_error(error);
                        result.first_hop_error_code = error.first_hop_error_code;
                        result.profile_proxy = Stage::failed(error);
                    },
                    Ok(None) => unreachable!("preferred route stays protected"),
                }
                result.total_ms = start.elapsed().as_millis();
                return Ok(result);
            }
            if !effective.enabled {
                let entry = entry.as_ref().expect("profile entry loaded");
                result.profile_proxy_address = Some(format!("{}:{}", entry.host, entry.port));
                result.source = Some("profile_proxy_without_first_hop");
                let phase = Instant::now();
                match proxy::probe_on_route(entry).await {
                    Ok(_) => { result.profile_proxy = Stage::ok(phase); result.set_exit(proxy::geo_check_on_route(Some(entry), None).await); },
                    Err(_) => result.profile_proxy = Stage::failed(ChainError::new("proxy_chain_profile_connect_failed", "profile", "The profile proxy failed on the explicitly selected route without the first hop.")),
                }
                result.total_ms = start.elapsed().as_millis();
                return Ok(result);
            }
        }
        let target = if let Some(entry) = &entry {
            result.profile_proxy_address = Some(format!("{}:{}", entry.host, entry.port));
            Target::new(&entry.host, entry.port)?
        } else {
            Target::new("example.com", 443)?
        };
        match config.resolve_for(&saved.proxy_chain, &target).await {
            Err(e) => result.system_proxy = Stage::failed(e),
            Ok(resolved) => {
                result.source = Some(resolved.source);
                result.resolved_for = resolved.resolved_for;
                let hop = resolved.hop;
                result.proxy_kind = Some(if hop.kind == connector::ProxyKind::Http {
                    "http"
                } else {
                    "socks5"
                });
                result.proxy_address = Some(format!("{}:{}", hop.host, hop.port));
                match connector::connect(&hop, &target).await {
                    Err(e) => result.system_proxy = Stage::failed(e),
                    Ok(stream) => {
                        drop(stream);
                        result.system_proxy = Stage::ok(start);
                        if let Some(entry) = entry.filter(|_| kind == "profile") {
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
                                    result.route = Some(prepared.route);
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
