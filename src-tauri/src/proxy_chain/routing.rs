//! Explicit routes are resolved once per launch. The assigned profile binding
//! stays untouched; neither the global switch nor a failed hop can change it.
use super::{*, route::{FirstHop, ProfileProxyRoute, SecondHop}};

pub struct ExplicitRoute {
    pub proxy: Option<ProxyEntry>,
    pub prepared: Option<PreparedProxy>,
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
    matches!(route,ProfileProxyRoute::Direct|ProfileProxyRoute::Chain{first:FirstHop::Direct,second:SecondHop::Direct})
}

async fn single(entry: &ProxyEntry, route: Route) -> anyhow::Result<PreparedProxy> {
    let target=Target::new(&entry.host,entry.port)?;
    let tls=if entry.kind==ProxyKind::Https {Some(tls::client()?)} else {None};
    let tunnel=Tunnel::bind_assigned(target,tls).await?;
    let mut prepared=PreparedProxy{effective:effective_entry(entry,&tunnel),tunnel,route,first_hop_error:None,verified_geo:None,verified_tcp_ms:None};
    if let Err(error)=verify_prepared(&mut prepared).await {prepared.tunnel.shutdown().await;return Err(error.into());}
    Ok(prepared)
}

async fn system() -> anyhow::Result<PreparedProxy> {
    let tunnel=Tunnel::bind_system().await?;
    let effective=ProxyEntry {proxy_chain_policy:Default::default(),id:String::new(),name:String::new(),kind:ProxyKind::Socks5,host:"127.0.0.1".into(),port:tunnel.addr().port(),username:String::new(),password:String::new(),country:String::new(),notes:String::new()};
    let mut prepared=PreparedProxy{effective,tunnel,route:Route::ExplicitSystem,first_hop_error:None,verified_geo:None,verified_tcp_ms:None};
    if let Err(error)=verify_prepared(&mut prepared).await {let error=prepared.tunnel.last_error().unwrap_or(error);prepared.tunnel.shutdown().await;return Err(error.into());}
    Ok(prepared)
}

pub async fn prepare_route(route:&ProfileProxyRoute,assigned:Option<&ProxyEntry>,first:Option<&ProxyEntry>,allow_assigned_fallback:bool) -> anyhow::Result<ExplicitRoute> {
    route.validate()?;
    if is_direct(route) {return Ok(ExplicitRoute{proxy:None,prepared:None});}
    let prepared=match route {
        ProfileProxyRoute::Direct|ProfileProxyRoute::Invalid=>unreachable!("validated direct route handled above"),
        ProfileProxyRoute::System=>system().await?,
        ProfileProxyRoute::Proxy=>single(required(assigned)?,Route::ExplicitProxy).await?,
        ProfileProxyRoute::Chain{first:FirstHop::Direct,second:SecondHop::Proxy}=>single(required(assigned)?,Route::ExplicitProxy).await?,
        ProfileProxyRoute::Chain{first:FirstHop::System,second:SecondHop::Direct}=>system().await?,
        ProfileProxyRoute::Chain{first:FirstHop::Proxy{..},second:SecondHop::Direct}=>single(required(first)?,Route::ExplicitProxy).await?,
        ProfileProxyRoute::Chain{first:chosen,second:SecondHop::Proxy}=>{
            let assigned=required(assigned)?;
            let target=Target::new(&assigned.host,assigned.port)?;
            // Missing library entries and invalid configuration are not network
            // failures and cannot be hidden by the optional assigned fallback.
            let selected=match chosen {FirstHop::System=>None,FirstHop::Proxy{..}=>Some(hop(required(first)?)?),FirstHop::Direct=>unreachable!()};
            let attempt=async {
                let first=match selected {Some(first)=>first,None=>detect_windows::resolve_for(&target).await?.hop};
                if same_endpoint(assigned,&first)? {return assigned_route(assigned,Route::SameProxyEndpoint,None).await;}
                let mut prepared=prepare_with_hop(assigned,first).await?;prepared.route=Route::ExplicitChain;
                if let Err(error)=verify_prepared(&mut prepared).await {prepared.tunnel.shutdown().await;return Err(error.into());}Ok(prepared)
            }.await;
            match attempt {
                Ok(prepared)=>prepared,
                Err(error)=>{
                    let error=safe_prepare_error(error);
                    if !allow_assigned_fallback || matches!(error.code,"proxy_chain_invalid_config"|"proxy_chain_credentials_unavailable"|"proxy_chain_windows_required"|"proxy_chain_same_proxy_failed") {return Err(error.into());}
                    assigned_route(assigned,Route::AssignedProxyFallback,Some(error)).await?
                }
            }
        },
        ProfileProxyRoute::Chain{first:FirstHop::Direct,second:SecondHop::Direct}=>unreachable!(),
    };
    Ok(ExplicitRoute{proxy:Some(prepared.effective.clone()),prepared:Some(prepared)})
}

