//! Detection uses real loopback proxy protocols. Fixtures answer Geo requests
//! themselves; they never resolve or connect to any external destination.

use super::config::{PasswordUpdate, ProxyChainSettings};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

const FIRST_USER: &str = "first+user@example.invalid";
const FIRST_PASS: &str = "first synthetic +@/#secret";
const SECOND_USER: &str = "second-user";
const SECOND_PASS: &str = "second synthetic +@/#secret";

struct Service {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl Drop for Service {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn http_head(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 16 * 1024 {
        bytes.push(stream.read_u8().await?);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn geo_response(stream: &mut TcpStream, ip: &str) -> std::io::Result<()> {
    let body = json!({
        "status": "success", "query": ip,
        "country": "Synthetic country", "countryCode": "ZZ",
        "regionName": "Synthetic region", "city": "Synthetic city",
        "isp": "Synthetic ISP", "timezone": "Etc/UTC", "lat": 1.25, "lon": 2.5
    })
    .to_string();
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
}

async fn http_proxy(destination: Option<SocketAddr>, geo_ok: bool) -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let record = requests.clone();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                connection = listener.accept() => {
                    let (mut stream, _) = connection.unwrap();
                    let record = record.clone();
                    clients.spawn(async move {
                        let Ok(head) = http_head(&mut stream).await else { return };
                        record.lock().unwrap().push(head.clone());
                        if head.starts_with("CONNECT ") {
                            if stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.is_err() { return; }
                            if let Some(destination) = destination {
                                let Ok(mut second) = TcpStream::connect(destination).await else { return };
                                let _ = tokio::io::copy_bidirectional(&mut stream, &mut second).await;
                            }
                        } else if geo_ok {
                            let _ = geo_response(&mut stream, "198.51.100.44").await;
                        } else {
                            let body = r#"{"status":"fail","message":"fixture-private-detail"}"#;
                            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                            let _ = stream.write_all(response.as_bytes()).await;
                        }
                    });
                },
            }
        }
    });
    Service {
        addr,
        requests,
        task,
    }
}

async fn socks_exchange(
    stream: &mut TcpStream,
    requests: &Mutex<Vec<String>>,
    username: &str,
    password: &str,
    ip: &str,
) -> std::io::Result<()> {
    if stream.read_u8().await? != 5 {
        return Ok(());
    }
    let mut methods = vec![0; stream.read_u8().await? as usize];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&2) {
        return Ok(());
    }
    stream.write_all(&[5, 2]).await?;
    if stream.read_u8().await? != 1 {
        return Ok(());
    }
    let mut user = vec![0; stream.read_u8().await? as usize];
    stream.read_exact(&mut user).await?;
    let mut pass = vec![0; stream.read_u8().await? as usize];
    stream.read_exact(&mut pass).await?;
    if user != username.as_bytes() || pass != password.as_bytes() {
        requests
            .lock()
            .unwrap()
            .push("AUTHENTICATION_MISMATCH".into());
        stream.write_all(&[1, 1]).await?;
        return Ok(());
    }
    stream.write_all(&[1, 0]).await?;
    let mut header = [0; 4];
    stream.read_exact(&mut header).await?;
    if header[..3] != [5, 1, 0] || header[3] != 3 {
        requests
            .lock()
            .unwrap()
            .push("DESTINATION_WAS_NOT_REMOTE_DNS".into());
        return Ok(());
    }
    let mut host = vec![0; stream.read_u8().await? as usize];
    stream.read_exact(&mut host).await?;
    let port = stream.read_u16().await?;
    requests
        .lock()
        .unwrap()
        .push(format!("SOCKS {}:{port}", String::from_utf8_lossy(&host)));
    stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await?;
    // Connectivity probes close after CONNECT; only the Geo request has HTTP.
    let head = http_head(stream).await?;
    requests.lock().unwrap().push(head);
    geo_response(stream, ip).await
}

async fn socks_proxy(username: &'static str, password: &'static str, ip: &'static str) -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let record = requests.clone();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                connection = listener.accept() => {
                    let (mut stream, _) = connection.unwrap();
                    let record = record.clone();
                    clients.spawn(async move {
                        let _ = socks_exchange(&mut stream, &record, username, password, ip).await;
                    });
                },
            }
        }
    });
    Service {
        addr,
        requests,
        task,
    }
}

