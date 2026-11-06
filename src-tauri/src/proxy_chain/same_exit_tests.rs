//! Local protocol evidence for route optimisation. No Internet targets resolve.
use super::{connector::{ProxyHop,ProxyKind as HopKind},PreparedProxy,Route};
use base64::{engine::general_purpose::STANDARD,Engine as _};
use serde_json::json;
use std::{net::SocketAddr,sync::{Arc,atomic::{AtomicUsize,Ordering}},time::Duration};
use tokio::{io::{AsyncReadExt,AsyncWriteExt},net::{TcpListener,TcpStream},task::{JoinHandle,JoinSet}};

const FIRST_USER:&str="first-synthetic";
const FIRST_PASS:&str="first-synthetic-password";
const SECOND_USER:&str="second-synthetic";
const SECOND_PASS:&str="second-synthetic-password";

#[derive(Clone)]
struct Leaf {ip:&'static str,user:&'static str,password:&'static str,valid_geo:bool,reject_auth:bool,hang_geo:bool}
impl Leaf {
    fn first(ip:&'static str)->Self {Self{ip,user:FIRST_USER,password:FIRST_PASS,valid_geo:true,reject_auth:false,hang_geo:false}}
    fn second(ip:&'static str)->Self {Self{ip,user:SECOND_USER,password:SECOND_PASS,valid_geo:true,reject_auth:false,hang_geo:false}}
}
#[derive(Clone)]
enum Chain {Relay(SocketAddr),Inline(Leaf),Refuse}
#[derive(Default)]
struct Counts {accepted:AtomicUsize,active:AtomicUsize,chained:AtomicUsize,geos:AtomicUsize}
struct Active(Arc<Counts>);
impl Drop for Active {fn drop(&mut self){self.0.active.fetch_sub(1,Ordering::SeqCst);}}
struct Service {addr:SocketAddr,counts:Arc<Counts>,task:JoinHandle<()>}
impl Drop for Service {fn drop(&mut self){self.task.abort();}}

async fn head(stream:&mut TcpStream)->std::io::Result<String> {
    let mut bytes=Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") && bytes.len()<16384 {bytes.push(stream.read_u8().await?);}
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
fn authorized(head:&str,leaf:&Leaf)->bool {
    let expected=format!("proxy-authorization: basic {}",STANDARD.encode(format!("{}:{}",leaf.user,leaf.password)));
    head.lines().any(|line|line.to_ascii_lowercase()==expected.to_ascii_lowercase()) && !leaf.reject_auth
}
async fn refuse(stream:&mut TcpStream,status:u16)->std::io::Result<()> {
    stream.write_all(format!("HTTP/1.1 {status} Synthetic refusal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await
}
async fn leaf_response(stream:&mut TcpStream,request:&str,leaf:&Leaf,counts:&Counts)->std::io::Result<()> {
    if !authorized(request,leaf) {return refuse(stream,407).await;}
    if request.starts_with("CONNECT example.com:443 ") {return stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await;}
    if request.starts_with("CONNECT ") {return refuse(stream,502).await;}
    counts.geos.fetch_add(1,Ordering::SeqCst);
    if leaf.hang_geo {let _=stream.read(&mut [0;1]).await?;return Ok(());}
    let body=if leaf.valid_geo {json!({"status":"success","query":leaf.ip,"country":"Synthetic","countryCode":"ZZ","regionName":"Synthetic","city":"Synthetic","timezone":"Etc/UTC","isp":"Synthetic","lat":1,"lon":2}).to_string()}
        else {json!({"status":"fail"}).to_string()};
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await
}
async fn service(leaf:Leaf,chain:Option<(SocketAddr,Chain)>,reject_direct:bool)->Service {
    let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr=listener.local_addr().unwrap();let counts=Arc::new(Counts::default());let record=counts.clone();
    let task=tokio::spawn(async move {
        let mut clients=JoinSet::new();
        loop {tokio::select! {
            _=clients.join_next(),if !clients.is_empty()=>{},
            accepted=listener.accept()=>{
                let Ok((mut stream,_))=accepted else {break;};
                record.accepted.fetch_add(1,Ordering::SeqCst);record.active.fetch_add(1,Ordering::SeqCst);
                let counts=record.clone();let leaf=leaf.clone();let chain=chain.clone();
                clients.spawn(async move {
                    let _active=Active(counts.clone());
                    if reject_direct {return;}
                    let Ok(request)=head(&mut stream).await else {return;};
                    if !authorized(&request,&leaf) {let _=refuse(&mut stream,407).await;return;}
                    if let Some((target,action))=chain {
                        if request.starts_with(&format!("CONNECT {target} ")) {
                            counts.chained.fetch_add(1,Ordering::SeqCst);
                            if matches!(action,Chain::Refuse) {let _=refuse(&mut stream,502).await;return;}
                            if stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.is_err(){return;}
                            match action {
                                Chain::Relay(addr)=>if let Ok(mut remote)=TcpStream::connect(addr).await {let _=tokio::io::copy_bidirectional(&mut stream,&mut remote).await;},
                                Chain::Inline(second)=>if let Ok(request)=head(&mut stream).await {let _=leaf_response(&mut stream,&request,&second,&counts).await;},
                                Chain::Refuse=>unreachable!(),
                            }
                            return;
                        }
                    }
                    let _=leaf_response(&mut stream,&request,&leaf,&counts).await;
                });
            }
        }}
    });
    Service{addr,counts,task}
}
fn entry(addr:SocketAddr)->crate::space::proxy::ProxyEntry {
    serde_json::from_value(json!({"id":"same-exit-assigned","name":"Synthetic assigned","kind":"http","host":addr.ip().to_string(),"port":addr.port(),"username":SECOND_USER,"password":SECOND_PASS})).unwrap()
}
fn hop(first:&Service)->ProxyHop {ProxyHop{kind:HopKind::Http,host:first.addr.ip().to_string(),port:first.addr.port(),username:FIRST_USER.into(),password:FIRST_PASS.into()}}
fn preferred_geo() {crate::settings::save(&crate::settings::Settings{geo_checker:Some("ip-api.com".into()),..Default::default()}).unwrap();}
async fn shutdown(prepared:PreparedProxy) {
    let addr=prepared.tunnel.addr();prepared.tunnel.shutdown().await;
    assert!(TcpStream::connect(addr).await.is_err());
}
async fn wait_no_clients(service:&Service) {
    tokio::time::timeout(Duration::from_secs(2),async {while service.counts.active.load(Ordering::SeqCst)!=0 {tokio::time::sleep(Duration::from_millis(5)).await;}}).await.expect("owned comparison clients were not closed");
}

#[test]
fn exit_comparison_uses_valid_ip_values_not_country_or_textual_similarity() {
    assert!(super::same_exit_ip("2001:db8::1","2001:0DB8:0:0:0:0:0:1"));
    assert!(!super::same_exit_ip("198.51.100.1","198.51.100.2"));
    assert!(!super::same_exit_ip("HK","HK"));
    assert!(!super::same_exit_ip("",""));
}

#[tokio::test]
async fn matching_fresh_exits_reuse_only_the_original_assigned_proxy_without_editing_it() {
    let _root=crate::store::test_root("same-exit-retained-assigned");preferred_geo();
    let second=service(Leaf::second("198.51.100.91"),None,false).await;
    let first=service(Leaf::first("198.51.100.91"),Some((second.addr,Chain::Relay(second.addr))),false).await;
    let assigned=entry(second.addr);let before=serde_json::to_value(&assigned).unwrap();
    let prepared=super::prepare_checked_chain(&assigned,hop(&first)).await.unwrap();
    assert_eq!(prepared.route,Route::SameExitVerified);
    assert_eq!(prepared.effective.username,SECOND_USER);assert_eq!(prepared.effective.password,SECOND_PASS);
    assert_eq!(serde_json::to_value(&assigned).unwrap(),before);
    let calls=first.counts.accepted.load(Ordering::SeqCst);
    assert_eq!(crate::space::proxy::geo_check_on_route(Some(&prepared.effective),None).await.unwrap().ip,"198.51.100.91");
    assert_eq!(first.counts.accepted.load(Ordering::SeqCst),calls,"retained traffic must not use the first hop");
    shutdown(prepared).await;wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn different_fresh_exits_keep_the_verified_chain_without_an_assigned_direct_probe() {
    let _root=crate::store::test_root("different-exit-kept-chain");preferred_geo();
    let second=service(Leaf::second("198.51.100.92"),None,false).await;
    let first=service(Leaf::first("198.51.100.93"),Some((second.addr,Chain::Relay(second.addr))),false).await;
    let prepared=super::prepare_checked_chain(&entry(second.addr),hop(&first)).await.unwrap();
    assert_eq!(prepared.route,Route::FirstHop);
    assert_eq!(second.counts.accepted.load(Ordering::SeqCst),first.counts.chained.load(Ordering::SeqCst),"different exits must not cause a direct assigned connection");
    shutdown(prepared).await;wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn failed_assigned_single_optimisation_does_not_discard_a_working_chain() {
    let _root=crate::store::test_root("same-exit-direct-unavailable");preferred_geo();
    let second=service(Leaf::second("198.51.100.94"),None,true).await;
    let first=service(Leaf::first("198.51.100.94"),Some((second.addr,Chain::Inline(Leaf::second("198.51.100.94")))),false).await;
    let prepared=super::prepare_checked_chain(&entry(second.addr),hop(&first)).await.unwrap();
    assert_eq!(prepared.route,Route::FirstHop);assert!(second.counts.accepted.load(Ordering::SeqCst)>0);
    assert_eq!(crate::space::proxy::geo_check_on_route(Some(&prepared.effective),None).await.unwrap().ip,"198.51.100.94");
    shutdown(prepared).await;wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn explicit_chain_authentication_refusal_does_not_run_same_exit_optimisation() {
    let _root=crate::store::test_root("same-exit-auth-refused");preferred_geo();
    let mut rejected=Leaf::second("198.51.100.95");rejected.reject_auth=true;
    let second=service(rejected,None,false).await;
    let first=service(Leaf::first("198.51.100.95"),Some((second.addr,Chain::Relay(second.addr))),false).await;
    let error=super::prepare_checked_chain(&entry(second.addr),hop(&first)).await.err().unwrap();
    assert_eq!(error.downcast_ref::<super::ChainError>().unwrap().code,"proxy_chain_profile_auth_failed");
    assert_eq!(first.counts.geos.load(Ordering::SeqCst),0,"must not query another route after explicit auth refusal");
    assert_eq!(second.counts.accepted.load(Ordering::SeqCst),1);
    wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn unknown_first_exit_cannot_be_replaced_with_a_cached_or_chain_exit() {
    let _root=crate::store::test_root("same-exit-first-unknown");preferred_geo();
    let second=service(Leaf::second("198.51.100.96"),None,false).await;
    let mut unknown=Leaf::first("198.51.100.96");unknown.valid_geo=false;
    let first=service(unknown,Some((second.addr,Chain::Relay(second.addr))),false).await;
    let prepared=super::prepare_checked_chain_with_budget(&entry(second.addr),hop(&first),Duration::from_millis(300)).await.unwrap();
    assert_eq!(prepared.route,Route::FirstHop);
    assert_eq!(second.counts.accepted.load(Ordering::SeqCst),first.counts.chained.load(Ordering::SeqCst));
    shutdown(prepared).await;wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn same_exit_recovery_requires_two_fresh_single_routes_after_chain_transport_failure() {
    let _root=crate::store::test_root("same-exit-recovery");preferred_geo();
    let second=service(Leaf::second("198.51.100.97"),None,false).await;
    let first=service(Leaf::first("198.51.100.97"),Some((second.addr,Chain::Refuse)),false).await;
    let prepared=super::prepare_checked_chain(&entry(second.addr),hop(&first)).await.unwrap();
    assert_eq!(prepared.route,Route::SameExitVerified);assert!(first.counts.chained.load(Ordering::SeqCst)>0);
    assert!(first.counts.geos.load(Ordering::SeqCst)>0);assert!(second.counts.geos.load(Ordering::SeqCst)>0);
    shutdown(prepared).await;wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn unequal_exit_recovery_keeps_the_original_chain_failure() {
    let _root=crate::store::test_root("same-exit-refused-recovery");preferred_geo();
    let second=service(Leaf::second("198.51.100.98"),None,false).await;
    let first=service(Leaf::first("198.51.100.99"),Some((second.addr,Chain::Refuse)),false).await;
    let error=super::prepare_checked_chain(&entry(second.addr),hop(&first)).await.err().unwrap();
    assert_eq!(error.downcast_ref::<super::ChainError>().unwrap().code,"system_proxy_connect_refused");
    wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn comparison_timeout_closes_its_pending_clients_and_keeps_the_verified_chain() {
    let _root=crate::store::test_root("same-exit-comparison-timeout");preferred_geo();
    let second=service(Leaf::second("198.51.100.100"),None,false).await;
    let mut hanging=Leaf::first("198.51.100.100");hanging.hang_geo=true;
    let first=service(hanging,Some((second.addr,Chain::Relay(second.addr))),false).await;
    let prepared=super::prepare_checked_chain_with_budget(&entry(second.addr),hop(&first),Duration::from_millis(100)).await.unwrap();
    assert_eq!(prepared.route,Route::FirstHop);wait_no_clients(&first).await;
    assert_eq!(crate::space::proxy::geo_check_on_route(Some(&prepared.effective),None).await.unwrap().ip,"198.51.100.100");
    shutdown(prepared).await;wait_no_clients(&first).await;wait_no_clients(&second).await;
}

#[tokio::test]
async fn each_start_uses_its_supplied_frozen_hop_and_never_reloads_a_new_system_proxy_for_comparison() {
    let _root=crate::store::test_root("same-exit-frozen-hop");preferred_geo();
    let second=service(Leaf::second("198.51.100.101"),None,false).await;
    let first=service(Leaf::first("198.51.100.101"),Some((second.addr,Chain::Relay(second.addr))),false).await;
    let changed=service(Leaf::first("198.51.100.102"),Some((second.addr,Chain::Relay(second.addr))),false).await;
    let assigned=entry(second.addr);let original=serde_json::to_value(&assigned).unwrap();
    let a=super::prepare_checked_chain(&assigned,hop(&first)).await.unwrap();assert_eq!(a.route,Route::SameExitVerified);
    let b=super::prepare_checked_chain(&assigned,hop(&changed)).await.unwrap();assert_eq!(b.route,Route::FirstHop);
    let before=changed.counts.accepted.load(Ordering::SeqCst);
    assert_eq!(crate::space::proxy::geo_check_on_route(Some(&a.effective),None).await.unwrap().ip,"198.51.100.101");
    assert_eq!(changed.counts.accepted.load(Ordering::SeqCst),before);
    assert_eq!(serde_json::to_value(&assigned).unwrap(),original);
    shutdown(a).await;shutdown(b).await;wait_no_clients(&first).await;wait_no_clients(&changed).await;wait_no_clients(&second).await;
}
