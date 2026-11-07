//! Owned loopback proxies and trusted local TLS pages; no public traffic.
use super::{connector::{ProxyHop,ProxyKind as HopKind,Target},readiness,PreparedProxy,Route};
use crate::space::proxy::ProxyEntry;
use base64::{engine::general_purpose::STANDARD,Engine as _};
use serde_json::json;
use std::{net::SocketAddr,sync::{Arc,atomic::{AtomicUsize,Ordering}},time::Duration};
use tokio::{io::{AsyncRead,AsyncReadExt,AsyncWriteExt},net::{TcpListener,TcpStream},task::{JoinHandle,JoinSet}};

const HOST:&str="readiness.synthetic.test";
const USER:&str="readiness-synthetic";
const PASSWORD:&str="readiness synthetic password";
const BODY:&str="<!doctype html><title>Ready</title><main>Actual same-route HTTPS content</main>";

#[derive(Default)]
struct Counts {geo:AtomicUsize,content:AtomicUsize,accepted:AtomicUsize,active:AtomicUsize}
struct Active(Arc<Counts>);
impl Drop for Active {fn drop(&mut self){self.0.active.fetch_sub(1,Ordering::SeqCst);}}
struct Service {addr:SocketAddr,counts:Arc<Counts>,task:JoinHandle<()>}
impl Drop for Service {fn drop(&mut self){self.task.abort();}}

#[derive(Clone)]
struct Options {
    tls:Arc<rustls::ServerConfig>,
    geo_refuses_auth:bool,
    probe_refuses:bool,
    content_status:u16,
    content_body:&'static str,
    chain_refusal:Option<SocketAddr>,
}

async fn head(stream:&mut(impl AsyncRead+Unpin))->std::io::Result<String> {
    let mut bytes=Vec::new();
    while !bytes.ends_with(b"\r\n\r\n")&&bytes.len()<16384 {bytes.push(stream.read_u8().await?);}
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn response(stream:&mut TcpStream,status:u16)->std::io::Result<()> {
    stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await
}

fn tls_config(cert_host:&str)->(Arc<rustls::ServerConfig>,Arc<rustls::ClientConfig>) {
    let generated=rcgen::generate_simple_self_signed(vec![cert_host.into()]).unwrap();
    let cert=generated.cert.der().clone();
    let provider=Arc::new(rustls::crypto::ring::default_provider());
    let server=rustls::ServerConfig::builder_with_provider(provider.clone()).with_safe_default_protocol_versions().unwrap().with_no_client_auth()
        .with_single_cert(vec![cert.clone()],rustls::pki_types::PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der()).into()).unwrap();
    let mut roots=rustls::RootCertStore::empty();roots.add(cert).unwrap();
    let client=rustls::ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions().unwrap().with_root_certificates(roots).with_no_client_auth();
    (Arc::new(server),Arc::new(client))
}

fn options(tls:Arc<rustls::ServerConfig>)->Options {
    Options{tls,geo_refuses_auth:false,probe_refuses:false,content_status:200,content_body:BODY,chain_refusal:None}
}

