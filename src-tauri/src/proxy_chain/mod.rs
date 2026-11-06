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
#[cfg(test)]
mod same_exit_tests;
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
    /// Measured reachability latency. A live Geo response on the same route
    /// also proves TCP transport when the dedicated probe target is unavailable.
    pub verified_tcp_ms: Option<u128>,
}

/// Internal evidence from a strict preparation that reached the proxy but
/// could not obtain its live exit. It is not a ready route or public error DTO.
#[derive(Debug, Clone)]
struct VerifiedRouteFailure {
    error: ChainError,
    route: Route,
    tcp_ms: u128,
    first_hop_error: Option<ChainError>,
}

impl std::fmt::Display for VerifiedRouteFailure {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {std::fmt::Display::fmt(&self.error,f)}
}

fn preparation_failure(prepared:&PreparedProxy,error:ChainError)->anyhow::Error {
    let cause=anyhow::Error::new(error.clone());
    match prepared.verified_tcp_ms {
        Some(tcp_ms)=>cause.context(VerifiedRouteFailure{error,route:prepared.route,tcp_ms,first_hop_error:prepared.first_hop_error.clone()}),
        None=>cause,
    }
}

pub(crate) fn partial_verification(error:&anyhow::Error)->Option<(ChainError,Route,u128)> {
    error.downcast_ref::<VerifiedRouteFailure>().map(|verified|(verified.error.clone(),verified.route,verified.tcp_ms))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Route { FirstHop, AssignedProxyFallback, SameProxyEndpoint, SameExitVerified, ExplicitDirect, ExplicitSystem, ExplicitProxy, ExplicitChain }
impl Route {
    pub fn as_str(self) -> &'static str { match self { Self::FirstHop => "first_hop", Self::AssignedProxyFallback => "assigned_proxy_fallback", Self::SameProxyEndpoint => "same_proxy_endpoint", Self::SameExitVerified => "same_exit_verified", Self::ExplicitDirect => "explicit_direct", Self::ExplicitSystem => "explicit_system", Self::ExplicitProxy => "explicit_proxy", Self::ExplicitChain => "explicit_chain" } }
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
        let hop=resolved?;
        if same_endpoint(entry,&hop)? {
            return assigned_route(entry,Route::SameProxyEndpoint,None).await.map(Some);
        }
        return prepare_checked_chain(entry,hop).await.map(Some);
    }
    let first_error = match resolved {
        Ok(hop) => {
            if same_endpoint(entry, &hop)? {
                return assigned_route(entry, Route::SameProxyEndpoint, None).await.map(Some);
            }
            match prepare_checked_chain(entry, hop).await {
                Ok(prepared) => return Ok(Some(prepared)),
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

pub(crate) async fn verify_prepared(prepared: &mut PreparedProxy) -> Result<(), ChainError> {
    prepared.verified_tcp_ms=None;
    prepared.verified_geo=None;
    let probe = proxy::probe_on_route(&prepared.effective).await
        .map_err(|cause| profile_failure(&prepared.tunnel, &cause));
    if let Err(error) = &probe {
        // An explicit authentication refusal must never be hidden by another
        // connection. An unavailable probe destination, however, says nothing
        // about whether this same proxy can carry a real Geo request.
        if matches!(error.code, "proxy_chain_profile_auth_failed" | "system_proxy_auth_failed") {
            return Err(error.clone());
        }
    }
    prepared.verified_tcp_ms=probe.as_ref().ok().copied();
    let geo_start=Instant::now();
    match proxy::geo_check_on_route(Some(&prepared.effective), None).await {
        Ok(geo)=>{
            // This is fresh traffic through the exact prepared relay, using
            // its original credentials. It is neither cached nor a new route.
            prepared.verified_tcp_ms=prepared.verified_tcp_ms.or(Some(geo_start.elapsed().as_millis()));
            prepared.verified_geo=Some(geo);
            Ok(())
        },
        Err(cause)=>{
            let mut error=probe.err().unwrap_or_else(||ChainError::new("proxy_chain_profile_geo_failed", "exit", "The assigned proxy connected, but its live exit IP could not be verified on this route."));
            error.geo_failures=cause.downcast_ref::<proxy::GeoLookupError>().map(|error|error.failures.clone());
            Err(error)
        },
    }
}

async fn assigned_route(entry: &ProxyEntry, route: Route, first_hop_error: Option<ChainError>) -> anyhow::Result<PreparedProxy> {
    let target = Target::new(&entry.host, entry.port)?;
    let tls = if entry.kind == ProxyKind::Https { Some(tls::client()?) } else { None };
    let tunnel = Tunnel::bind_assigned(target, tls).await?;
    let mut prepared = PreparedProxy { effective: effective_entry(entry, &tunnel), tunnel, route, first_hop_error, verified_geo: None, verified_tcp_ms: None };
    if let Err(cause)=verify_prepared(&mut prepared).await {
        let first_hop_error_code = prepared.first_hop_error.as_ref().map(|e| e.code);
        let mut error = if route == Route::SameProxyEndpoint {
            ChainError::new("proxy_chain_same_proxy_failed", "profile", "The identical proxy endpoint was used once, but the assigned proxy handshake or live exit could not be verified. No website direct connection was attempted.")
        } else {
            ChainError::new("proxy_chain_routes_failed", "profile", "The first-hop route and the direct connection to the assigned proxy could not be verified. The assigned proxy was kept; no website direct connection was attempted.")
        };
        error.first_hop_error_code = first_hop_error_code;
        error.geo_failures=cause.geo_failures;
        let error=preparation_failure(&prepared,error);
        prepared.tunnel.shutdown().await;
        return Err(error);
    }
    Ok(prepared)
}

async fn prepare_with_hop(entry: &ProxyEntry, hop: ProxyHop) -> anyhow::Result<PreparedProxy> {
    let target = Target::new(&entry.host, entry.port)?;
    // Verify through the local relay, not a separate connection that is
    // discarded before the actual proxy handshake begins.
    let tunnel = if entry.kind == ProxyKind::Https {
        let tls = tls::client()?;
        Tunnel::bind_tls(hop, target, tls).await?
    } else {
        Tunnel::bind(hop, target).await?
    };
    let effective = effective_entry(entry, &tunnel);
    Ok(PreparedProxy { effective, tunnel, route: Route::FirstHop, first_hop_error: None, verified_geo: None, verified_tcp_ms: None })
}

/// The caller has already resolved one first hop for the assigned endpoint.
/// Equal fresh exits are an optimisation, not proof of equal node identity.
/// The retained single route always uses the original assigned credentials;
/// it never becomes a mutable system-only route or a website direct route.
pub(super) async fn prepare_checked_chain(entry: &ProxyEntry, hop: ProxyHop) -> anyhow::Result<PreparedProxy> {
    prepare_checked_chain_with_budget(entry, hop, std::time::Duration::from_secs(6)).await
}

async fn prepare_checked_chain_with_budget(entry: &ProxyEntry, hop: ProxyHop, comparison_budget: std::time::Duration) -> anyhow::Result<PreparedProxy> {
    let mut chained=prepare_with_hop(entry,hop.clone()).await?;
    let verified=verify_prepared(&mut chained).await;
    let deadline=tokio::time::Instant::now()+comparison_budget;
    match verified {
        Ok(())=>{
            let expected=chained.verified_geo.as_ref().expect("verified chain exit").ip.clone();
            if let Some(single)=try_same_exit_single(entry,&hop,Some(&expected),deadline).await {
                chained.tunnel.shutdown().await;
                Ok(single)
            } else {Ok(chained)}
        },
        Err(error)=>{
            let recoverable=!matches!(error.code,"proxy_chain_profile_auth_failed"|"system_proxy_auth_failed")
                && !matches!(error.layer,"configuration"|"exit");
            let failure=preparation_failure(&chained,error);
            chained.tunnel.shutdown().await;
            if recoverable {
                if let Some(single)=try_same_exit_single(entry,&hop,None,deadline).await {return Ok(single);}
            }
            Err(failure)
        },
    }
}

/// Cancellation only borrows an owned candidate. Every failed or expired
/// candidate is explicitly shut down before returning, including active I/O.
async fn assigned_candidate_before(entry:&ProxyEntry,deadline:tokio::time::Instant) -> Option<PreparedProxy> {
    if tokio::time::Instant::now()>=deadline {return None;}
    let target=Target::new(&entry.host,entry.port).ok()?;
    let tls=if entry.kind==ProxyKind::Https {Some(tls::client().ok()?)} else {None};
    let tunnel=Tunnel::bind_assigned(target,tls).await.ok()?;
    let mut prepared=PreparedProxy{effective:effective_entry(entry,&tunnel),tunnel,route:Route::ExplicitProxy,first_hop_error:None,verified_geo:None,verified_tcp_ms:None};
    if matches!(tokio::time::timeout_at(deadline,verify_prepared(&mut prepared)).await,Ok(Ok(()))) {Some(prepared)}
    else {prepared.tunnel.shutdown().await;None}
}

async fn try_same_exit_single(entry:&ProxyEntry,hop:&ProxyHop,expected:Option<&str>,deadline:tokio::time::Instant) -> Option<PreparedProxy> {
    // Use the frozen hop itself. Resolving Windows/PAC again for a Geo URL
    // could select a different first hop than the assigned endpoint did.
    let first=assigned_candidate_before(&system_entry(hop),deadline).await?;
    let first_ip=first.verified_geo.as_ref().expect("verified first-hop exit").ip.clone();
    first.tunnel.shutdown().await;
    if expected.is_some_and(|ip|!same_exit_ip(ip,&first_ip)) {return None;}
    let mut assigned=assigned_candidate_before(entry,deadline).await?;
    if assigned.verified_geo.as_ref().is_some_and(|geo|same_exit_ip(&geo.ip,&first_ip)) {
        assigned.route=Route::SameExitVerified;
        Some(assigned)
    } else {assigned.tunnel.shutdown().await;None}
}

fn same_exit_ip(left:&str,right:&str)->bool {
    match (left.parse::<std::net::IpAddr>(),right.parse::<std::net::IpAddr>()) {
        (Ok(left),Ok(right))=>left==right,
        _=>false,
    }
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
        return ChainError::new(if error.0 == 407 { "proxy_chain_profile_auth_failed" } else { "proxy_chain_profile_connect_failed" }, "profile", format!("The profile HTTP proxy refused CONNECT (status {}). Check its credentials and source restrictions.", error.0))
            .with_proxy_response("http",if error.0==407 {"authentication"} else {"connect"},error.0,if error.0==407 {None} else {Some("example.com:443")});
    }
    // A reply received on THIS probe is authoritative. Another concurrent
    // Geo stream's close/reset must not overwrite a definite second-hop reply.
    if let Some(error) = cause.downcast_ref::<proxy::Socks5ProbeError>() {
        return match error {
            proxy::Socks5ProbeError::AuthenticationRejected(status) => ChainError::new(
                "proxy_chain_profile_auth_failed", "profile",
                format!("The profile SOCKS5 proxy rejected authentication (status {status}). Check its credentials and allowed source IPs; system proxy credentials are separate."),
            ).with_proxy_response("socks5","authentication",(*status).into(),None),
            proxy::Socks5ProbeError::ConnectRejected(status) => ChainError::new(
                "proxy_chain_profile_connect_failed", "profile",
                format!("The profile SOCKS5 proxy refused the destination CONNECT request (reply {status})."),
            ).with_proxy_response("socks5","connect",(*status).into(),Some("example.com:443")),
        };
    }
    // Classify our own probe labels; never return its raw error/URL to the UI.
    let text = cause.to_string().to_ascii_lowercase();
    if !text.contains("rejected the username") && !text.contains("login method") {
        if let Some(error) = tunnel.last_error() { return error; }
    }
    if cause.chain().any(|source| source.downcast_ref::<tokio::time::error::Elapsed>().is_some())
        || text.contains("time") || text.contains("never answered") || text.contains("no answer") {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    error_details: Option<error::ProxyFailureDetails>,
}
impl Stage {
    fn pending() -> Self {
        Self {
            state: "not_tested",
            latency_ms: None,
            error_code: None,
            error_message: None,
            error_details: None,
        }
    }
    fn ok(start: Instant) -> Self {
        Self::measured(start.elapsed().as_millis())
    }
    fn measured(latency_ms:u128)->Self {
        Self {
            state: "ok",
            latency_ms: Some(latency_ms),
            error_code: None,
            error_message: None,
            error_details: None,
        }
    }
    fn failed(e: ChainError) -> Self {
        Self {
            state: "failed",
            latency_ms: None,
            error_code: Some(e.code),
            error_message: Some(e.message),
            error_details: e.details,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_error_details: Option<Vec<proxy::GeoProviderFailure>>,
    total_ms: u128,
    tested_at: String,
}

impl TestResult {
    fn set_exit(&mut self, geo: anyhow::Result<proxy::GeoInfo>) {
        match geo {
            Ok(geo) => {
                self.exit_error=None;
                self.exit_error_code=None;
                self.exit_error_details=None;
                self.exit_ip = Some(geo.ip.clone());
                self.exit_geo = Some(geo);
            }
            Err(error) => {
                self.exit_error_details=error.downcast_ref::<proxy::GeoLookupError>().map(|error|error.failures.clone())
                    .or_else(||error.downcast_ref::<ChainError>().and_then(|error|error.geo_failures.clone()));
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
        exit_error_details: None,
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
                let mut tested=routing::test_resolved_route(Some(&route::ProfileProxyRoute::Proxy),policy::ChainPolicy::Disabled,Some(entry),None).await?;
                tested.source=Some("profile_proxy_without_first_hop");
                tested.total_ms=start.elapsed().as_millis();
                return Ok(tested);
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
                if let Some(entry) = entry.filter(|_| kind == "profile") {
                    let same=same_endpoint(&entry,&hop)?;
                    let prepared=if same {assigned_route(&entry,Route::SameProxyEndpoint,None).await}
                        else {prepare_checked_chain(&entry,hop).await};
                    match prepared {
                        Err(error) => {
                            if let Some(verified)=error.downcast_ref::<VerifiedRouteFailure>() {
                                result.route=Some(verified.route);
                                if verified.route==Route::FirstHop {result.system_proxy=Stage::measured(verified.tcp_ms);}
                                result.profile_proxy=Stage::measured(verified.tcp_ms);
                                result.set_exit(Err(verified.error.clone().into()));
                            } else {
                                let error=safe_prepare_error(error);
                                if error.layer=="system_proxy" {result.system_proxy=Stage::failed(error);}
                                else {if !same {result.system_proxy=Stage::ok(start);}result.profile_proxy=Stage::failed(error);}
                            }
                        },
                        Ok(prepared) => {
                            result.route = Some(prepared.route);
                            let ms=prepared.verified_tcp_ms.expect("route reachability verified");
                            if prepared.route==Route::FirstHop {result.system_proxy=Stage::measured(ms);}
                            else {
                                result.source=Some(prepared.route.as_str());
                                result.system_proxy=Stage::pending();
                            }
                            result.profile_proxy=Stage::measured(ms);
                            result.set_exit(Ok(prepared.verified_geo.clone().expect("route exit verified")));
                            prepared.tunnel.shutdown().await;
                        },
                    }
                } else {
                    match connector::connect(&hop, &target).await {
                        Err(error) if error.code=="system_proxy_auth_failed"=>result.system_proxy=Stage::failed(error),
                        Err(error)=>{
                            let entry=system_entry(&hop);
                            let phase=Instant::now();
                            let geo=proxy::geo_check_on_route(Some(&entry),None).await;
                            result.system_proxy=if geo.is_ok(){Stage::ok(phase)}else{Stage::failed(error)};
                            result.set_exit(geo);
                        },
                        Ok(stream)=>{
                            drop(stream);
                            result.system_proxy=Stage::ok(start);
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