/// The caller resolves ids under the current space and its permissions. This
/// function performs no writes, starts no browser and has no saved-test cache.
pub async fn test_resolved_route(route:Option<&ProfileProxyRoute>,policy:policy::ChainPolicy,assigned:Option<&ProxyEntry>,first:Option<&ProxyEntry>) -> anyhow::Result<TestResult> {
    policy.validate()?;
    if let Some(route)=route {route.validate()?;}
    let start=Instant::now();
    let mut result=TestResult{kind:"profile".into(),proxy_kind:None,proxy_address:None,source:None,resolved_for:None,profile_proxy_address:assigned.map(|p|format!("{}:{}",p.host,p.port)),route:None,first_hop_error_code:None,system_proxy:Stage::pending(),profile_proxy:Stage::pending(),local_network:None,exit_ip:None,exit_geo:None,exit_error:None,exit_error_code:None,total_ms:0,tested_at:format!("@{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs())};
    if route.is_some_and(is_direct) {
        result.kind="local".into();result.route=Some(Route::ExplicitDirect);result.source=Some("explicit_direct");
        let geo=proxy::geo_check_on_route(None,None).await;
        result.local_network=Some(if geo.is_ok(){Stage::ok(start)}else{Stage::failed(ChainError::new("proxy_chain_local_network_failed","local","The explicitly selected direct connection did not return a live exit."))});
        result.set_exit(geo);
    } else {
        let prepared=if let Some(route)=route {prepare_route(route,assigned,first,policy==policy::ChainPolicy::PreferFirstHop).await.map(|p|p.prepared)} else {
            let saved=saved_settings()?;
            let config=policy::effective(&saved.proxy_chain,policy,assigned.map(|p|p.proxy_chain_policy).unwrap_or_default())?;
            if config.enabled {prepare(assigned,&config).await} else if let Some(assigned)=assigned {single(assigned,Route::ExplicitProxy).await.map(Some)} else {Ok(None)}
        };
        match prepared {
            Ok(Some(mut prepared))=>{
                if prepared.verified_geo.is_none() {if let Err(error)=verify_prepared(&mut prepared).await {result.profile_proxy=Stage::failed(error);prepared.tunnel.shutdown().await;result.total_ms=start.elapsed().as_millis();return Ok(result);}}
                result.route=Some(prepared.route);result.source=Some(prepared.route.as_str());result.first_hop_error_code=prepared.first_hop_error.as_ref().map(|e|e.code);
                let first_only=matches!(route,Some(ProfileProxyRoute::System|ProfileProxyRoute::Chain{second:SecondHop::Direct,..}));
                let first_used=matches!(route,Some(ProfileProxyRoute::System|ProfileProxyRoute::Chain{first:FirstHop::System|FirstHop::Proxy{..},..}));
                result.system_proxy=if let Some(error)=prepared.first_hop_error.clone() {Stage::failed(error)} else if prepared.route==Route::SameProxyEndpoint {Stage::pending()} else if first_used || route.is_none()&&matches!(prepared.route,Route::FirstHop) {Stage::ok(start)} else {Stage::pending()};
                result.profile_proxy=if first_only {Stage::pending()} else {Stage::ok(start)};
                if first_only {result.profile_proxy_address=None;}
                if let Some(first)=first.filter(|_|first_used) {result.proxy_address=Some(format!("{}:{}",first.host,first.port));result.proxy_kind=Some(match first.kind {ProxyKind::Http=>"http",ProxyKind::Https=>"https",ProxyKind::Socks5=>"socks5"});}
                result.set_exit(Ok(prepared.verified_geo.clone().expect("route verified")));
                prepared.tunnel.shutdown().await;
            },
            Err(error)=>{
                let error=safe_prepare_error(error);
                let first_only=matches!(route,Some(ProfileProxyRoute::System|ProfileProxyRoute::Chain{second:SecondHop::Direct,..}));
                result.first_hop_error_code=error.first_hop_error_code;
                if let Some(code)=error.first_hop_error_code {
                    result.system_proxy=Stage::failed(ChainError::new(code,"system_proxy","The first-hop route failed before the assigned-proxy fallback also failed."));
                    result.profile_proxy=Stage::failed(error);
                } else if first_only || matches!(error.layer,"system_proxy"|"configuration") {result.system_proxy=Stage::failed(error);} else {
                    // An endpoint skipped by de-duplication, a missing assigned
                    // proxy, or a combined-route failure proves no first-hop pass.
                    if !matches!(error.code,"proxy_chain_same_proxy_failed"|"proxy_chain_routes_failed"|"proxy_chain_profile_required")
                        && matches!(route,Some(ProfileProxyRoute::Chain{first:FirstHop::System|FirstHop::Proxy{..},second:SecondHop::Proxy})) {result.system_proxy=Stage::ok(start);}
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
