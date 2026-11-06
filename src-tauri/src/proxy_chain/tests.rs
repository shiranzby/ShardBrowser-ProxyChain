//! Synthetic loopback services only. None of these tests reads settings,
//! credentials, the system proxy configuration or the user's profiles.

use super::connector::{self, ProxyHop, ProxyKind, Target};
use super::tunnel::Tunnel;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

const SHORT: Duration = Duration::from_secs(3);

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(SHORT, future)
        .await
        .expect("loopback test deadline")
}

fn hop(addr: SocketAddr, kind: ProxyKind) -> ProxyHop {
    ProxyHop {
        kind,
        host: addr.ip().to_string(),
        port: addr.port(),
        username: String::new(),
        password: String::new(),
    }
}

fn target() -> Target {
    Target::new("remote-proxy.invalid", 1080).unwrap()
}

#[tokio::test]
async fn explicit_system_listener_resolves_each_actual_connect_and_refuses_udp_and_direct_pac() {
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_addr=listener.local_addr().unwrap();
    let requests=Arc::new(std::sync::Mutex::new(Vec::new()));
    let received=requests.clone();
    let worker=tokio::spawn(async move {
        let mut clients=JoinSet::new();
        loop {tokio::select! {
            _=clients.join_next(),if !clients.is_empty()=>{},
            accepted=listener.accept()=>{let (mut socket,_)=accepted.unwrap();let received=received.clone();clients.spawn(async move{
                let head=http_head(&mut socket).await;received.lock().unwrap().push(String::from_utf8(head).unwrap());
                socket.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                let mut byte=[0];if socket.read_exact(&mut byte).await.is_ok(){socket.write_all(&byte).await.unwrap();}
            });}
        }}
    });
    let resolved=Arc::new(std::sync::Mutex::new(Vec::new()));let seen=resolved.clone();
    let resolver:super::tunnel::SystemResolver=Arc::new(move |target|{
        seen.lock().unwrap().push(target);
        Box::pin(async move {Ok(super::detect_windows::ResolvedProxy::manual(hop(first_addr,ProxyKind::Http)))})
    });
    let tunnel=Tunnel::bind_system_with_resolver(resolver).await.unwrap();
    let proxy=hop(tunnel.addr(),ProxyKind::Socks5);
    for name in ["first-destination.invalid","other-destination.invalid"] {
        let target=Target::new(name,443).unwrap();
        let mut stream=within(connector::connect(&proxy,&target)).await.unwrap();
        stream.write_all(b"x").await.unwrap();let mut echo=[0];stream.read_exact(&mut echo).await.unwrap();assert_eq!(&echo,b"x");
    }
    assert_eq!(resolved.lock().unwrap().iter().map(|t|t.host.clone()).collect::<Vec<_>>(),["first-destination.invalid","other-destination.invalid"]);
    assert!(requests.lock().unwrap()[1].starts_with("CONNECT other-destination.invalid:443 "));
    let mut udp=TcpStream::connect(tunnel.addr()).await.unwrap();udp.write_all(&[5,1,0]).await.unwrap();let mut accepted=[0;2];udp.read_exact(&mut accepted).await.unwrap();
    udp.write_all(&[5,3,0,1,127,0,0,1,0,1]).await.unwrap();let mut refused=[0;10];udp.read_exact(&mut refused).await.unwrap();assert_eq!(refused[1],7);
    assert_eq!(resolved.lock().unwrap().len(),2,"UDP cannot trigger a system route");
    tunnel.shutdown().await;worker.abort();let _=worker.await;
    let destination=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let resolver:super::tunnel::SystemResolver=Arc::new(|_|Box::pin(async {Err(super::error::ChainError::new("proxy_chain_auto_direct_refused","system_proxy","PAC returned DIRECT; explicit system routing requires a proxy."))}));
    let tunnel=Tunnel::bind_system_with_resolver(resolver).await.unwrap();
    assert!(within(connector::connect(&hop(tunnel.addr(),ProxyKind::Socks5),&Target::new("127.0.0.1",destination.local_addr().unwrap().port()).unwrap())).await.is_err());
    assert!(tokio::time::timeout(Duration::from_millis(100),destination.accept()).await.is_err(),"system refusal never dials the website directly");
    tunnel.shutdown().await;
}

