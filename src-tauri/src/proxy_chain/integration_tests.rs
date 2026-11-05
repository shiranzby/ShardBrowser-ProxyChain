//! Product integration invariants, using only isolated profile files and
//! synthetic loopback services. No system settings or remote proxies are read.

use super::config::ProxyChainSettings;
use super::connector::{ProxyHop, ProxyKind, Target};
use super::error::ChainError;
use super::tunnel::Tunnel;
use crate::space::proxy::ProxyEntry;
use serde_json::json;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

const WAIT: Duration = Duration::from_secs(15);

#[tokio::test]
async fn https_second_hop_verifies_original_name_and_carries_http_inside_tls() {
    use rustls::pki_types::PrivatePkcs8KeyDer;
    let certified = rcgen::generate_simple_self_signed(vec!["second.test".into()]).unwrap();
    let cert = certified.cert.der().clone();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server = rustls::ServerConfig::builder_with_provider(provider.clone()).with_safe_default_protocol_versions().unwrap()
        .with_no_client_auth().with_single_cert(vec![cert.clone()], PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()).into()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let second = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server));
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                raw = listener.accept() => {
                    let (raw, _) = raw.unwrap();
                    let acceptor = acceptor.clone();
                    clients.spawn(async move {
                        let Ok(mut stream) = acceptor.accept(raw).await else { return; };
                        let mut head = Vec::new();
                        while !head.ends_with(b"\r\n\r\n") && head.len() < 4096 {
                            let Ok(byte) = stream.read_u8().await else { return; };
                            head.push(byte);
                        }
                        assert!(head.starts_with(b"CONNECT example.com:443 HTTP/1.1"));
                        assert!(String::from_utf8_lossy(&head).contains("Proxy-Authorization: Basic c2Vjb25kOnNlY3JldA=="));
                        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                        let _ = stream.shutdown().await;
                    });
                }
            }
        }
    });
    let first = first_hop(addr).await;
    let hop = chain(first.addr).hop(&Default::default()).unwrap();
    let target = Target::new("second.test", addr.port()).unwrap();
    // Default trust must reject our self-signed fixture, not silently downgrade.
    assert_eq!(super::tls::connect(&hop, &target, super::tls::client().unwrap(), WAIT).await.err().unwrap().code, "proxy_chain_profile_tls_failed");
    let mut roots = rustls::RootCertStore::empty(); roots.add(cert).unwrap();
    let client = Arc::new(rustls::ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth());
    let wrong = Target::new("wrong.test", addr.port()).unwrap();
    assert_eq!(super::tls::connect(&hop, &wrong, client.clone(), WAIT).await.err().unwrap().code, "proxy_chain_profile_tls_failed");
    let tunnel = Tunnel::bind_tls(hop, target, client).await.unwrap();
    let mut local = TcpStream::connect(tunnel.addr()).await.unwrap();
    local.write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nProxy-Authorization: Basic c2Vjb25kOnNlY3JldA==\r\n\r\n").await.unwrap();
    let head = tokio::time::timeout(WAIT, http_head(&mut local)).await.unwrap().unwrap();
    assert!(head.starts_with(b"HTTP/1.1 200"));
    tunnel.shutdown().await;
    assert!(first.accepted.load(Ordering::SeqCst) >= 3);
    second.abort();
    let _ = second.await;
}

#[tokio::test]
async fn http_second_hop_refusal_is_typed_and_does_not_change_saved_proxy() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut s, _) = listener.accept().await.unwrap();
            if http_head(&mut s).await.is_ok() { let _ = s.write_all(b"HTTP/1.1 407 Authentication Required\r\n\r\n").await; }
        }
    });
    let first = first_hop(addr).await;
    let mut proxy = entry("http-fixture", addr);
    proxy.kind = crate::space::proxy::ProxyKind::Http;
    let before = serde_json::to_value(&proxy).unwrap();
    let prepared = super::prepare(Some(&proxy), &chain(first.addr)).await.unwrap().unwrap();
    let refusal = crate::space::proxy::probe_on_route(&prepared.effective).await.unwrap_err();
    assert_eq!(super::profile_failure(&prepared.tunnel, &refusal).code, "proxy_chain_profile_auth_failed");
    assert_eq!(serde_json::to_value(&proxy).unwrap(), before);
    prepared.tunnel.shutdown().await;
    task.abort();
}