fn config(addr: SocketAddr, mode: &str) -> ProxyChainSettings {
    ProxyChainSettings {
        enabled: true,
        mode: mode.into(),
        host: addr.ip().to_string(),
        port: addr.port(),
        username: FIRST_USER.into(),
        password: PasswordUpdate::Replace(FIRST_PASS.into()),
        ..Default::default()
    }
}

fn assigned(addr: SocketAddr, kind: &str) -> crate::space::proxy::ProxyEntry {
    serde_json::from_value(json!({"id":"adaptive-synthetic", "name":"Synthetic assigned", "kind":kind,
        "host":addr.ip().to_string(),"port":addr.port(),"username":SECOND_USER,"password":SECOND_PASS,
        "proxy_chain_policy":"prefer_first_hop"})).unwrap()
}

#[tokio::test]
async fn explicit_proxy_routes_ignore_global_chain_and_preserve_their_selected_binding() {
    use super::{route::{FirstHop,ProfileProxyRoute as R,SecondHop},routing};
    let _root=crate::store::test_root("explicit-proxy-route");
    let server=socks_proxy(SECOND_USER,SECOND_PASS,"198.51.100.85").await;
    let assigned=assigned(server.addr,"socks5");
    let before=serde_json::to_value(&assigned).unwrap();
    crate::settings::save(&crate::settings::Settings{proxy_chain:ProxyChainSettings{enabled:true,invalid:true,..Default::default()},..Default::default()}).unwrap();
    for route in [R::Proxy,R::Chain{first:FirstHop::Direct,second:SecondHop::Proxy},R::Chain{first:FirstHop::Proxy{proxy_id:assigned.id.clone()},second:SecondHop::Direct}] {
        let prepared=routing::prepare_route(&route,Some(&assigned),Some(&assigned),false).await.unwrap().prepared.unwrap();
        assert_eq!(prepared.verified_geo.as_ref().unwrap().ip,"198.51.100.85");
        assert_eq!(prepared.effective.password,SECOND_PASS);
        assert_eq!(serde_json::to_value(&assigned).unwrap(),before);
        prepared.tunnel.shutdown().await;
    }
    for route in [R::Direct,R::Chain{first:FirstHop::Direct,second:SecondHop::Direct}] {
        let ready=routing::prepare_route(&route,None,None,false).await.unwrap();
        assert!(ready.proxy.is_none()&&ready.prepared.is_none());
    }
}

#[tokio::test]
async fn explicit_chains_deduplicate_only_identical_entries_and_fallback_only_when_requested() {
    use super::{route::{FirstHop,ProfileProxyRoute as R,SecondHop},routing,Route};
    let _root=crate::store::test_root("explicit-chain-route");
    let second=socks_proxy(SECOND_USER,SECOND_PASS,"198.51.100.86").await;
    let entry=assigned(second.addr,"socks5");
    let route=R::Chain{first:FirstHop::Proxy{proxy_id:entry.id.clone()},second:SecondHop::Proxy};
    let same=routing::prepare_route(&route,Some(&entry),Some(&entry),false).await.unwrap().prepared.unwrap();
    assert_eq!(same.route,Route::SameProxyEndpoint);same.tunnel.shutdown().await;
    let detected=routing::test_resolved_route(Some(&route),super::policy::ChainPolicy::Enabled,Some(&entry),Some(&entry)).await.unwrap();
    assert_eq!(detected.route,Some(Route::SameProxyEndpoint));
    assert_eq!(detected.system_proxy.state,"not_tested","the duplicate first hop was skipped, not tested successfully");
    assert_eq!(detected.profile_proxy.state,"ok");
    let first=http_proxy(None,true).await;
    let first_entry=assigned(first.addr,"http");
    assert!(routing::prepare_route(&route,Some(&entry),Some(&first_entry),false).await.is_err());
    let fallback=routing::prepare_route(&route,Some(&entry),Some(&first_entry),true).await.unwrap().prepared.unwrap();
    assert_eq!(fallback.route,Route::AssignedProxyFallback);
    assert!(fallback.first_hop_error.is_some());assert_eq!(fallback.verified_geo.as_ref().unwrap().ip,"198.51.100.86");
    fallback.tunnel.shutdown().await;
    assert!(routing::prepare_route(&route,Some(&entry),None,true).await.is_err());
    assert!(routing::prepare_route(&R::Proxy,None,None,true).await.is_err());
}