async fn service(options:Options)->Service {
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr=listener.local_addr().unwrap();let counts=Arc::new(Counts::default());let record=counts.clone();
    let task=tokio::spawn(async move {
        let mut clients=JoinSet::new();
        loop {tokio::select! {
            _=clients.join_next(),if !clients.is_empty()=>{},
            accepted=listener.accept()=>{
                let Ok((mut stream,_))=accepted else {break;};
                record.accepted.fetch_add(1,Ordering::SeqCst);record.active.fetch_add(1,Ordering::SeqCst);
                let counts=record.clone();let options=options.clone();
                clients.spawn(async move {
                    let _active=Active(counts.clone());
                    let Ok(request)=head(&mut stream).await else {return;};
                    let authorization=format!("Basic {}",STANDARD.encode(format!("{USER}:{PASSWORD}")));
                    if !request.lines().any(|line|line.split_once(':').is_some_and(|(name,value)|name.eq_ignore_ascii_case("Proxy-Authorization")&&value.trim()==authorization)) {
                        let _=response(&mut stream,407).await;return;
                    }
                    if options.chain_refusal.is_some_and(|target|request.starts_with(&format!("CONNECT {target} "))) {
                        let _=response(&mut stream,502).await;return;
                    }
                    if request.starts_with("CONNECT example.com:443 ") {
                        let _=response(&mut stream,if options.probe_refuses{502}else{200}).await;return;
                    }
                    if request.starts_with(&format!("CONNECT {HOST}:443 ")) {
                        counts.content.fetch_add(1,Ordering::SeqCst);
                        if stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.is_err(){return;}
                        let Ok(mut secure)=tokio_rustls::TlsAcceptor::from(options.tls).accept(&mut stream).await else {return;};
                        let Ok(request)=head(&mut secure).await else {return;};
                        assert!(request.starts_with("GET /health HTTP/1.1\r\n"));
                        let location=if options.content_status==302{"Location: https://redirect.synthetic.test/\r\n"}else{""};
                        let _=secure.write_all(format!("HTTP/1.1 {} Fixture\r\nContent-Type: text/html\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n{}",options.content_status,options.content_body.len(),options.content_body).as_bytes()).await;
                        let _=secure.shutdown().await;return;
                    }
                    if request.starts_with("CONNECT ip-api.com:80 ") {
                        counts.geo.fetch_add(1,Ordering::SeqCst);
                        if options.geo_refuses_auth {let _=response(&mut stream,407).await;return;}
                        if stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.is_err(){return;}
                        let Ok(request)=head(&mut stream).await else {return;};
                        assert!(request.starts_with("GET /json/?fields="));
                    } else {
                        if request.starts_with("CONNECT ") {let _=response(&mut stream,502).await;return;}
                        counts.geo.fetch_add(1,Ordering::SeqCst);
                        if options.geo_refuses_auth {let _=response(&mut stream,407).await;return;}
                    }
                    let body=r#"{"status":"fail","message":"synthetic Geo service unavailable"}"#;
                    let _=stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await;
                });
            }
        }}
    });
    Service{addr,counts,task}
}

fn entry(service:&Service)->ProxyEntry {
    serde_json::from_value(json!({"id":"readiness-assigned","name":"Synthetic readiness proxy","kind":"http","host":service.addr.ip().to_string(),"port":service.addr.port(),"username":USER,"password":PASSWORD})).unwrap()
}

async fn prepared(entry:&ProxyEntry)->PreparedProxy {
    let tunnel=super::Tunnel::bind_assigned(Target::new(&entry.host,entry.port).unwrap(),None).await.unwrap();
    PreparedProxy{effective:super::effective_entry(entry,&tunnel),tunnel,route:Route::ExplicitProxy,first_hop_error:None,verified_geo:None,verified_https:false,geo_failure:None,verified_tcp_ms:None}
}

fn configure(client:Arc<rustls::ClientConfig>)->readiness::TestConfiguration {
    crate::settings::save(&crate::settings::Settings{geo_checker:Some("ip-api.com".into()),..Default::default()}).unwrap();
    readiness::test_configuration(vec![format!("https://{HOST}/health")],client)
}

async fn cleanup(prepared:PreparedProxy,service:&Service) {
    let addr=prepared.tunnel.addr();prepared.tunnel.shutdown().await;
    assert!(TcpStream::connect(addr).await.is_err());
    tokio::time::timeout(Duration::from_secs(2),async {while service.counts.active.load(Ordering::SeqCst)!=0 {tokio::time::sleep(Duration::from_millis(5)).await;}}).await.unwrap();
}