struct Service {
    addr: SocketAddr,
    accepted: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}
impl Drop for Service {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn http_head(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    while !out.ends_with(b"\r\n\r\n") && out.len() < 16 * 1024 {
        out.push(stream.read_u8().await?);
    }
    Ok(out)
}

/// The first hop forwards only to the fixture's fixed destination, records
/// connection count, and never uses the hostname named in CONNECT for DNS.
async fn first_hop(destination: SocketAddr) -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                accepted = listener.accept() => {
                    let (mut a, _) = accepted.unwrap();
                    count.fetch_add(1, Ordering::SeqCst);
                    clients.spawn(async move {
                        if http_head(&mut a).await.is_err() { return; }
                        let Ok(mut b) = TcpStream::connect(destination).await else { return; };
                        if a.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.is_err() { return; }
                        let _ = tokio::io::copy_bidirectional(&mut a, &mut b).await;
                    });
                },
            }
        }
    });
    Service {
        addr,
        accepted,
        task,
    }
}

async fn second_hop(reject_auth: bool) -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap();
                    count.fetch_add(1, Ordering::SeqCst);
                    clients.spawn(async move {
                        if reject_auth {
                            let mut header = [0; 2];
                            // The connectivity-only preflight opens and closes
                            // without greeting; the profile probe follows later.
                            if stream.read_exact(&mut header).await.is_err() { return; }
                            let mut methods = vec![0; header[1] as usize];
                            if stream.read_exact(&mut methods).await.is_err() { return; }
                            if stream.write_all(&[5, 2]).await.is_err() { return; }
                            if stream.read_u8().await.is_err() { return; }
                            let Ok(n) = stream.read_u8().await else { return; };
                            let mut user = vec![0; n as usize];
                            if stream.read_exact(&mut user).await.is_err() { return; }
                            let Ok(n) = stream.read_u8().await else { return; };
                            let mut pass = vec![0; n as usize];
                            if stream.read_exact(&mut pass).await.is_err() { return; }
                            let _ = stream.write_all(&[1, 1]).await;
                        } else {
                            let mut buf = [0; 512];
                            loop {
                                match stream.read(&mut buf).await {
                                    Ok(0) | Err(_) => return,
                                    Ok(n) => if stream.write_all(&buf[..n]).await.is_err() { return; },
                                }
                            }
                        }
                    });
                },
            }
        }
    });
    Service {
        addr,
        accepted,
        task,
    }
}

fn entry(id: &str, addr: SocketAddr) -> ProxyEntry {
    serde_json::from_value(json!({
        "id": id, "name": format!("Synthetic {id}"), "kind": "socks5",
        "host": addr.ip().to_string(), "port": addr.port(),
        "username": "second-user", "password": "second-synthetic-secret"
    }))
    .unwrap()
}

fn chain(addr: SocketAddr) -> ProxyChainSettings {
    ProxyChainSettings {
        enabled: true,
        mode: "http".into(),
        host: addr.ip().to_string(),
        port: addr.port(),
        ..Default::default()
    }
}

fn profile(id: &str, proxy_id: &str) -> PathBuf {
    let path = crate::store::profiles_dir()
        .unwrap()
        .join(format!("{id}.json"));
    std::fs::write(
        &path,
        json!({
            "_meta": { "id": id, "rev": 1, "proxy_id": proxy_id, "created_at": "@1" },
            "name": id, "webrtc": "auto", "navigator": { "platform": "Win32" },
            "screen": { "width": 1920, "height": 1080 },
            "synthetic_fingerprint_marker": format!("fingerprint-{id}")
        })
        .to_string(),
    )
    .unwrap();
    path
}