#[tokio::test]
async fn explicit_route_failure_detection_keeps_both_failed_routes_and_the_first_error() {
    use super::{route::{FirstHop,ProfileProxyRoute as R,SecondHop},routing,policy::ChainPolicy};
    let _root=crate::store::test_root("explicit-double-failure-stages");
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_addr=listener.local_addr().unwrap();
    let requests=Arc::new(Mutex::new(Vec::new()));
    let record=requests.clone();
    let first=Service {addr:first_addr,requests,task:tokio::spawn(async move {
        while let Ok((mut socket,_))=listener.accept().await {
            if let Ok(head)=http_head(&mut socket).await { record.lock().unwrap().push(head); }
            let _=socket.write_all(b"HTTP/1.1 403 Synthetic refusal\r\nContent-Length: 0\r\n\r\n").await;
        }
    })};
    let second=socks_proxy(SECOND_USER,SECOND_PASS,"198.51.100.87").await;
    let mut entry=assigned(second.addr,"socks5");
    entry.password="wrong synthetic password".into();
    let first_entry=assigned(first.addr,"http");
    let route=R::Chain {first:FirstHop::Proxy {proxy_id:first_entry.id.clone()},second:SecondHop::Proxy};
    let result=routing::test_resolved_route(Some(&route),ChainPolicy::PreferFirstHop,Some(&entry),Some(&first_entry)).await.unwrap();
    assert_eq!(result.first_hop_error_code,Some("system_proxy_connect_refused"));
    assert_eq!(result.system_proxy.state,"failed");
    assert_eq!(result.system_proxy.error_code,Some("system_proxy_connect_refused"));
    assert_eq!(result.profile_proxy.state,"failed");
    assert_eq!(result.profile_proxy.error_code,Some("proxy_chain_routes_failed"));
    assert!(result.exit_ip.is_none()&&result.exit_geo.is_none());
    assert!(!first.requests.lock().unwrap().is_empty());
    assert!(second.requests.lock().unwrap().iter().any(|r|r=="AUTHENTICATION_MISMATCH"));
    let safe=serde_json::to_string(&result).unwrap();
    for secret in [SECOND_PASS,"wrong synthetic password"] {assert!(!safe.contains(secret));}
}

#[tokio::test]
async fn explicit_route_failure_detection_does_not_claim_an_identical_failed_endpoint_passed() {
    use super::{route::{FirstHop,ProfileProxyRoute as R,SecondHop},routing,policy::ChainPolicy};
    let _root=crate::store::test_root("explicit-identical-failure-stages");
    let second=http_proxy(None,false).await;
    let entry=assigned(second.addr,"http");
    let route=R::Chain {first:FirstHop::Proxy {proxy_id:entry.id.clone()},second:SecondHop::Proxy};
    let result=routing::test_resolved_route(Some(&route),ChainPolicy::Enabled,Some(&entry),Some(&entry)).await.unwrap();
    assert_ne!(result.system_proxy.state,"ok");
    assert_eq!(result.profile_proxy.state,"failed");
    assert_eq!(result.profile_proxy.error_code,Some("proxy_chain_same_proxy_failed"));
    assert!(result.first_hop_error_code.is_none());
    assert!(result.exit_ip.is_none()&&result.exit_geo.is_none());
    assert!(!second.requests.lock().unwrap().iter().any(|head|head.starts_with(&format!("CONNECT {}:{} ",entry.host,entry.port))));
}

fn prefer(config: &ProxyChainSettings) -> ProxyChainSettings {
    super::policy::effective(config, super::policy::ChainPolicy::PreferFirstHop, Default::default()).unwrap()
}

fn save_assigned_profile(entry: &crate::space::proxy::ProxyEntry, profile_policy: &str) {
    std::fs::write(crate::store::proxies_path().unwrap(), json!({"proxies":[entry]}).to_string()).unwrap();
    std::fs::write(crate::store::profiles_dir().unwrap().join("adaptive-profile.json"), json!({
        "name":"Synthetic", "_meta":{"id":"adaptive-profile","proxy_id":entry.id,"proxy_chain_policy":profile_policy}
    }).to_string()).unwrap();
}

