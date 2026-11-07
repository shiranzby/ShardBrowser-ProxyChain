//! Explicit routes are resolved once per launch. The assigned profile binding
//! stays untouched; neither the global switch nor a failed hop can change it.
use super::{*, route::{FirstSource, ProfileProxyRoute, ProxySource, RoutePlan}};

pub struct ExplicitRoute {
    pub proxy: Option<ProxyEntry>,
    pub prepared: Option<PreparedProxy>,
}

/// One compatibility boundary for profile intent. Launch and detection use
/// the same resolved policy; the old wire remains unchanged on disk.
pub struct ProfilePlan {
    pub route: Option<RoutePlan>,
    pub config: ProxyChainSettings,
}

pub fn profile_plan(route:Option<&ProfileProxyRoute>,profile_policy:policy::ChainPolicy,assigned:Option<&ProxyEntry>,config:&ProxyChainSettings) -> anyhow::Result<ProfilePlan> {
    profile_policy.validate()?;
    let route=route.map(ProfileProxyRoute::plan).transpose()?;
    let config=if let Some(route)=route {
        let mut config=config.clone();
        config.enabled=route!=RoutePlan::Direct;
        config.allow_assigned_fallback=profile_policy==policy::ChainPolicy::PreferFirstHop;
        config
    } else {policy::effective(config,profile_policy,assigned.map(|proxy|proxy.proxy_chain_policy).unwrap_or_default())?};
    Ok(ProfilePlan{route,config})
}

impl ProfilePlan {
    pub async fn prepare(&self,assigned:Option<&ProxyEntry>,first:Option<&ProxyEntry>,saved:&ProxyChainSettings) -> anyhow::Result<ExplicitRoute> {
        if let Some(plan)=self.route {return prepare_plan(plan,assigned,first,self.config.allow_assigned_fallback).await;}
        let prepared=super::prepare_with_saved(assigned,&self.config,saved).await?;
        let proxy=prepared.as_ref().map(|prepared|prepared.effective.clone()).or_else(||assigned.cloned());
        Ok(ExplicitRoute{proxy,prepared})
    }
}

fn required(entry: Option<&ProxyEntry>) -> anyhow::Result<&ProxyEntry> {
    entry.ok_or_else(|| ChainError::new("proxy_chain_profile_required", "profile", "Select the proxy required by this explicit route before testing or starting.").into())
}

fn hop(entry: &ProxyEntry) -> Result<ProxyHop, ChainError> {
    let hop=ProxyHop { kind:match entry.kind { ProxyKind::Http=>connector::ProxyKind::Http, ProxyKind::Https=>connector::ProxyKind::Https, ProxyKind::Socks5=>connector::ProxyKind::Socks5 },
        host:entry.host.clone(),port:entry.port,username:entry.username.clone(),password:entry.password.clone() };
    connector::validate(&hop,&Target::new("example.com",443)?)?;
    Ok(hop)
}

pub fn is_direct(route: &ProfileProxyRoute) -> bool {
    route.plan().is_ok_and(|plan| plan == RoutePlan::Direct)
}

async fn system() -> anyhow::Result<PreparedProxy> {
    let tunnel=Tunnel::bind_system().await?;
    let effective=ProxyEntry {proxy_chain_policy:Default::default(),id:String::new(),name:String::new(),kind:ProxyKind::Socks5,host:"127.0.0.1".into(),port:tunnel.addr().port(),username:String::new(),password:String::new(),country:String::new(),notes:String::new()};
    let mut prepared=PreparedProxy{effective,tunnel,route:Route::ExplicitSystem,first_hop_error:None,verified_geo:None,verified_https:false,geo_failure:None,verified_tcp_ms:None};
    if let Err(error)=verify_prepared(&mut prepared).await {
        let error=if prepared.verified_tcp_ms.is_some(){error}else{prepared.tunnel.last_error().unwrap_or(error)};
        let error=preparation_failure(&prepared,error);prepared.tunnel.shutdown().await;return Err(error);
    }
    Ok(prepared)
}

pub async fn prepare_route(route:&ProfileProxyRoute,assigned:Option<&ProxyEntry>,first:Option<&ProxyEntry>,allow_assigned_fallback:bool) -> anyhow::Result<ExplicitRoute> {
    prepare_plan(route.plan()?,assigned,first,allow_assigned_fallback).await
}