#[tokio::test]
async fn explicit_https_first_hop_verifies_certificate_and_original_host() {
    use rustls::pki_types::PrivatePkcs8KeyDer;
    let certified=rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let cert=certified.cert.der().clone();let provider=Arc::new(rustls::crypto::ring::default_provider());
    let server=rustls::ServerConfig::builder_with_provider(provider.clone()).with_safe_default_protocol_versions().unwrap().with_no_client_auth()
        .with_single_cert(vec![cert.clone()],PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()).into()).unwrap();
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();
    let calls=Arc::new(AtomicUsize::new(0));let received=calls.clone();
    let task=tokio::spawn(async move {let acceptor=tokio_rustls::TlsAcceptor::from(Arc::new(server));let mut clients=JoinSet::new();loop {tokio::select!{
        _=clients.join_next(),if !clients.is_empty()=>{},
        accepted=listener.accept()=>{let (raw,_)=accepted.unwrap();let acceptor=acceptor.clone();let received=received.clone();clients.spawn(async move{
            let Ok(mut stream)=acceptor.accept(raw).await else{return;};let mut head=Vec::new();while !head.ends_with(b"\r\n\r\n"){let Ok(b)=stream.read_u8().await else{return;};head.push(b);}
            assert!(head.starts_with(b"CONNECT remote-proxy.invalid:1080 HTTP/1.1"));received.fetch_add(1,Ordering::SeqCst);stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        });}
    }}});
    let first=hop(addr,ProxyKind::Https);
    assert_eq!(within(connector::connect(&first,&target())).await.err().unwrap().code,"system_proxy_tls_failed");
    let mut roots=rustls::RootCertStore::empty();roots.add(cert).unwrap();
    let client=Arc::new(rustls::ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth());
    let mut wrong=first.clone();wrong.host="localhost".into();
    assert_eq!(within(connector::connect_with_tls_config(&wrong,&target(),SHORT,Some(client.clone()))).await.err().unwrap().code,"system_proxy_tls_failed");
    drop(within(connector::connect_with_tls_config(&first,&target(),SHORT,Some(client))).await.unwrap());
    assert_eq!(calls.load(Ordering::SeqCst),1,"only trusted matching TLS sends CONNECT");task.abort();let _=task.await;
}

async fn http_head(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        bytes.push(stream.read_u8().await.unwrap());
        assert!(bytes.len() <= 20_000);
    }
    bytes
}

async fn socks_greeting(stream: &mut TcpStream) -> Vec<u8> {
    assert_eq!(stream.read_u8().await.unwrap(), 5);
    let n = stream.read_u8().await.unwrap() as usize;
    let mut methods = vec![0; n];
    stream.read_exact(&mut methods).await.unwrap();
    methods
}

async fn socks_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = vec![0; 4];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes[..3], &[5, 1, 0]);
    let size = match bytes[3] {
        1 => 4,
        4 => 16,
        3 => {
            let n = stream.read_u8().await.unwrap();
            bytes.push(n);
            n as usize
        }
        _ => panic!("invalid test address type"),
    };
    let mut rest = vec![0; size + 2];
    stream.read_exact(&mut rest).await.unwrap();
    bytes.extend_from_slice(&rest);
    bytes
}

#[test]
fn configuration_rejects_authorities_control_characters_and_zero_ports() {
    for host in [
        "",
        "host:123",
        "https://proxy.invalid",
        "user@proxy.invalid",
        "proxy\r\nInjected: x",
        "a b",
        "a/b",
        "[bad]",
        "127.0.0.1%00",
    ] {
        assert!(Target::new(host, 80).is_err(), "must reject invalid host");
    }
    assert!(Target::new("proxy.invalid", 0).is_err());
    let mut p = hop("127.0.0.1:1234".parse().unwrap(), ProxyKind::Http);
    p.username = "unsafe:user".into();
    assert!(connector::validate(&p, &target()).is_err());
    p.username = "valid".into();
    p.password = "bad\r\n".into();
    assert!(connector::validate(&p, &target()).is_err());
}