#[tokio::test]
async fn geo_failure_keeps_a_same_route_strict_https_response_ready_without_an_exit_ip() {
    let _root=crate::store::test_root("https-ready-no-geo");
    let (server,client)=tls_config(HOST);let _config=configure(client);
    let service=service(options(server)).await;let assigned=entry(&service);
    let original=serde_json::to_value(&assigned).unwrap();let mut prepared=prepared(&assigned).await;
    super::verify_prepared(&mut prepared).await.unwrap();
    assert!(prepared.is_ready()&&prepared.verified_https&&prepared.verified_tcp_ms.is_some());
    assert!(prepared.verified_geo.is_none());
    let geo=prepared.exit_result().unwrap_err();
    assert_eq!(geo.downcast_ref::<super::ChainError>().unwrap().code,"proxy_chain_profile_geo_failed");
    assert_eq!(prepared.geo_failure.as_ref().unwrap().layer,"exit");
    assert!(prepared.geo_failure.as_ref().unwrap().geo_failures.is_some());
    assert_eq!(service.counts.content.load(Ordering::SeqCst),1);
    assert_eq!(serde_json::to_value(&assigned).unwrap(),original);
    cleanup(prepared,&service).await;
}

#[tokio::test]
async fn detection_keeps_https_connectivity_and_unknown_geo_as_distinct_results() {
    let _root=crate::store::test_root("https-ready-detection");
    let (server,client)=tls_config(HOST);let _config=configure(client);
    let service=service(options(server)).await;let assigned=entry(&service);
    let result=super::routing::test_resolved_route(Some(&super::route::ProfileProxyRoute::Proxy),super::policy::ChainPolicy::Enabled,Some(&assigned),None).await.unwrap();
    assert_eq!(result.profile_proxy.state,"ok");assert_eq!(result.system_proxy.state,"not_tested");
    assert_eq!(result.route,Some(Route::ExplicitProxy));
    assert_eq!(result.exit_error_code,Some("proxy_chain_profile_geo_failed"));
    assert!(result.exit_ip.is_none()&&result.exit_geo.is_none());
    assert_eq!(service.counts.content.load(Ordering::SeqCst),1);
}

#[tokio::test]
async fn https_can_prove_transport_after_a_probe_target_refusal_but_geo_stays_unknown() {
    let _root=crate::store::test_root("https-ready-probe-refusal");
    let (server,client)=tls_config(HOST);let _config=configure(client);
    let mut setup=options(server);setup.probe_refuses=true;
    let service=service(setup).await;let mut prepared=prepared(&entry(&service)).await;
    super::verify_prepared(&mut prepared).await.unwrap();
    assert!(prepared.is_ready()&&prepared.verified_https&&prepared.verified_tcp_ms.is_some());
    assert_eq!(prepared.geo_failure.as_ref().unwrap().code,"proxy_chain_profile_geo_failed");
    cleanup(prepared,&service).await;
}

#[tokio::test]
async fn an_untrusted_certificate_or_wrong_hostname_cannot_prove_readiness() {
    let _root=crate::store::test_root("https-ready-cert-refusal");
    for hostname in [HOST,"wrong.synthetic.test"] {
        let (server,trusted)=tls_config(hostname);
        let client=if hostname==HOST {super::tls::client().unwrap()}else{trusted};
        let _config=configure(client);let service=service(options(server)).await;
        let mut prepared=prepared(&entry(&service)).await;
        let error=super::verify_prepared(&mut prepared).await.unwrap_err();
        assert_eq!(error.code,"proxy_chain_profile_geo_failed");assert!(!prepared.is_ready());
        assert!(prepared.verified_geo.is_none()&&!prepared.verified_https);
        cleanup(prepared,&service).await;
    }
}

#[tokio::test]
async fn missing_content_empty_content_and_redirects_cannot_replace_geo() {
    let _root=crate::store::test_root("https-ready-body-refusal");
    for(status,body)in [(503,BODY),(200,""),(302,BODY)] {
        let(server,client)=tls_config(HOST);let _config=configure(client);
        let mut setup=options(server);setup.content_status=status;setup.content_body=body;
        let service=service(setup).await;let mut prepared=prepared(&entry(&service)).await;
        assert!(super::verify_prepared(&mut prepared).await.is_err());assert!(!prepared.is_ready());
        assert_eq!(service.counts.content.load(Ordering::SeqCst),1,"no redirect target may be connected");
        cleanup(prepared,&service).await;
    }
}