fn save_proxies(entries: &[ProxyEntry]) -> Vec<u8> {
    let bytes = serde_json::to_vec_pretty(&json!({ "proxies": entries })).unwrap();
    std::fs::write(crate::store::proxies_path().unwrap(), &bytes).unwrap();
    bytes
}

#[cfg(windows)]
fn marker_engine(root: &Path) -> (PathBuf, PathBuf) {
    let marker = root.join("engine-was-started");
    let engine = root.join("synthetic-engine.cmd");
    std::fs::write(
        &engine,
        format!("@echo off\r\necho started>\"{}\"\r\n", marker.display()),
    )
    .unwrap();
    (engine, marker)
}

async fn eventually(mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(WAIT, async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("synthetic integration deadline");
}

async fn roundtrip(stream: &mut TcpStream, content: &[u8]) {
    tokio::time::timeout(WAIT, async {
        stream.write_all(content).await.unwrap();
        let mut received = vec![0; content.len()];
        stream.read_exact(&mut received).await.unwrap();
        assert_eq!(received, content);
    })
    .await
    .unwrap();
}

async fn closed(stream: &mut TcpStream) {
    let result = tokio::time::timeout(WAIT, stream.read(&mut [0; 1]))
        .await
        .unwrap();
    assert!(
        matches!(result, Ok(0)) || result.is_err(),
        "the original profile stream must close"
    );
}

#[test]
fn conflicting_switches_cannot_override_the_chain() {
    for value in [
        "--proxy-server=x",
        "/NO-PROXY-SERVER",
        "-enable-quic",
        "--proxy-auto-detect",
        "--proxy-pac-url x",
        "--shardx-webrtc-policy=auto",
        "--proxy-bypass-list=*",
        "--force-webrtc-ip-handling-policy=default",
        "--remote-debugging-port=9222",
        "/REMOTE-DEBUGGING-PIPE",
        "-remote-debugging-address=0.0.0.0",
        "--shardx-automation",
        "--user-data-dir x",
        "--fingerprint-profile=x",
        "\"/PrOxY-SeRvEr=socks5://localhost:1\"",
        "-proxy-server http://localhost:1",
    ] {
        assert!(
            super::validate_args(value).is_err(),
            "a conflicting switch escaped validation"
        );
    }
    assert!(super::validate_args("--window-size=1280,800 --lang=zh-CN").is_ok());
}

#[test]
fn missing_chain_settings_preserve_old_settings_and_invalid_enabled_settings_refuse() {
    let _root = crate::store::test_root("chain-settings-integration");
    let path = crate::store::settings_path().unwrap();
    std::fs::write(&path, r#"{"theme":"light","extra_args":"--lang=de","data_root":"D:/synthetic-data","api_port":42000}"#).unwrap();
    let old = super::saved_settings().unwrap();
    assert!(!old.proxy_chain.enabled);
    assert_eq!(old.theme, "light");
    assert_eq!(old.extra_args, "--lang=de");
    assert_eq!(old.data_root.as_deref(), Some("D:/synthetic-data"));
    assert_eq!(old.api_port, 42000);

    for invalid in [
        r#"{"proxy_chain":{"enabled":true,"port":"bad"}}"#,
        r#"{"proxy_chain":{"enabled":true,"mode":"future-mode"}}"#,
        r#"{"proxy_chain":null}"#,
    ] {
        std::fs::write(&path, invalid).unwrap();
        let saved = super::saved_settings().unwrap();
        assert!(saved.proxy_chain.enabled);
        assert!(saved.proxy_chain.hop(&saved.proxy_chain).is_err());
    }
    std::fs::write(&path, b"not valid JSON").unwrap();
    assert!(
        super::saved_settings().is_err(),
        "unreadable settings cannot become direct mode"
    );
    std::fs::write(&path, b"{}").unwrap();
    assert!(!super::saved_settings().unwrap().proxy_chain.enabled);
}

#[test]
fn profile_policy_survives_disk_and_export_without_changing_fingerprint() {
    use crate::space::profile::{save_here, load_raw, SaveOpts, StoredProfile};
    use super::policy::ChainPolicy;
    let _root = crate::store::test_root("chain-profile-policy-disk");
    let mut stored: StoredProfile = serde_json::from_value(json!({"name":"Synthetic policy", "navigator":{"platform":"Win32"},"_meta":{"proxy_id":"kept-proxy","proxy_chain_policy":"disabled"}})).unwrap();
    let row = save_here(stored, SaveOpts::default()).unwrap();
    assert_eq!(row.proxy_chain_policy, ChainPolicy::Disabled);
    stored = load_raw(&row.id).unwrap();
    let fingerprint = stored.config.clone();
    stored.meta.proxy_chain_policy = ChainPolicy::Enabled;
    save_here(stored, SaveOpts::default()).unwrap();
    let saved = load_raw(&row.id).unwrap();
    assert_eq!(saved.meta.proxy_chain_policy, ChainPolicy::Enabled);
    assert_eq!(saved.meta.proxy_id.as_deref(), Some("kept-proxy"));
    assert_eq!(saved.config, fingerprint);
    let roundtrip: StoredProfile = serde_json::from_value(serde_json::to_value(&saved).unwrap()).unwrap();
    assert_eq!(roundtrip.meta.proxy_chain_policy, ChainPolicy::Enabled);
    let mut bad = roundtrip;
    bad.meta.proxy_chain_policy = ChainPolicy::Invalid;
    assert!(save_here(bad, SaveOpts::default()).is_err());
    assert_eq!(load_raw(&row.id).unwrap().meta.proxy_chain_policy, ChainPolicy::Enabled);
}

#[tokio::test]
async fn thirty_profiles_and_saved_data_survive_tunnel_preparation_without_rewriting_originals() {
    let _root = crate::store::test_root("chain-thirty-profiles");
    let first = second_hop(false).await;
    let mut originals = Vec::new();
    let mut entries = Vec::new();
    for n in 0..30 {
        let id = format!("saved-profile-{n:02}");
        let proxy_id = format!("saved-proxy-{n:02}");
        let path = profile(&id, &proxy_id);
        let bytes = std::fs::read(&path).unwrap();
        let user_data = crate::store::user_data_root().unwrap().join(&id);
        for relative in [
            "Default/Cookies",
            "Default/Local Storage/leveldb/000001.log",
            "Default/IndexedDB/000001.log",
            "fingerprint.json",
        ] {
            let data = user_data.join(relative);
            std::fs::create_dir_all(data.parent().unwrap()).unwrap();
            let content = format!("synthetic-user-data-{n}-{relative}").into_bytes();
            std::fs::write(&data, &content).unwrap();
            originals.push((data, content));
        }
        originals.push((path, bytes));
        entries.push(entry(&proxy_id, first.addr));
    }
    let proxy_bytes = save_proxies(&entries);
    let loaded = crate::space::profile::list_all().unwrap();
    assert_eq!(loaded.len(), 30);
    // Reopening is a fresh file read; all original IDs and bindings survive.
    for n in 0..30 {
        let id = format!("saved-profile-{n:02}");
        let reopened = crate::space::profile::load_raw(&id).unwrap();
        assert_eq!(reopened.meta.id, id);
        assert_eq!(
            reopened.meta.proxy_id.as_deref(),
            Some(format!("saved-proxy-{n:02}").as_str())
        );
    }
    let a = first_hop(first.addr).await;
    for proxy in entries.iter().take(3) {
        let before = serde_json::to_value(proxy).unwrap();
        let prepared = super::prepare_with_hop(
            proxy,
            ProxyHop {
                kind: ProxyKind::Http,
                host: a.addr.ip().to_string(),
                port: a.addr.port(),
                username: String::new(),
                password: String::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(prepared.effective.host, "127.0.0.1");
        assert!(prepared.effective.id.is_empty());
        assert_eq!(prepared.effective.username, proxy.username);
        assert_eq!(prepared.effective.password, proxy.password);
        assert_eq!(
            serde_json::to_value(proxy).unwrap(),
            before,
            "in-memory original must stay untouched"
        );
        prepared.tunnel.shutdown().await;
    }
    assert_eq!(
        std::fs::read(crate::store::proxies_path().unwrap()).unwrap(),
        proxy_bytes
    );
    for (path, expected) in originals {
        assert_eq!(
            std::fs::read(path).unwrap(),
            expected,
            "saved data was rewritten"
        );
    }
    assert_eq!(crate::space::profile::list_all().unwrap().len(), 30);
}

#[cfg(windows)]
#[tokio::test]
async fn first_hop_failure_prevents_the_real_launch_path_from_spawning() {
    let _serial = crate::migrate::serial();
    let root = crate::store::test_root("chain-launch-first-fails");
    let second = second_hop(false).await;
    let unavailable = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = unavailable.local_addr().unwrap();
    let first = tokio::spawn(async move {
        let (mut socket, _) = unavailable.accept().await.unwrap();
        // A service that immediately closes CONNECT is unavailable, while
        // its address remains ours instead of being recycled by another test.
        let _ = http_head(&mut socket).await;
        let _ = socket.shutdown().await;
    });
    let p = entry("bad-a-proxy", second.addr);
    save_proxies(std::slice::from_ref(&p));
    profile("bad-a-profile", &p.id);
    crate::settings::save(&crate::settings::Settings {
        proxy_chain: chain(a),
        ..Default::default()
    })
    .unwrap();
    let (engine, marker) = marker_engine(&root.path);
    let error = crate::launch::launch_with(engine, "bad-a-profile", false)
        .await
        .err()
        .expect("first hop must refuse launch");
    assert_eq!(
        error.downcast_ref::<ChainError>().map(|e| e.code),
        Some("system_proxy_io_failed")
    );
    tokio::time::timeout(WAIT, first).await.unwrap().unwrap();
    assert!(!marker.exists());
    assert!(!crate::process::Tracker::shared().is_running("bad-a-profile"));
    assert_eq!(
        second.accepted.load(Ordering::SeqCst),
        0,
        "never connect directly to second hop"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn second_hop_authentication_failure_prevents_spawn_and_keeps_saved_credentials() {
    let _serial = crate::migrate::serial();
    let root = crate::store::test_root("chain-launch-second-fails");
    let second = second_hop(true).await;
    let a = first_hop(second.addr).await;
    let p = entry("bad-b-proxy", second.addr);
    let saved = save_proxies(std::slice::from_ref(&p));
    let profile = profile("bad-b-profile", &p.id);
    let before = std::fs::read(&profile).unwrap();
    crate::settings::save(&crate::settings::Settings {
        proxy_chain: chain(a.addr),
        ..Default::default()
    })
    .unwrap();
    let (engine, marker) = marker_engine(&root.path);
    let error = tokio::time::timeout(
        WAIT,
        crate::launch::launch_with(engine, "bad-b-profile", false),
    )
    .await
    .unwrap()
    .err()
    .expect("second hop must refuse launch");
    assert_eq!(
        error.downcast_ref::<ChainError>().map(|e| e.code),
        Some("proxy_chain_profile_auth_failed")
    );
    assert!(!marker.exists());
    assert!(!crate::process::Tracker::shared().is_running("bad-b-profile"));
    assert!(
        a.accepted.load(Ordering::SeqCst) >= 2,
        "first-hop preflight and actual profile probe both use A"
    );
    assert_eq!(
        std::fs::read(crate::store::proxies_path().unwrap()).unwrap(),
        saved
    );
    assert_eq!(std::fs::read(profile).unwrap(), before);
}

/// The parent starts this exact test in its own process, with no profile or
/// network secrets. A control socket proves the child is alive. The normal
/// test run has no control address and returns immediately.
#[test]
fn tracker_child_waits_for_its_control_connection() {
    let Ok(addr) = std::env::var("SHARD_CHAIN_SYNTHETIC_CHILD_CONTROL") else {
        return;
    };
    let mut socket = std::net::TcpStream::connect(addr).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut byte = [0];
    let _ = std::io::Read::read_exact(&mut socket, &mut byte);
}

async fn tracked_child(id: &str, tunnel: Tunnel) -> TcpStream {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut cmd = tokio::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "proxy_chain::integration_tests::tracker_child_waits_for_its_control_connection",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(
        "SHARD_CHAIN_SYNTHETIC_CHILD_CONTROL",
        listener.local_addr().unwrap().to_string(),
    )
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }
    let child = cmd.spawn().unwrap();
    let control = tokio::time::timeout(WAIT, listener.accept())
        .await
        .unwrap()
        .unwrap()
        .0;
    let udd = crate::store::user_data_root().unwrap().join(id);
    std::fs::create_dir_all(&udd).unwrap();
    crate::process::Tracker::shared().track_with_chain(
        id.into(),
        child,
        udd,
        crate::process::ExitHook::Local,
        Vec::new(),
        Some(tunnel),
    );
    control
}

#[tokio::test]
async fn tracker_owns_three_profile_tunnels_and_stopping_one_preserves_the_other_two() {
    let _serial = crate::migrate::serial();
    let _root = crate::store::test_root("chain-tracker-lifecycle");
    let second = second_hop(false).await;
    let first = first_hop(second.addr).await;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let ids = [
        format!("tracker-a-{id}"),
        format!("tracker-b-{id}"),
        format!("tracker-c-{id}"),
    ];
    let mut controls = Vec::new();
    let mut browsers = Vec::new();
    let mut ports = Vec::new();
    let mut closed_signals = Vec::new();
    for id in &ids {
        profile(id, "synthetic-proxy");
        let tunnel = Tunnel::bind(
            ProxyHop {
                kind: ProxyKind::Http,
                host: first.addr.ip().to_string(),
                port: first.addr.port(),
                username: String::new(),
                password: String::new(),
            },
            Target::new("fixed-second.invalid", 1080).unwrap(),
        )
        .await
        .unwrap();
        ports.push(tunnel.addr());
        closed_signals.push(tunnel.closed_signal());
        let mut browser = TcpStream::connect(tunnel.addr()).await.unwrap();
        roundtrip(&mut browser, id.as_bytes()).await;
        controls.push(tracked_child(id, tunnel).await);
        browsers.push(browser);
    }
    assert_ne!(ports[0], ports[1]);
    assert_ne!(ports[0], ports[2]);
    assert_ne!(ports[1], ports[2]);
    let tracker = crate::process::Tracker::shared();
    for id in &ids {
        assert!(tracker.is_running(id));
        assert!(tracker.is_chained(id));
    }
    assert!(
        tokio::time::timeout(WAIT, tracker.kill(&ids[1]))
            .await
            .unwrap()
            .unwrap(),
        "Stop must acknowledge the owned child"
    );
    eventually(|| !tracker.is_running(&ids[1])).await;
    assert!(!tracker.is_chained(&ids[1]));
    // An ephemeral port may already belong to another parallel test after
    // release. Follow the original listener's completion signal and original
    // established socket, not a fresh connection to a recycled port number.
    assert!(
        *closed_signals[1].borrow(),
        "stopped profile listener must finish before Tracker reports stopped"
    );
    closed(&mut browsers[1]).await;
    roundtrip(&mut browsers[0], b"a-still-proxied").await;
    roundtrip(&mut browsers[2], b"c-still-proxied").await;
    // The other two end naturally via their control sockets. Tracker must
    // perform the same cleanup when no Stop button was involved.
    controls[0].write_all(&[1]).await.unwrap();
    controls[2].write_all(&[1]).await.unwrap();
    eventually(|| !tracker.is_running(&ids[0]) && !tracker.is_running(&ids[2])).await;
    for (signal, browser) in closed_signals.iter().zip(&mut browsers) {
        assert!(*signal.borrow(), "original listener outlived its session");
        closed(browser).await;
    }
}