#[tokio::test]
async fn preferred_failed_first_hop_keeps_the_assigned_proxy_and_freezes_its_protected_route() {
    let _root = crate::store::test_root("preferred-fixed-route");
    let second = socks_proxy(SECOND_USER, SECOND_PASS, "198.51.100.81").await;
    let first = http_proxy(None, true).await; // CONNECT closes before the second protocol.
    let entry = assigned(second.addr, "socks5");
    let original = serde_json::to_value(&entry).unwrap();
    let prepared = super::prepare(Some(&entry), &prefer(&config(first.addr, "http"))).await.unwrap().unwrap();
    assert_eq!(prepared.route, super::Route::AssignedProxyFallback);
    assert!(prepared.first_hop_error.is_some());
    assert_eq!(prepared.verified_geo.as_ref().unwrap().ip, "198.51.100.81");
    assert_eq!(prepared.effective.username, SECOND_USER);
    assert_eq!(prepared.effective.password, SECOND_PASS);
    assert!(prepared.effective.id.is_empty());
    assert_eq!(serde_json::to_value(&entry).unwrap(), original);
    let first_calls = first.requests.lock().unwrap().len();
    // Existing Session traffic is a fixed tunnel, not a new policy selection.
    crate::settings::save(&crate::settings::Settings { proxy_chain: config(first.addr, "http"), ..Default::default() }).unwrap();
    assert_eq!(crate::space::proxy::geo_check_on_route(Some(&prepared.effective), None).await.unwrap().ip, "198.51.100.81");
    assert_eq!(first.requests.lock().unwrap().len(), first_calls);
    let closed = prepared.tunnel.closed_signal();
    prepared.tunnel.shutdown().await;
    assert!(*closed.borrow());
}