#[test]
fn hostnames_are_idna_and_ipv6_is_not_confused_with_a_port() {
    assert_eq!(
        Target::new("[2001:db8::1]", 1080).unwrap().host,
        "2001:db8::1"
    );
    assert_eq!(
        Target::new("bücher.example", 1080).unwrap().host,
        "xn--bcher-kva.example"
    );
}

#[test]
fn socks_credentials_are_validated_by_utf8_byte_length() {
    let mut p = hop("127.0.0.1:1234".parse().unwrap(), ProxyKind::Socks5);
    p.username = "é".repeat(128);
    assert!(connector::validate(&p, &target()).is_err());
    p.username = "é".repeat(127);
    assert!(connector::validate(&p, &target()).is_ok());
    p.username.clear();
    p.password = "password".into();
    assert!(connector::validate(&p, &target()).is_err());
}

#[test]
fn debugging_and_configuration_errors_do_not_expose_credentials() {
    let mut p = hop("127.0.0.1:1234".parse().unwrap(), ProxyKind::Http);
    p.username = "secret-user".into();
    p.password = "secret-password".into();
    let debug = format!("{p:?}");
    assert!(!debug.contains("secret-user"));
    assert!(!debug.contains("secret-password"));
    p.host = "secret-password@bad".into();
    let error = connector::validate(&p, &target()).unwrap_err();
    assert!(!format!("{error:?} {error}").contains("secret-password"));
}

#[tokio::test]
async fn http_connect_preserves_pipelined_bytes_and_keeps_credentials_at_the_first_hop() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut p = hop(listener.local_addr().unwrap(), ProxyKind::Http);
    p.username = "first-user".into();
    p.password = "first-password".into();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let head = String::from_utf8(http_head(&mut stream).await).unwrap();
        assert!(head.starts_with("CONNECT remote-proxy.invalid:1080 HTTP/1.1\r\n"));
        assert!(head.contains("Host: remote-proxy.invalid:1080\r\n"));
        assert!(
            head.contains("Proxy-Authorization: Basic Zmlyc3QtdXNlcjpmaXJzdC1wYXNzd29yZA==\r\n")
        );
        stream
            .write_all(b"HTTP/1.1 200 Connection established\r\nX-Test: yes\r\n\r\n\x05\x02payload")
            .await
            .unwrap();
    });
    let mut stream = within(connector::connect(&p, &target())).await.unwrap();
    let mut payload = [0; 9];
    within(stream.read_exact(&mut payload)).await.unwrap();
    assert_eq!(&payload, b"\x05\x02payload");
    within(server).await.unwrap();
}

#[tokio::test]
async fn http_connect_formats_an_ipv6_authority() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = hop(listener.local_addr().unwrap(), ProxyKind::Http);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert!(http_head(&mut stream)
            .await
            .starts_with(b"CONNECT [2001:db8::1]:4104 HTTP/1.1\r\n"));
        stream.write_all(b"HTTP/1.0 200 OK\r\n\r\n").await.unwrap();
    });
    within(connector::connect(
        &p,
        &Target::new("2001:db8::1", 4104).unwrap(),
    ))
    .await
    .unwrap();
    within(server).await.unwrap();
}

#[tokio::test]
async fn http_refusals_are_distinct_and_never_echo_response_secrets() {
    for (response, code) in [
        (
            "HTTP/1.1 407 secret-password\r\nProxy-Authenticate: secret-password\r\n\r\n",
            "system_proxy_auth_failed",
        ),
        (
            "HTTP/1.1 403 secret-password\r\n\r\n",
            "system_proxy_connect_refused",
        ),
        (
            "HTTP/1.1 500 secret-password\r\n\r\n",
            "system_proxy_connect_refused",
        ),
        (
            "HTTP/1.1 2000 secret-password\r\n\r\n",
            "system_proxy_protocol_error",
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = hop(listener.local_addr().unwrap(), ProxyKind::Http);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            http_head(&mut stream).await;
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let error = within(connector::connect(&p, &target())).await.unwrap_err();
        assert_eq!(error.code, code);
        assert!(!format!("{error:?} {error}").contains("secret-password"));
        within(server).await.unwrap();
    }
}

#[tokio::test]
async fn http_partial_closed_and_oversized_replies_fail() {
    for response in [b"HTTP/1.1 200 OK\r\n".to_vec(), vec![b'x'; 16 * 1024 + 1]] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = hop(listener.local_addr().unwrap(), ProxyKind::Http);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            http_head(&mut stream).await;
            let _ = stream.write_all(&response).await;
        });
        assert!(within(connector::connect(&p, &target())).await.is_err());
        within(server).await.unwrap();
    }
}