async fn prepare_plan(plan:RoutePlan,assigned:Option<&ProxyEntry>,first:Option<&ProxyEntry>,allow_assigned_fallback:bool) -> anyhow::Result<ExplicitRoute> {
    if plan == RoutePlan::Direct {return Ok(ExplicitRoute{proxy:None,prepared:None});}
    let prepared=match plan {
        RoutePlan::Direct=>unreachable!("direct route handled above"),
        RoutePlan::System=>system().await?,
        RoutePlan::Single{source}=>{
            let entry=required(match source {ProxySource::Assigned=>assigned,ProxySource::SelectedFirst=>first})?;
            super::prepare_single(entry,Route::ExplicitProxy,None).await?
        },
        RoutePlan::Chain{first:chosen}=>{
            let assigned=required(assigned)?;
            let target=Target::new(&assigned.host,assigned.port)?;
            // Missing library entries and invalid configuration are not network
            // failures and cannot be hidden by the optional assigned fallback.
            let selected=match chosen {FirstSource::System=>None,FirstSource::SelectedFirst=>Some(hop(required(first)?)?)};
            let attempt=async {
                let first=match selected {Some(first)=>first,None=>detect_windows::resolve_for(&target).await?.hop};
                if same_endpoint(assigned,&first)? {return assigned_route(assigned,Route::SameProxyEndpoint,None).await;}
                let mut prepared=prepare_checked_chain(assigned,first).await.map_err(|mut error|{
                    if let Some(verified)=error.downcast_mut::<VerifiedRouteFailure>() {
                        if verified.route==Route::FirstHop {verified.route=Route::ExplicitChain;}
                    }
                    error
                })?;
                if prepared.route==Route::FirstHop {prepared.route=Route::ExplicitChain;}
                Ok(prepared)
            }.await;
            match attempt {
                Ok(prepared)=>prepared,
                Err(error)=>{
                    let verified=error.downcast_ref::<VerifiedRouteFailure>().cloned();
                    let error=safe_prepare_error(error);
                    if !allow_assigned_fallback || matches!(error.code,"proxy_chain_invalid_config"|"proxy_chain_credentials_unavailable"|"proxy_chain_windows_required"|"proxy_chain_same_proxy_failed") {
                        return Err(match verified {Some(verified)=>anyhow::Error::new(error).context(verified),None=>error.into()});
                    }
                    assigned_route(assigned,Route::AssignedProxyFallback,Some(error)).await?
                }
            }
        },
    };
    Ok(ExplicitRoute{proxy:Some(prepared.effective.clone()),prepared:Some(prepared)})
}

fn record_connected_route(result:&mut TestResult,plan:Option<RoutePlan>,first:Option<&ProxyEntry>,actual:Route,first_error:Option<ChainError>,tcp_ms:u128) {
    result.route=Some(actual);result.source=Some(actual.as_str());result.first_hop_error_code=first_error.as_ref().map(|error|error.code);
    let first_only=plan.is_some_and(RoutePlan::first_only);
    let first_used=plan.is_some_and(RoutePlan::uses_first);
    result.system_proxy=if let Some(error)=first_error {Stage::failed(error)} else if matches!(actual,Route::SameProxyEndpoint|Route::SameExitVerified) {Stage::pending()} else if first_used || plan.is_none()&&actual==Route::FirstHop {Stage::measured(tcp_ms)} else {Stage::pending()};
    result.profile_proxy=if first_only {Stage::pending()} else {Stage::measured(tcp_ms)};
    if first_only {result.profile_proxy_address=None;}
    if let Some(first)=first.filter(|_|first_used) {result.proxy_address=Some(format!("{}:{}",first.host,first.port));result.proxy_kind=Some(match first.kind {ProxyKind::Http=>"http",ProxyKind::Https=>"https",ProxyKind::Socks5=>"socks5"});}
}

/// The caller resolves ids under the current space and its permissions. This
/// function performs no writes, starts no browser and has no saved-test cache.
pub async fn test_resolved_route(route:Option<&ProfileProxyRoute>,policy:policy::ChainPolicy,assigned:Option<&ProxyEntry>,first:Option<&ProxyEntry>) -> anyhow::Result<TestResult> {
    let saved=if route.is_none(){saved_settings()?.proxy_chain}else{ProxyChainSettings::default()};
    test_with_config(route,policy,assigned,first,&saved,&saved).await
}