#[tokio::test]
async fn preferred_working_chain_accepts_matching_exit_ip_without_skipping_a_hop() {
    let _root = crate::store::test_root("preferred-same-exit");
    let second = http_proxy(None, true).await;
    let first = http_proxy(Some(second.addr), true).await;
    let entry = assigned(second.addr, "http");
    save_assigned_profile(&entry, "inherit");
    let draft = config(first.addr, "http");
    let first_result = super::test("system".into(), draft.clone(), None).await.unwrap();
    let profile_result = super::test("profile".into(), draft, Some("adaptive-profile".into())).await.unwrap();
    assert_eq!(first_result.exit_ip, profile_result.exit_ip);
    assert_eq!(profile_result.route, Some(super::Route::FirstHop));
    assert_eq!(profile_result.profile_proxy.state, "ok");
    assert!(profile_result.first_hop_error_code.is_none());
    assert!(!first.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn preferred_exact_endpoint_requires_protocol_host_port_and_credentials() {
    let _root = crate::store::test_root("preferred-identical-endpoint");
    let second = http_proxy(None, true).await;
    let entry = assigned(second.addr, "http");
    let mut draft = config(second.addr, "http");
    draft.username = SECOND_USER.into(); draft.password = PasswordUpdate::Replace(SECOND_PASS.into());
    let prepared = super::prepare(Some(&entry), &prefer(&draft)).await.unwrap().unwrap();
    assert_eq!(prepared.route, super::Route::SameProxyEndpoint);
    assert!(prepared.first_hop_error.is_none());
    assert!(!second.requests.lock().unwrap().iter().any(|head| head.starts_with(&format!("CONNECT {}:{} ", entry.host, entry.port))));
    prepared.tunnel.shutdown().await;
    let mut hop = draft.hop(&draft).unwrap();
    assert!(super::same_endpoint(&entry, &hop).unwrap());
    hop.password.push('x'); assert!(!super::same_endpoint(&entry, &hop).unwrap()); hop.password.pop();
    hop.username.push('x'); assert!(!super::same_endpoint(&entry, &hop).unwrap()); hop.username.pop();
    hop.port += 1; assert!(!super::same_endpoint(&entry, &hop).unwrap()); hop.port -= 1;
    hop.kind = super::connector::ProxyKind::Socks5; assert!(!super::same_endpoint(&entry, &hop).unwrap());
    let mut tls_entry = entry.clone(); tls_entry.kind = crate::space::proxy::ProxyKind::Https;
    assert!(!super::same_endpoint(&tls_entry, &draft.hop(&draft).unwrap()).unwrap());
    let mut host_entry = entry.clone(); host_entry.host = "PROXY.EXAMPLE".into();
    let mut host_hop = draft.hop(&draft).unwrap(); host_hop.host = "proxy.example".into();
    assert!(super::same_endpoint(&host_entry, &host_hop).unwrap());
}

#[tokio::test]
async fn preferred_invalid_config_missing_proxy_and_corrupt_credentials_never_dial_assigned() {
    let _root = crate::store::test_root("preferred-no-permission-bypass");
    let second = http_proxy(None, true).await;
    let entry = assigned(second.addr, "http");
    let mut draft = prefer(&config(second.addr, "http"));
    draft.invalid = true;
    assert!(super::prepare(Some(&entry), &draft).await.is_err());
    draft.invalid = false;
    assert!(super::prepare(None, &draft).await.is_err());
    draft.password = PasswordUpdate::Keep;
    draft.protected_password = Some("broken-dpapi".into());
    let error = super::prepare(Some(&entry), &draft).await.err().unwrap();
    assert_eq!(error.downcast_ref::<super::error::ChainError>().unwrap().code, "proxy_chain_credentials_unavailable");
    assert!(second.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn strict_route_failure_does_not_try_assigned_and_preferred_double_failure_is_safe() {
    let _root = crate::store::test_root("preferred-vs-strict");
    let second = http_proxy(None, false).await;
    let first = http_proxy(None, true).await;
    let mut entry = assigned(second.addr, "http");
    entry.proxy_chain_policy = super::policy::ChainPolicy::Enabled;
    save_assigned_profile(&entry, "inherit");
    let strict = super::test("profile".into(), config(first.addr, "http"), Some("adaptive-profile".into())).await.unwrap();
    assert_eq!(strict.profile_proxy.state, "failed");
    assert!(second.requests.lock().unwrap().is_empty());
    entry.proxy_chain_policy = super::policy::ChainPolicy::PreferFirstHop;
    save_assigned_profile(&entry, "inherit");
    let failed = super::test("profile".into(), config(first.addr, "http"), Some("adaptive-profile".into())).await.unwrap();
    assert_eq!(failed.profile_proxy.error_code, Some("proxy_chain_routes_failed"));
    assert!(failed.first_hop_error_code.is_some());
    assert!(failed.exit_ip.is_none());
    assert!(!second.requests.lock().unwrap().is_empty());
    let serialized = serde_json::to_string(&failed).unwrap();
    for secret in [FIRST_PASS, SECOND_PASS, "fixture-private-detail"] { assert!(!serialized.contains(secret)); }
}

#[tokio::test]
async fn preferred_public_draft_uses_saved_first_hop_credentials_and_snapshot_keeps_route() {
    use base64::Engine;
    let _root = crate::store::test_root("preferred-saved-credentials");
    let second = http_proxy(None, true).await;
    let first = http_proxy(Some(second.addr), true).await;
    let entry = assigned(second.addr, "http");
    save_assigned_profile(&entry, "inherit");
    let mut stored = config(first.addr, "http");
    stored.apply_update(&Default::default()).unwrap();
    crate::settings::save(&crate::settings::Settings { proxy_chain: stored.clone(), ..Default::default() }).unwrap();
    let result = super::test("profile".into(), stored.public(), Some("adaptive-profile".into())).await.unwrap();
    assert_eq!(result.route, Some(super::Route::FirstHop));
    let expected = format!("Proxy-Authorization: Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{FIRST_USER}:{FIRST_PASS}")));
    assert!(first.requests.lock().unwrap().iter().all(|head| head.contains(&expected)));
    let snap = crate::space::proxy::full_test(&entry).await.unwrap();
    assert_eq!(snap.route.as_deref(), Some("first_hop"));
    assert_eq!(snap.transport.as_deref(), Some("proxy_chain"));
    assert_eq!(snap.udp_state.as_deref(), Some("unsupported_by_chain"));
    assert!(snap.udp_ms.is_none());
    let loaded = crate::space::proxy::latest_test(&entry.id).unwrap();
    assert_eq!(loaded.route.as_deref(), Some("first_hop"), "display history survives a reload");
    assert!(crate::space::proxy::latest_test_for(&entry).is_none(), "launch cannot reuse adaptive history as capability evidence");
}

#[tokio::test]
async fn preferred_rejects_assigned_authentication_and_never_switches_after_route_selection() {
    let _root = crate::store::test_root("preferred-auth-and-freeze");
    let second = socks_proxy(SECOND_USER, SECOND_PASS, "198.51.100.81").await;
    let first = http_proxy(None, true).await;
    let mut entry = assigned(second.addr, "socks5");
    entry.password = "wrong synthetic password".into();
    let error = super::prepare(Some(&entry), &prefer(&config(first.addr, "http"))).await.err().unwrap();
    assert_eq!(error.downcast_ref::<super::error::ChainError>().unwrap().code, "proxy_chain_routes_failed");
    assert!(second.requests.lock().unwrap().iter().any(|r| r == "AUTHENTICATION_MISMATCH"));
    entry.password = SECOND_PASS.into();
    let prepared = super::prepare(Some(&entry), &prefer(&config(first.addr, "http"))).await.unwrap().unwrap();
    assert_eq!(prepared.route, super::Route::AssignedProxyFallback);
    let first_calls = first.requests.lock().unwrap().len();
    drop(second);
    assert!(crate::space::proxy::probe_on_route(&prepared.effective).await.is_err());
    assert_eq!(first.requests.lock().unwrap().len(), first_calls, "a selected route cannot reconnect through the first hop");
    prepared.tunnel.shutdown().await;
}

#[tokio::test]
async fn preferred_identical_endpoint_failure_is_reported_as_one_route() {
    let _root = crate::store::test_root("preferred-same-endpoint-error");
    let second = http_proxy(None, false).await;
    let mut entry = assigned(second.addr, "http");
    entry.username = FIRST_USER.into(); entry.password = FIRST_PASS.into();
    let error = super::prepare(Some(&entry), &prefer(&config(second.addr, "http"))).await.err().unwrap();
    let error = error.downcast_ref::<super::error::ChainError>().unwrap();
    assert_eq!(error.code, "proxy_chain_same_proxy_failed");
    assert!(error.first_hop_error_code.is_none());
    assert!(!second.requests.lock().unwrap().iter().any(|head| head.starts_with(&format!("CONNECT {}:{} ", entry.host, entry.port))));
}

#[tokio::test]
async fn profile_choice_controls_detection_and_overrides_proxy_then_global() {
    let _root = crate::store::test_root("chain-policy-detection");
    let second = http_proxy(None, true).await;
    let first = http_proxy(Some(second.addr), true).await;
    let proxies = crate::store::proxies_path().unwrap();
    let profile = crate::store::profiles_dir().unwrap().join("policy-profile.json");
    for (global, proxy_policy, profile_policy, chained) in [
        (true, "enabled", "disabled", false),
        (true, "disabled", "inherit", false),
        (false, "disabled", "enabled", true),
        (false, "enabled", "inherit", true),
        (false, "inherit", "inherit", false),
    ] {
        std::fs::write(&proxies, json!({"proxies":[{"id":"policy-proxy","name":"Synthetic","kind":"http","host":"127.0.0.1","port":second.addr.port(),"proxy_chain_policy":proxy_policy}]}).to_string()).unwrap();
        std::fs::write(&profile, json!({"name":"Synthetic policy","_meta":{"id":"policy-profile","proxy_id":"policy-proxy","proxy_chain_policy":profile_policy}}).to_string()).unwrap();
        let before = first.requests.lock().unwrap().len();
        let mut draft = config(first.addr, "http"); draft.enabled = global;
        let dto = serde_json::to_value(super::test("profile".into(), draft, Some("policy-profile".into())).await.unwrap()).unwrap();
        assert_eq!(dto["profile_proxy"]["state"], "ok", "{dto}");
        assert_eq!(dto["exit_ip"], "198.51.100.44");
        assert_eq!(first.requests.lock().unwrap().len() > before, chained);
        assert_eq!(dto["system_proxy"]["state"], if chained { "ok" } else { "not_tested" });
    }
}


#[tokio::test]
async fn system_http_detection_returns_live_geo_through_the_same_authenticated_hop() {
    let _root = crate::store::test_root("chain-system-http-geo");
    // Saved enabled chaining must not wrap the first hop a second time.
    let settings_path = crate::store::settings_path().unwrap();
    std::fs::write(
        &settings_path,
        r#"{"proxy_chain":{"enabled":true,"mode":"http","host":"unused.invalid","port":1}}"#,
    )
    .unwrap();
    let before_settings = std::fs::read(&settings_path).unwrap();
    let proxies_path = crate::store::proxies_path().unwrap();
    std::fs::write(&proxies_path, br#"{"proxies":[]}"#).unwrap();
    let before_proxies = std::fs::read(&proxies_path).unwrap();
    let service = http_proxy(None, true).await;
    let result = super::test("system".into(), config(service.addr, "http"), None)
        .await
        .unwrap();
    let dto = serde_json::to_value(result).unwrap();
    assert_eq!(dto["system_proxy"]["state"], "ok");
    assert_eq!(dto["exit_ip"], "198.51.100.44");
    assert_eq!(dto["exit_geo"]["country"], "Synthetic country");
    assert_eq!(dto["exit_geo"]["region"], "Synthetic region");
    assert_eq!(dto["exit_geo"]["city"], "Synthetic city");
    assert!(dto["exit_error"].is_null());
    assert!(dto["exit_error_code"].is_null());
    let requests = service.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("CONNECT example.com:443 "));
    assert!(requests[1].starts_with("GET http://ip-api.com/"));
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let auth = format!(
        "proxy-authorization: basic {}",
        STANDARD.encode(format!("{FIRST_USER}:{FIRST_PASS}"))
    )
    .to_ascii_lowercase();
    for request in requests.iter() {
        assert!(request.to_ascii_lowercase().contains(&auth));
    }
    assert_eq!(std::fs::read(settings_path).unwrap(), before_settings);
    assert_eq!(std::fs::read(proxies_path).unwrap(), before_proxies);
}

#[tokio::test]
async fn system_socks5_detection_uses_remote_dns_and_its_original_credentials() {
    let _root = crate::store::test_root("chain-system-socks-geo");
    let service = socks_proxy(FIRST_USER, FIRST_PASS, "198.51.100.44").await;
    let result = super::test("system".into(), config(service.addr, "socks5"), None)
        .await
        .unwrap();
    let dto = serde_json::to_value(result).unwrap();
    assert_eq!(dto["system_proxy"]["state"], "ok");
    assert_eq!(dto["exit_ip"], "198.51.100.44");
    assert_eq!(dto["exit_geo"]["city"], "Synthetic city");
    let requests = service.requests.lock().unwrap();
    assert!(requests.iter().any(|r| r == "SOCKS example.com:443"));
    assert!(requests.iter().any(|r| r == "SOCKS ip-api.com:80"));
    assert!(!requests
        .iter()
        .any(|r| r.contains("MISMATCH") || r.contains("NOT_REMOTE_DNS")));
}

#[tokio::test]
async fn system_geo_failure_preserves_connectivity_and_never_returns_provider_details() {
    let _root = crate::store::test_root("chain-system-geo-fails");
    let service = http_proxy(None, false).await;
    let result = super::test("system".into(), config(service.addr, "http"), None)
        .await
        .unwrap();
    let dto = serde_json::to_value(result).unwrap();
    assert_eq!(dto["system_proxy"]["state"], "ok");
    assert!(dto["exit_ip"].is_null());
    assert!(dto["exit_geo"].is_null());
    assert_eq!(dto["exit_error_code"], "proxy_chain_system_geo_failed");
    assert!(!dto.to_string().contains("fixture-private-detail"));
    let requests = service.requests.lock().unwrap();
    assert!(requests
        .iter()
        .any(|r| r.starts_with("GET http://ip-api.com/")));
    assert!(requests
        .iter()
        .any(|r| r.starts_with("CONNECT ipapi.co:443 ")));
    assert!(requests
        .iter()
        .any(|r| r.starts_with("CONNECT ipwho.is:443 ")));
}

#[tokio::test]
async fn profile_geo_is_the_second_hop_exit_and_original_binding_is_unchanged() {
    let _root = crate::store::test_root("chain-profile-exit-geo");
    let second = socks_proxy(SECOND_USER, SECOND_PASS, "203.0.113.81").await;
    let first = http_proxy(Some(second.addr), true).await;
    let saved = json!({"proxies": [{
        "id": "geo-second-proxy", "name": "Synthetic second", "kind": "socks5",
        "host": "second-proxy.invalid", "port": 1080,
        "username": SECOND_USER, "password": SECOND_PASS
    }]});
    let proxies_path = crate::store::proxies_path().unwrap();
    let original_proxies = serde_json::to_vec(&saved).unwrap();
    std::fs::write(&proxies_path, &original_proxies).unwrap();
    let profile_path = crate::store::profiles_dir()
        .unwrap()
        .join("geo-profile.json");
    let original_profile = json!({
        "_meta": {"id": "geo-profile", "proxy_id": "geo-second-proxy", "rev": 1, "created_at": "@1"},
        "name": "Synthetic geo profile"
    }).to_string();
    std::fs::write(&profile_path, &original_profile).unwrap();
    let result = super::test(
        "profile".into(),
        config(first.addr, "http"),
        Some("geo-profile".into()),
    )
    .await
    .unwrap();
    let dto = serde_json::to_value(result).unwrap();
    assert_eq!(dto["system_proxy"]["state"], "ok");
    assert_eq!(dto["profile_proxy"]["state"], "ok");
    assert_eq!(
        dto["exit_ip"], "203.0.113.81",
        "first-hop Geo cannot masquerade as the profile exit"
    );
    assert_eq!(dto["exit_geo"]["country_code"], "ZZ");
    assert_eq!(dto["exit_geo"]["region"], "Synthetic region");
    assert_eq!(dto["exit_geo"]["city"], "Synthetic city");
    assert!(first
        .requests
        .lock()
        .unwrap()
        .iter()
        .all(|r| r.starts_with("CONNECT second-proxy.invalid:1080 ")));
    let requests = second.requests.lock().unwrap();
    assert!(requests.iter().any(|r| r == "SOCKS example.com:443"));
    assert!(requests.iter().any(|r| r == "SOCKS ip-api.com:80"));
    assert!(!requests
        .iter()
        .any(|r| r.contains("MISMATCH") || r.contains("NOT_REMOTE_DNS")));
    assert_eq!(std::fs::read(proxies_path).unwrap(), original_proxies);
    assert_eq!(
        std::fs::read_to_string(profile_path).unwrap(),
        original_profile
    );
}

fn pending(kind: &str) -> super::TestResult {
    super::TestResult {
        route: None,
        first_hop_error_code: None,
        kind: kind.into(),
        proxy_kind: None,
        proxy_address: None,
        source: None,
        resolved_for: None,
        profile_proxy_address: None,
        system_proxy: super::Stage::pending(),
        profile_proxy: super::Stage::pending(),
        local_network: None,
        exit_ip: None,
        exit_geo: None,
        exit_error: None,
        exit_error_code: None,
        total_ms: 0,
        tested_at: "@1".into(),
    }
}

#[test]
fn every_detection_kind_serializes_full_geo_and_safe_distinct_geo_errors() {
    for kind in ["local", "system", "profile"] {
        let geo = crate::space::proxy::GeoInfo {
            ip: "192.0.2.9".into(),
            country: "Country".into(),
            country_code: "ZZ".into(),
            region: "Region".into(),
            city: "City".into(),
            isp: "ISP".into(),
            timezone: "Etc/UTC".into(),
            latitude: 1.25,
            longitude: 2.5,
            provider: "Synthetic".into(),
        };
        let expected = serde_json::to_value(&geo).unwrap();
        let mut result = pending(kind);
        result.set_exit(Ok(geo));
        let dto = serde_json::to_value(result).unwrap();
        assert_eq!(dto["exit_ip"], "192.0.2.9");
        assert_eq!(dto["exit_geo"], expected);
        assert!(dto["exit_error_code"].is_null());
        let mut failure = pending(kind);
        failure.set_exit(Err(anyhow::anyhow!("private URL / credential detail")));
        let dto = serde_json::to_value(failure).unwrap();
        assert_eq!(
            dto["exit_error_code"],
            format!("proxy_chain_{kind}_geo_failed")
        );
        assert!(dto["exit_geo"].is_null());
        assert!(dto["exit_ip"].is_null());
        assert!(!dto.to_string().contains("private URL"));
    }
}

#[test]
fn system_entry_brackets_ipv6_for_geo_urls_and_keeps_credentials_in_memory() {
    for kind in [
        super::connector::ProxyKind::Http,
        super::connector::ProxyKind::Socks5,
    ] {
        let hop = super::connector::ProxyHop {
            kind,
            host: "::1".into(),
            port: 7890,
            username: FIRST_USER.into(),
            password: FIRST_PASS.into(),
        };
        let entry = super::system_entry(&hop);
        assert_eq!(entry.host, "[::1]");
        assert!(entry.id.is_empty());
        assert_eq!(entry.username, FIRST_USER);
        assert_eq!(entry.password, FIRST_PASS);
        assert_eq!(
            hop.host, "::1",
            "the resolved immutable hop must not change"
        );
        assert!(reqwest::Proxy::all(entry.to_proxy_server_arg()).is_ok());
    }
}