#[tokio::test]
async fn a_silent_system_proxy_has_one_total_handshake_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = hop(listener.local_addr().unwrap(), ProxyKind::Http);
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let error = within(connector::connect_with_timeout(
        &p,
        &target(),
        Duration::from_millis(120),
    ))
    .await
    .unwrap_err();
    assert_eq!(error.code, "system_proxy_timeout");
    server.abort();
}

#[tokio::test]
async fn socks_no_auth_connect_leaves_the_second_proxy_bytes_untouched() {
    for bound in [
        vec![1, 127, 0, 0, 1, 0, 0],
        vec![3, 3, b'd', b'n', b's', 0, 0],
        {
            let mut b = vec![4];
            b.extend([0; 18]);
            b
        },
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = hop(listener.local_addr().unwrap(), ProxyKind::Socks5);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert_eq!(socks_greeting(&mut stream).await, [0]);
            stream.write_all(&[5, 0]).await.unwrap();
            let request = socks_request(&mut stream).await;
            assert_eq!(
                request[3], 3,
                "second proxy hostname is resolved at the first hop"
            );
            assert_eq!(&request[5..request.len() - 2], b"remote-proxy.invalid");
            let mut reply = vec![5, 0, 0];
            reply.extend(bound);
            reply.extend(b"second");
            stream.write_all(&reply).await.unwrap();
        });
        let mut stream = within(connector::connect(&p, &target())).await.unwrap();
        let mut bytes = [0; 6];
        within(stream.read_exact(&mut bytes)).await.unwrap();
        assert_eq!(&bytes, b"second");
        within(server).await.unwrap();
    }
}

#[tokio::test]
async fn socks_username_password_exchange_follows_the_selected_method() {
    for authenticate in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut p = hop(listener.local_addr().unwrap(), ProxyKind::Socks5);
        p.username = "first-user".into();
        p.password = "first-password".into();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert_eq!(socks_greeting(&mut stream).await, [0, 2]);
            stream
                .write_all(&[5, if authenticate { 2 } else { 0 }])
                .await
                .unwrap();
            if authenticate {
                assert_eq!(stream.read_u8().await.unwrap(), 1);
                let n = stream.read_u8().await.unwrap() as usize;
                let mut user = vec![0; n];
                stream.read_exact(&mut user).await.unwrap();
                assert_eq!(user, b"first-user");
                let n = stream.read_u8().await.unwrap() as usize;
                let mut pass = vec![0; n];
                stream.read_exact(&mut pass).await.unwrap();
                assert_eq!(pass, b"first-password");
                stream.write_all(&[1, 0]).await.unwrap();
            }
            socks_request(&mut stream).await;
            stream
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
        });
        within(connector::connect(&p, &target())).await.unwrap();
        within(server).await.unwrap();
    }
}