/// The settings dialog supplies an unsaved draft. Saved settings are used only
/// to resolve protected credentials, never to replace that draft's route.
pub(super) async fn test_with_config(route:Option<&ProfileProxyRoute>,policy:policy::ChainPolicy,assigned:Option<&ProxyEntry>,first:Option<&ProxyEntry>,config:&ProxyChainSettings,saved:&ProxyChainSettings) -> anyhow::Result<TestResult> {
    let compiled=profile_plan(route,policy,assigned,config)?;
    let plan=compiled.route;
    let start=Instant::now();
    let mut result=TestResult{kind:"profile".into(),proxy_kind:None,proxy_address:None,source:None,resolved_for:None,profile_proxy_address:assigned.map(|p|format!("{}:{}",p.host,p.port)),route:None,route_ready:None,first_hop_error_code:None,system_proxy:Stage::pending(),profile_proxy:Stage::pending(),local_network:None,exit_ip:None,exit_geo:None,exit_error:None,exit_error_code:None,exit_error_details:None,total_ms:0,tested_at:format!("@{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs())};
    // The selected endpoint is useful evidence even when its handshake fails.
    // Do not leave the first-hop address dependent on a successful result.
    if plan.is_some_and(RoutePlan::uses_selected_first) {
        if let Some(first)=first {result.proxy_address=Some(format!("{}:{}",first.host,first.port));result.proxy_kind=Some(match first.kind {ProxyKind::Http=>"http",ProxyKind::Https=>"https",ProxyKind::Socks5=>"socks5"});}
    }
    if plan==Some(RoutePlan::Single{source:ProxySource::SelectedFirst}) {
        result.profile_proxy_address=first.map(|proxy|format!("{}:{}",proxy.host,proxy.port));
    }
    if plan.is_some_and(RoutePlan::first_only) {result.profile_proxy_address=None;}
    if plan==Some(RoutePlan::Direct) {
        result.kind="local".into();result.route=Some(Route::ExplicitDirect);result.source=Some("explicit_direct");
        let geo=proxy::geo_check_on_route(None,None).await;
        result.local_network=Some(if geo.is_ok(){Stage::ok(start)}else{Stage::failed(ChainError::new("proxy_chain_local_network_failed","local","The explicitly selected direct connection did not return a live exit."))});
        result.set_exit(geo);
    } else {
        let prepared=match compiled.prepare(assigned,first,saved).await {
            Ok(resolved)=>match (resolved.prepared,resolved.proxy) {
                (Some(prepared),_)=>Ok(Some(prepared)),
                // A legacy non-chain launch retains its native proxy behavior.
                // Its test still verifies that exact proxy through the common
                // single-proxy relay, without changing the saved route.
                (None,Some(proxy))=>super::prepare_single(&proxy,Route::ExplicitProxy,None).await.map(Some),
                (None,None)=>Ok(None),
            },
            Err(error)=>Err(error),
        };
        match prepared {
            Ok(Some(mut prepared))=>{
                if !prepared.is_ready() {if let Err(error)=verify_prepared(&mut prepared).await {
                    if let Some(tcp_ms)=prepared.verified_tcp_ms {
                        record_connected_route(&mut result,plan,first,prepared.route,prepared.first_hop_error.clone(),tcp_ms);
                        result.set_exit(Err(error.into()));
                    } else if error.layer=="system_proxy" || error.layer=="configuration" {
                        result.system_proxy=Stage::failed(error);
                    } else {result.profile_proxy=Stage::failed(error);}
                    prepared.tunnel.shutdown().await;result.total_ms=start.elapsed().as_millis();return Ok(result);
                }}
                record_connected_route(&mut result,plan,first,prepared.route,prepared.first_hop_error.clone(),prepared.verified_tcp_ms.expect("route TCP verified"));
                result.route_ready=Some(prepared.is_ready());
                result.set_exit(prepared.exit_result());
                prepared.tunnel.shutdown().await;
            },
            Err(error)=>{
                if let Some(verified)=error.downcast_ref::<VerifiedRouteFailure>() {
                    record_connected_route(&mut result,plan,first,verified.route,verified.first_hop_error.clone(),verified.tcp_ms);
                    result.set_exit(Err(verified.error.clone().into()));
                    result.total_ms=start.elapsed().as_millis();return Ok(result);
                }
                let error=safe_prepare_error(error);
                let first_only=plan.is_some_and(RoutePlan::first_only);
                result.first_hop_error_code=error.first_hop_error_code;
                if let Some(code)=error.first_hop_error_code {
                    result.system_proxy=Stage::failed(ChainError::new(code,"system_proxy","The first-hop route failed before the assigned-proxy fallback also failed."));
                    result.profile_proxy=Stage::failed(error);
                } else if first_only || matches!(error.layer,"system_proxy"|"configuration") {result.system_proxy=Stage::failed(error);} else {
                    // An endpoint skipped by de-duplication, a missing assigned
                    // proxy, or a combined-route failure proves no first-hop pass.
                    if !matches!(error.code,"proxy_chain_same_proxy_failed"|"proxy_chain_routes_failed"|"proxy_chain_profile_required")
                        && matches!(plan,Some(RoutePlan::Chain{..})) {result.system_proxy=Stage::ok(start);}
                    result.profile_proxy=Stage::failed(error);
                }
            },
            Ok(None)=>{
                result.source=Some("legacy_direct");
                let geo=proxy::geo_check_on_route(None,None).await;
                result.local_network=Some(if geo.is_ok(){Stage::ok(start)}else{Stage::failed(ChainError::new("proxy_chain_local_network_failed","local","The local network test did not complete."))});
                result.set_exit(geo);
            },
        }
    }
    result.total_ms=start.elapsed().as_millis();Ok(result)
}
