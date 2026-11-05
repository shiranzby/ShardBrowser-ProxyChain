//! TLS to the original HTTPS proxy, inside the first-hop CONNECT stream.
//! The loopback endpoint speaks HTTP; authentication stays inside verified TLS.
use super::{
    connector::{self, ProxyHop, Target},
    error::ChainError,
};
use std::{sync::Arc, time::Duration};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, TlsConnector};

pub(crate) fn client() -> Result<Arc<rustls::ClientConfig>, ChainError> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| failed())?
    .with_root_certificates(roots)
    .with_no_client_auth();
    // Browser/reqwest sends HTTP/1 CONNECT over our local HTTP endpoint.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

fn failed() -> ChainError {
    ChainError::new("proxy_chain_profile_tls_failed", "profile", "TLS to the HTTPS profile proxy failed. Check that this port supports TLS and its certificate matches the original proxy host. No HTTP downgrade or certificate bypass was used.")
}

pub(crate) async fn connect(
    hop: &ProxyHop,
    target: &Target,
    config: Arc<rustls::ClientConfig>,
    deadline: Duration,
) -> Result<TlsStream<TcpStream>, ChainError> {
    let name =
        rustls::pki_types::ServerName::try_from(target.host.clone()).map_err(|_| failed())?;
    tokio::time::timeout(deadline, async {
        let raw = connector::connect_with_timeout(hop, target, deadline).await?;
        TlsConnector::from(config).connect(name, raw).await.map_err(|_| failed())
    }).await.map_err(|_| ChainError::new("proxy_chain_profile_tls_timeout", "profile", "TLS to the HTTPS profile proxy timed out. The connection was closed without a direct fallback."))?
}