#[tokio::test]
async fn socks_authentication_and_connect_errors_have_safe_codes() {
    for (greeting, auth, reply, code) in [
        (vec![4, 0], None, None, "system_proxy_protocol_error"),
        (vec![5, 3], None, None, "system_proxy_protocol_error"),
        (vec![5, 255], None, None, "system_proxy_auth_failed"),
        (
            vec![5, 2],
            Some(vec![1, 1]),
            None,
            "system_proxy_auth_failed",
        ),
        (
            vec![5, 2],
            Some(vec![2, 0]),
            None,
            "system_proxy_protocol_error",
        ),
        (
            vec![5, 0],
            None,
            Some(vec![5, 5, 0, 1]),
            "system_proxy_connect_refused",
        ),
        (
            vec![5, 0],
            None,
            Some(vec![5, 0, 1, 1]),
            "system_proxy_protocol_error",
        ),
        (
            vec![5, 0],
            None,
            Some(vec![5, 0, 0, 9]),
            "system_proxy_protocol_error",
        ),
        (
            vec![5, 0],
            None,
            Some(vec![5, 0, 0, 3, 0]),
            "system_proxy_protocol_error",
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut p = hop(listener.local_addr().unwrap(), ProxyKind::Socks5);
        p.username = "user".into();
        p.password = "secret-password".into();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            socks_greeting(&mut stream).await;
            stream.write_all(&greeting).await.unwrap();
            if let Some(auth) = auth {
                let mut bytes = vec![0; 1 + 1 + 4 + 1 + 15];
                stream.read_exact(&mut bytes).await.unwrap();
                stream.write_all(&auth).await.unwrap();
            }
            if let Some(reply) = reply {
                socks_request(&mut stream).await;
                stream.write_all(&reply).await.unwrap();
            }
        });
        let error = within(connector::connect(&p, &target())).await.unwrap_err();
        assert_eq!(error.code, code);
        assert!(!format!("{error:?}").contains("secret-password"));
        within(server).await.unwrap();
    }
}

#[tokio::test]
async fn socks_ipv6_destination_is_encoded_as_an_ip() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = hop(listener.local_addr().unwrap(), ProxyKind::Socks5);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        socks_greeting(&mut stream).await;
        stream.write_all(&[5, 0]).await.unwrap();
        let req = socks_request(&mut stream).await;
        assert_eq!(req[3], 4);
        assert_eq!(req.len(), 22);
        assert_eq!(&req[20..], &4104u16.to_be_bytes());
        stream
            .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .await
            .unwrap();
    });
    within(connector::connect(
        &p,
        &Target::new("[2001:db8::1]", 4104).unwrap(),
    ))
    .await
    .unwrap();
    within(server).await.unwrap();
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