#[tokio::test]
async fn an_explicit_authentication_refusal_is_not_retried_as_https() {
    let _root=crate::store::test_root("https-ready-auth-refusal");
    let(server,client)=tls_config(HOST);let _config=configure(client);
    let service=service(options(server)).await;let mut assigned=entry(&service);assigned.password="incorrect synthetic password".into();
    let mut prepared=prepared(&assigned).await;
    assert_eq!(super::verify_prepared(&mut prepared).await.unwrap_err().code,"proxy_chain_profile_auth_failed");
    assert_eq!(service.counts.geo.load(Ordering::SeqCst),0);assert_eq!(service.counts.content.load(Ordering::SeqCst),0);
    assert!(!prepared.is_ready());cleanup(prepared,&service).await;
}

#[tokio::test]
async fn authentication_refused_during_geo_does_not_run_a_working_https_fallback() {
    let _root=crate::store::test_root("https-ready-geo-auth-refusal");
    let(server,client)=tls_config(HOST);let _config=configure(client);
    let mut setup=options(server);setup.geo_refuses_auth=true;
    let service=service(setup).await;let mut prepared=prepared(&entry(&service)).await;
    assert_eq!(super::verify_prepared(&mut prepared).await.unwrap_err().code,"proxy_chain_profile_auth_failed");
    assert!(service.counts.geo.load(Ordering::SeqCst)>0);assert_eq!(service.counts.content.load(Ordering::SeqCst),0);
    assert!(!prepared.is_ready());cleanup(prepared,&service).await;
}

#[tokio::test]
async fn a_first_hop_auth_refusal_hidden_by_the_local_socks_reply_stops_https_retries() {
    let _root=crate::store::test_root("https-ready-first-hop-auth");
    let _config=readiness::test_configuration(vec![format!("https://{HOST}/health"),"https://second.synthetic.test/health".into()],super::tls::client().unwrap());
    let calls=Arc::new(AtomicUsize::new(0));let observed=calls.clone();
    let resolver:super::tunnel::SystemResolver=Arc::new(move|_|{
        observed.fetch_add(1,Ordering::SeqCst);
        Box::pin(async {Err(super::ChainError::new("system_proxy_auth_failed","system_proxy","Synthetic first-hop authentication refusal."))})
    });
    let tunnel=super::Tunnel::bind_system_with_resolver(resolver).await.unwrap();
    let entry:ProxyEntry=serde_json::from_value(json!({"name":"Synthetic local relay","kind":"socks5","host":tunnel.addr().ip().to_string(),"port":tunnel.addr().port()})).unwrap();
    let error=readiness::verify(&entry,&tunnel).await.unwrap_err();
    assert_eq!(error.code,"system_proxy_auth_failed");
    assert_eq!(calls.load(Ordering::SeqCst),1,"the second HTTPS target cannot hide or retry an explicit first-hop refusal");
    tunnel.shutdown().await;
}

#[tokio::test]
async fn chain_failure_does_not_test_a_separate_https_ready_candidate() {
    let _root=crate::store::test_root("https-ready-no-same-exit");
    let(server,client)=tls_config(HOST);let _config=configure(client);
    let second=service(options(server.clone())).await;
    let mut setup=options(server);setup.chain_refusal=Some(second.addr);
    let first=service(setup).await;let assigned=entry(&second);
    let hop=ProxyHop{kind:HopKind::Http,host:first.addr.ip().to_string(),port:first.addr.port(),username:USER.into(),password:PASSWORD.into()};
    let error=super::prepare_checked_chain(&assigned,hop).await.err().unwrap();
    assert_eq!(error.downcast_ref::<super::ChainError>().unwrap().code,"system_proxy_connect_refused");
    assert_eq!(first.counts.geo.load(Ordering::SeqCst),0,"a failed chain cannot query the first hop's independent exit");
    assert_eq!(first.counts.content.load(Ordering::SeqCst),0,"a failed chain cannot test the first hop as a separate HTTPS candidate");
    assert_eq!(second.counts.accepted.load(Ordering::SeqCst),0,"a failed chain cannot dial the assigned proxy directly");
}
