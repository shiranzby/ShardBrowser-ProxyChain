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

#[test]
fn cached_test_uses_saved_proxy_or_frozen_profile_policy_not_only_global() {
    use super::policy::ChainPolicy;
    use crate::space::proxy;
    let _root = crate::store::test_root("chain-policy-history");
    std::fs::write(crate::store::settings_path().unwrap(), r#"{"proxy_chain":{"enabled":true}}"#).unwrap();
    std::fs::write(crate::store::proxies_path().unwrap(), json!({"proxies":[{
        "id":"history-proxy", "name":"Synthetic history", "kind":"http", "host":"127.0.0.1", "port":1,
        "proxy_chain_policy":"disabled"
    }]}).to_string()).unwrap();
    let direct = json!({
        "first_seen":"@1", "last_seen":"@1", "ip":"192.0.2.1", "country_code":"JP", "country":"Japan",
        "region":"", "city":"", "isp":"", "timezone":"Asia/Tokyo", "latitude":0.0, "longitude":0.0,
        "tcp_ms":1, "udp_ms":2, "udp_error":null, "provider":"synthetic"
    });
    let mut chained = direct.clone();
    chained["transport"] = json!("proxy_chain");
    chained["ip"] = json!("192.0.2.2");
    chained["udp_ms"] = json!(null);
    std::fs::write(crate::store::config_root().unwrap().join("proxies-history.json"), json!({
        "by_proxy":{"history-proxy":[direct, chained]}
    }).to_string()).unwrap();
    // The list follows the stored proxy's explicit off despite global on.
    assert_eq!(proxy::latest_test("history-proxy").unwrap().ip, "192.0.2.1");
    let mut runtime = proxy::get("history-proxy").unwrap().unwrap();
    runtime.proxy_chain_policy = ChainPolicy::Enabled;
    assert_eq!(proxy::latest_test_for(&runtime).unwrap().ip, "192.0.2.2");
    runtime.proxy_chain_policy = ChainPolicy::Disabled;
    assert_eq!(proxy::latest_test_for(&runtime).unwrap().udp_ms, Some(2));
    runtime.proxy_chain_policy = ChainPolicy::Invalid;
    assert!(proxy::latest_test_for(&runtime).is_none());
    assert_eq!(proxy::get("history-proxy").unwrap().unwrap().proxy_chain_policy, ChainPolicy::Disabled);
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
