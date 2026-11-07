//! A bounded HTTPS response on the prepared proxy is independent of Geo.
//! Every socket uses that proxy; target names are never resolved directly.
use super::{connector::{self, ProxyHop, Target}, error::ChainError, tls, tunnel::Tunnel};
use crate::space::proxy::{ProxyEntry, ProxyKind};
use http_body_util::{BodyExt, Empty};
use hyper::{body::Bytes, Request, StatusCode};
use hyper_util::rt::TokioIo;
use std::{sync::Arc, time::{Duration, Instant}};

const TOTAL_BUDGET:Duration=Duration::from_secs(6);
const TARGET_BUDGET:Duration=Duration::from_secs(3);
const MAX_BODY:usize=64*1024;
const TARGETS:[&str;2]=["https://example.com/","https://example.org/"];

fn unavailable()->ChainError {
    ChainError::new("proxy_chain_profile_geo_failed","exit","The exit lookup failed and this same proxy route did not return a verified HTTPS response.")
}

struct ConnectionTask(tokio::task::JoinHandle<()>);
impl Drop for ConnectionTask {fn drop(&mut self){self.0.abort();}}

pub(super) async fn verify(entry:&ProxyEntry,tunnel:&Tunnel)->Result<u128,ChainError> {
    let start=Instant::now();
    let (targets,config)=configuration()?;
    let hop=ProxyHop{kind:match entry.kind {ProxyKind::Http=>connector::ProxyKind::Http,ProxyKind::Https=>connector::ProxyKind::Https,ProxyKind::Socks5=>connector::ProxyKind::Socks5},
        host:entry.host.clone(),port:entry.port,username:entry.username.clone(),password:entry.password.clone()};
    tokio::time::timeout(TOTAL_BUDGET,async {
        for url in targets {
            match tokio::time::timeout(TARGET_BUDGET,one(&hop,&url,config.clone())).await {
                Ok(Ok(()))=>return Ok(start.elapsed().as_millis()),
                Ok(Err(error)) if matches!(error.code,"proxy_chain_profile_auth_failed"|"system_proxy_auth_failed")=>return Err(error),
                _=>{
                    // A system relay may give its local client a generic
                    // failure after publishing the actual first-hop refusal.
                    // Never hide or retry that authentication failure.
                    if let Some(error)=authentication_failure(tunnel){return Err(error);}
                },
            }
        }
        Err(unavailable())
    }).await.map_err(|_|authentication_failure(tunnel).unwrap_or_else(unavailable))?
}

fn authentication_failure(tunnel:&Tunnel)->Option<ChainError> {
    tunnel.last_error().filter(|error|matches!(error.code,"proxy_chain_profile_auth_failed"|"system_proxy_auth_failed"))
}

async fn one(hop:&ProxyHop,url:&str,config:Arc<rustls::ClientConfig>)->Result<(),ChainError> {
    let url=url::Url::parse(url).map_err(|_|unavailable())?;
    if url.scheme()!="https" || !url.username().is_empty() || url.password().is_some() {return Err(unavailable());}
    let host=url.host_str().ok_or_else(unavailable)?;
    let target=Target::new(host,url.port_or_known_default().ok_or_else(unavailable)?)?;
    let stream=connector::connect_with_timeout(hop,&target,TARGET_BUDGET).await.map_err(|mut error|{
        // This hop is the prepared local endpoint, whose authentication is the
        // original assigned proxy's. A parsed refusal remains authoritative.
        if error.code=="system_proxy_auth_failed" {error.code="proxy_chain_profile_auth_failed";error.layer="profile";}
        error
    })?;
    let name=rustls::pki_types::ServerName::try_from(host.to_owned()).map_err(|_|unavailable())?;
    let secure=tokio_rustls::TlsConnector::from(config).connect(name,stream).await.map_err(|_|unavailable())?;
    let (mut sender,connection)=hyper::client::conn::http1::Builder::new().max_buf_size(16*1024)
        .handshake::<_,Empty<Bytes>>(TokioIo::new(secure)).await.map_err(|_|unavailable())?;
    let _connection=ConnectionTask(tokio::spawn(async move {let _=connection.await;}));
    let mut path=url.path().to_owned();
    if path.is_empty(){path.push('/');}
    if let Some(query)=url.query(){path.push('?');path.push_str(query);}
    let authority=if let Some(port)=url.port(){format!("{host}:{port}")}else{host.to_owned()};
    let request=Request::builder().method("GET").uri(path).header("Host",authority)
        .header("Connection","close").body(Empty::<Bytes>::new()).map_err(|_|unavailable())?;
    let response=sender.send_request(request).await.map_err(|_|unavailable())?;
    if response.status()!=StatusCode::OK {return Err(unavailable());}
    if response.headers().get(hyper::header::CONTENT_LENGTH).and_then(|value|value.to_str().ok()).and_then(|value|value.parse::<u64>().ok()).is_some_and(|size|size>MAX_BODY as u64) {return Err(unavailable());}
    let mut body=response.into_body();let mut bytes=0usize;
    while let Some(frame)=body.frame().await {
        let frame=frame.map_err(|_|unavailable())?;
        if let Some(data)=frame.data_ref() {bytes=bytes.checked_add(data.len()).ok_or_else(unavailable)?;if bytes>MAX_BODY{return Err(unavailable());}}
    }
    if bytes==0 {return Err(unavailable());}
    Ok(())
}

fn configuration()->Result<(Vec<String>,Arc<rustls::ClientConfig>),ChainError> {
    #[cfg(test)]
    if let Some(value)=TEST_CONFIGURATION.with(|value|value.borrow().clone()){return Ok(value);}
    Ok((TARGETS.iter().map(|target|(*target).to_owned()).collect(),tls::client()?))
}

#[cfg(test)]
thread_local! {static TEST_CONFIGURATION:std::cell::RefCell<Option<(Vec<String>,Arc<rustls::ClientConfig>)>>=const{std::cell::RefCell::new(None)};}

#[cfg(test)]
pub(super) struct TestConfiguration(Option<(Vec<String>,Arc<rustls::ClientConfig>)>);
#[cfg(test)]
impl Drop for TestConfiguration {fn drop(&mut self){TEST_CONFIGURATION.with(|value|*value.borrow_mut()=self.0.take());}}
#[cfg(test)]
pub(super) fn test_configuration(targets:Vec<String>,config:Arc<rustls::ClientConfig>)->TestConfiguration {
    TestConfiguration(TEST_CONFIGURATION.with(|value|value.replace(Some((targets,config)))))
}