async fn echo() -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                result = listener.accept() => {
                    let (mut stream, _) = result.unwrap(); count.fetch_add(1, Ordering::SeqCst);
                    clients.spawn(async move {
                        let mut bytes = [0; 1024];
                        loop { match stream.read(&mut bytes).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => if stream.write_all(&bytes[..n]).await.is_err() { break; },
                        } }
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

async fn http_forward(destination: SocketAddr) -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    let task = tokio::spawn(async move {
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                _ = clients.join_next(), if !clients.is_empty() => {},
                result = listener.accept() => {
                    let (mut stream, _) = result.unwrap(); count.fetch_add(1, Ordering::SeqCst);
                    clients.spawn(async move {
                        http_head(&mut stream).await;
                        let mut second = TcpStream::connect(destination).await.unwrap();
                        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                        let _ = tokio::io::copy_bidirectional(&mut stream, &mut second).await;
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

async fn roundtrip(stream: &mut TcpStream, bytes: &[u8]) {
    within(stream.write_all(bytes)).await.unwrap();
    let mut received = vec![0; bytes.len()];
    within(stream.read_exact(&mut received)).await.unwrap();
    assert_eq!(received, bytes);
}

async fn eventually(mut ready: impl FnMut() -> bool) {
    within(async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
}

async fn closed(stream: &mut TcpStream) {
    let result = within(stream.read(&mut [0; 1])).await;
    assert!(
        matches!(result, Ok(0)) || result.is_err(),
        "connection must close"
    );
}

#[tokio::test]
async fn tunnel_is_a_transparent_fixed_target_and_shutdown_closes_active_streams() {
    let second = echo().await;
    let first = http_forward(second.addr).await;
    let tunnel = Tunnel::bind(hop(first.addr, ProxyKind::Http), target())
        .await
        .unwrap();
    assert_eq!(tunnel.addr().ip(), std::net::Ipv4Addr::LOCALHOST);
    let addr = tunnel.addr();
    let completed = tunnel.closed_signal();
    let mut browser = TcpStream::connect(addr).await.unwrap();
    // These are raw second-layer SOCKS5 greeting/auth/request bytes, not a
    // new request for the local forwarder to interpret or authenticate.
    roundtrip(
        &mut browser,
        b"\x05\x02\x00\x02\x01\x06second\x08password\x05\x01\x00\x03\x03web\x01\xbb",
    )
    .await;
    assert_eq!(first.accepted.load(Ordering::SeqCst), 1);
    assert_eq!(second.accepted.load(Ordering::SeqCst), 1);
    tunnel.shutdown().await;
    assert!(*completed.borrow());
    closed(&mut browser).await;
    // Completion belongs to this listener; a new connection or bind to its
    // old port could instead observe a port reused by a parallel fixture.
}

#[tokio::test]
async fn dropping_the_tunnel_cancels_connections_and_frees_its_listener() {
    let second = echo().await;
    let first = http_forward(second.addr).await;
    let tunnel = Tunnel::bind(hop(first.addr, ProxyKind::Http), target())
        .await
        .unwrap();
    let addr = tunnel.addr();
    let completed = tunnel.closed_signal();
    let mut browser = TcpStream::connect(addr).await.unwrap();
    roundtrip(&mut browser, b"alive").await;
    drop(tunnel);
    closed(&mut browser).await;
    eventually(|| *completed.borrow()).await;
}

#[tokio::test]
async fn three_profile_tunnels_remain_independent_when_one_is_stopped() {
    let second = echo().await;
    let first = http_forward(second.addr).await;
    let a = Tunnel::bind(hop(first.addr, ProxyKind::Http), target())
        .await
        .unwrap();
    let b = Tunnel::bind(hop(first.addr, ProxyKind::Http), target())
        .await
        .unwrap();
    let c = Tunnel::bind(hop(first.addr, ProxyKind::Http), target())
        .await
        .unwrap();
    assert_ne!(a.addr(), b.addr());
    assert_ne!(a.addr(), c.addr());
    assert_ne!(b.addr(), c.addr());
    let mut ca = TcpStream::connect(a.addr()).await.unwrap();
    let mut cb = TcpStream::connect(b.addr()).await.unwrap();
    let mut cc = TcpStream::connect(c.addr()).await.unwrap();
    roundtrip(&mut ca, b"profile-a").await;
    roundtrip(&mut cb, b"profile-b").await;
    roundtrip(&mut cc, b"profile-c").await;
    b.shutdown().await;
    closed(&mut cb).await;
    roundtrip(&mut ca, b"profile-a-still-alive").await;
    roundtrip(&mut cc, b"profile-c-still-alive").await;
    a.shutdown().await;
    c.shutdown().await;
}

#[tokio::test]
async fn a_failed_first_hop_never_connects_directly_to_the_second_hop() {
    let second = echo().await;
    // Keep the failing endpoint owned throughout the exchange. Dropping an
    // unused listener first lets another parallel test (or this tunnel's own
    // listener) take that port, so a later connect no longer tests failure.
    let refusing = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = refusing.local_addr().unwrap();
    let failure = tokio::spawn(async move {
        let (mut stream, _) = refusing.accept().await.unwrap();
        http_head(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n")
            .await
            .unwrap();
    });
    let tunnel = Tunnel::bind(
        hop(addr, ProxyKind::Http),
        Target::new(second.addr.ip().to_string(), second.addr.port()).unwrap(),
    )
    .await
    .unwrap();
    let mut browser = TcpStream::connect(tunnel.addr()).await.unwrap();
    closed(&mut browser).await;
    eventually(|| tunnel.last_error().is_some()).await;
    assert_eq!(
        tunnel.last_error().unwrap().code,
        "system_proxy_connect_refused"
    );
    assert_eq!(second.accepted.load(Ordering::SeqCst), 0);
    within(failure).await.unwrap();
    tunnel.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refused_first_hop_publishes_its_typed_error_before_loopback_eof() {
    const ROUNDS: usize = 32;
    let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = first.local_addr().unwrap();
    let refusing = tokio::spawn(async move {
        for _ in 0..ROUNDS {
            let (mut socket, _) = first.accept().await.unwrap();
            http_head(&mut socket).await;
            socket.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await.unwrap();
        }
    });
    for round in 0..ROUNDS {
        let tunnel = Tunnel::bind(hop(addr, ProxyKind::Http), target()).await.unwrap();
        let mut local = TcpStream::connect(tunnel.addr()).await.unwrap();
        let closed=within(local.read(&mut [0;1])).await;
        assert!(matches!(closed,Ok(0)) || closed.as_ref().is_err_and(|error|error.kind()==std::io::ErrorKind::ConnectionReset));
        // A retry/eventually here would hide the race this test guards.
        assert_eq!(tunnel.last_error().map(|error| error.code), Some("system_proxy_connect_refused"), "round {round}");
        tunnel.shutdown().await;
    }
    within(refusing).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_system_resolution_timeout_publishes_its_typed_error_before_loopback_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
    let (local, _) = listener.accept().await.unwrap();
    let error = Arc::new(std::sync::Mutex::new(None));
    let published = error.clone();
    let resolver: super::tunnel::SystemResolver = Arc::new(|_| Box::pin(std::future::pending()));
    let worker = tokio::spawn(async move {
        super::tunnel::system_connect(local, Duration::from_millis(50), resolver, published).await.err().unwrap()
    });
    client.write_all(&[5, 1, 0]).await.unwrap();
    let mut greeting = [0; 2];
    within(client.read_exact(&mut greeting)).await.unwrap();
    assert_eq!(greeting, [5, 0]);
    client.write_all(&[5, 1, 0, 1, 203, 0, 113, 1, 1, 187]).await.unwrap();
    let closed=within(client.read(&mut [0;1])).await;
    assert!(matches!(closed,Ok(0)) || closed.as_ref().is_err_and(|error|error.kind()==std::io::ErrorKind::ConnectionReset));
    assert_eq!(error.lock().unwrap().as_ref().map(|error| error.code), Some("system_proxy_timeout"));
    assert_eq!(within(worker).await.unwrap().code, "system_proxy_timeout");
}

#[tokio::test]
async fn quota_counts_pending_handshakes_and_is_released_after_a_flow_ends() {
    async fn at<T>(stage: &str, future: impl Future<Output = T>) -> T {
        tokio::time::timeout(SHORT, future)
            .await
            .unwrap_or_else(|_| panic!("quota test deadline: {stage}"))
    }

    let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tunnel = Tunnel::bind_with_limits(
        hop(first.local_addr().unwrap(), ProxyKind::Http),
        target(),
        connector::CONNECT_TIMEOUT,
        Duration::from_secs(300), // Product idle default; not part of this invariant.
        1,
    )
    .await
    .unwrap();
    let mut a = at("connect pending A", TcpStream::connect(tunnel.addr()))
        .await
        .expect("connect pending A");
    let (mut first_a, _) = at("accept pending A", first.accept())
        .await
        .expect("accept pending A");
    at("read A CONNECT without replying", http_head(&mut first_a)).await;
    // No 200 response: A is still in the system-proxy handshake and must
    // already own the only reservation, before any relay task can run.
    assert_eq!(tunnel.active_connections(), 1, "reserve pending A");
    let mut b = at("connect excess B", TcpStream::connect(tunnel.addr()))
        .await
        .expect("connect excess B");
    let rejected = at("reject excess B during A handshake", b.read(&mut [0; 1])).await;
    assert!(
        matches!(rejected, Ok(0)) || rejected.is_err(),
        "excess B must close during A handshake"
    );
    assert_eq!(tunnel.active_connections(), 1, "B must not own a slot");

    // Closing the owned first-hop socket gives the pending connector an
    // explicit EOF. No multi-hop relay half-close or idle timer is involved.
    drop(first_a);
    let ended = at("close original A after first-hop EOF", a.read(&mut [0; 1])).await;
    assert!(
        matches!(ended, Ok(0)) || ended.is_err(),
        "original A must close"
    );
    at("return A reservation", async {
        while tunnel.active_connections() != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    let mut c = at("connect replacement C", TcpStream::connect(tunnel.addr()))
        .await
        .expect("connect replacement C");
    let (mut first_c, _) = at("accept replacement C", first.accept())
        .await
        .expect("accept replacement C");
    at("read C CONNECT", http_head(&mut first_c)).await;
    at(
        "finish C CONNECT",
        first_c.write_all(b"HTTP/1.1 200 OK\r\n\r\n"),
    )
    .await
    .expect("finish C CONNECT");
    at("send C payload", c.write_all(b"three"))
        .await
        .expect("send C payload");
    let mut payload = [0; 5];
    at(
        "receive C payload at first hop",
        first_c.read_exact(&mut payload),
    )
    .await
    .expect("receive C payload at first hop");
    assert_eq!(&payload, b"three");
    at("echo C payload", first_c.write_all(&payload))
        .await
        .expect("echo C payload");
    let mut echoed = [0; 5];
    at("receive C echo", c.read_exact(&mut echoed))
        .await
        .expect("receive C echo");
    assert_eq!(&echoed, b"three");
    assert_eq!(tunnel.active_connections(), 1, "C reused the reservation");
    tunnel.shutdown().await;
}

#[tokio::test]
async fn flow_idle_timeout_is_reset_by_traffic_in_either_direction() {
    let second = echo().await;
    let first = http_forward(second.addr).await;
    let idle = Duration::from_millis(180);
    let tunnel =
        Tunnel::bind_with_limits(hop(first.addr, ProxyKind::Http), target(), SHORT, idle, 3)
            .await
            .unwrap();
    let mut browser = TcpStream::connect(tunnel.addr()).await.unwrap();
    for _ in 0..6 {
        roundtrip(&mut browser, b"active").await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    closed(&mut browser).await;
    eventually(|| tunnel.last_error().is_some()).await;
    assert_eq!(
        tunnel.last_error().unwrap().code,
        "proxy_chain_idle_timeout"
    );
    eventually(|| tunnel.active_connections() == 0).await;
    tunnel.shutdown().await;
}

#[tokio::test]
async fn shutdown_cancels_a_pending_first_hop_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tunnel = Tunnel::bind(
        hop(listener.local_addr().unwrap(), ProxyKind::Socks5),
        target(),
    )
    .await
    .unwrap();
    let mut browser = TcpStream::connect(tunnel.addr()).await.unwrap();
    let (mut first, _) = within(listener.accept()).await.unwrap();
    socks_greeting(&mut first).await;
    tunnel.shutdown().await;
    closed(&mut browser).await;
    closed(&mut first).await;
}

#[tokio::test]
async fn a_shared_tunnel_is_fully_closed_before_close_and_wait_returns() {
    let second = echo().await;
    let first = http_forward(second.addr).await;
    let tunnel = std::sync::Arc::new(
        Tunnel::bind(hop(first.addr, ProxyKind::Http), target())
            .await
            .unwrap(),
    );
    let addr = tunnel.addr();
    let completed = tunnel.closed_signal();
    let mut browser = TcpStream::connect(addr).await.unwrap();
    roundtrip(&mut browser, b"profile-is-still-running").await;
    let tracker_copy = tunnel.clone();
    within(tracker_copy.close_and_wait()).await.unwrap();
    assert!(*completed.borrow());
    assert_eq!(tunnel.active_connections(), 0);
    closed(&mut browser).await;
    // An exit path and a tracker monitor may both request closure.
    within(tunnel.close_and_wait()).await.unwrap();
}

#[tokio::test]
async fn a_greeting_timeout_keeps_its_type_under_the_probe_context() {
    let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tunnel = Tunnel::bind(hop(first.local_addr().unwrap(), ProxyKind::Http), target()).await.unwrap();
    let elapsed=tokio::time::timeout(Duration::from_millis(1),std::future::pending::<()>()).await.unwrap_err();
    let cause=anyhow::Error::new(elapsed).context("the proxy accepted the connection but never greeted us");
    assert_eq!(super::profile_failure(&tunnel,&cause).code,"proxy_chain_profile_timeout");
    tunnel.shutdown().await;
}
