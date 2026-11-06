//! Connect to one fixed destination through the system proxy. There is no
//! direct fallback, and only the system proxy's own name is resolved here.

use super::error::ChainError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::fmt;
use std::net::IpAddr;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_HTTP_HEAD: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    Http,
    Https,
    Socks5,
}

pub trait ProxyStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ProxyStream for T {}
pub type Stream = Box<dyn ProxyStream>;
impl fmt::Debug for dyn ProxyStream {
    fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str("ProxyStream [redacted]")}
}

#[derive(Clone)]
pub struct ProxyHop {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl fmt::Debug for ProxyHop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyHop")
            .field("kind", &self.kind)
            .field("port", &self.port)
            .field("credentials", &"[redacted]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
}

impl Target {
    pub fn new(host: impl Into<String>, port: u16) -> Result<Self, ChainError> {
        Ok(Self {
            host: normal_host(&host.into())?,
            port: valid_port(port)?,
        })
    }

    fn authority(&self) -> String {
        if self.host.parse::<std::net::Ipv6Addr>().is_ok() {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

fn bad_config(message: &'static str) -> ChainError {
    ChainError::new("proxy_chain_invalid_config", "configuration", message)
}

fn valid_port(port: u16) -> Result<u16, ChainError> {
    if port == 0 {
        Err(bad_config("A proxy port must be between 1 and 65535."))
    } else {
        Ok(port)
    }
}

/// Accept an IP or an IDNA hostname, not a URL, authority or command fragment.
fn normal_host(host: &str) -> Result<String, ChainError> {
    if host.is_empty() || host.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(bad_config(
            "A proxy host must be an IP address or hostname.",
        ));
    }
    let ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = ip.parse::<IpAddr>() {
        return Ok(ip.to_string());
    }
    if host.chars().any(|c| ":/\\@?#%;[]=,".contains(c)) {
        return Err(bad_config(
            "Enter the proxy host separately from its port and credentials.",
        ));
    }
    match url::Host::parse(host) {
        Ok(url::Host::Domain(name)) if !name.is_empty() && name.len() <= 253 => Ok(name),
        _ => Err(bad_config(
            "A proxy host must be an IP address or hostname.",
        )),
    }
}

pub fn validate(hop: &ProxyHop, target: &Target) -> Result<(), ChainError> {
    normal_host(&hop.host)?;
    normal_host(&target.host)?;
    valid_port(hop.port)?;
    valid_port(target.port)?;
    if hop
        .username
        .chars()
        .chain(hop.password.chars())
        .any(char::is_control)
    {
        return Err(bad_config(
            "Proxy credentials cannot contain control characters.",
        ));
    }
    if hop.username.len().saturating_add(hop.password.len()) > 4096 {
        return Err(bad_config(
            "Proxy credentials cannot exceed 4096 bytes in total.",
        ));
    }
    if matches!(hop.kind, ProxyKind::Http | ProxyKind::Https) && hop.username.contains(':') {
        return Err(bad_config("An HTTP Basic username cannot contain a colon."));
    }
    if hop.kind == ProxyKind::Socks5 {
        let credentials = !hop.username.is_empty() || !hop.password.is_empty();
        if credentials
            && (hop.username.is_empty() || hop.username.len() > 255 || hop.password.len() > 255)
        {
            return Err(bad_config(
                "SOCKS5 credentials require a username and at most 255 bytes per field.",
            ));
        }
    }
    Ok(())
}

fn io_error() -> ChainError {
    ChainError::new(
        "system_proxy_io_failed",
        "system_proxy",
        "The system proxy closed or interrupted the connection.",
    )
}

fn protocol_error(message: &'static str) -> ChainError {
    ChainError::new("system_proxy_protocol_error", "system_proxy", message)
}

pub async fn connect(hop: &ProxyHop, target: &Target) -> Result<Stream, ChainError> {
    connect_with_timeout(hop, target, CONNECT_TIMEOUT).await
}

/// A protected route may explicitly skip the first hop, but it still dials
/// only the original assigned proxy. Never pass a browser destination here.
pub(crate) async fn connect_assigned(target: &Target, deadline: Duration) -> Result<TcpStream, ChainError> {
    tokio::time::timeout(deadline, TcpStream::connect((target.host.as_str(), target.port)))
        .await
        .map_err(|_| ChainError::new("proxy_chain_assigned_proxy_timeout", "profile", "Connecting to the assigned proxy without the first hop timed out. No website direct connection was attempted."))?
        .map_err(|_| ChainError::new("proxy_chain_assigned_proxy_failed", "profile", "Could not connect to the assigned proxy without the first hop. No website direct connection was attempted."))
}

pub(super) async fn connect_with_timeout(
    hop: &ProxyHop,
    target: &Target,
    budget: Duration,
) -> Result<Stream, ChainError> {
    connect_with_tls_config(hop,target,budget,None).await
}

pub(super) async fn connect_with_tls_config(hop:&ProxyHop,target:&Target,budget:Duration,tls:Option<std::sync::Arc<rustls::ClientConfig>>) -> Result<Stream,ChainError> {
    validate(hop, target)?;
    let host = normal_host(&hop.host)?;
    let target = Target::new(target.host.clone(), target.port)?;
    tokio::time::timeout(budget, async {
        let stream = TcpStream::connect((host.as_str(), hop.port))
            .await
            .map_err(|_| {
                ChainError::new(
                    "system_proxy_connect_failed",
                    "system_proxy",
                    "Could not connect to the system proxy.",
                )
            })?;
        let _ = stream.set_nodelay(true);
        let mut stream: Stream = if hop.kind == ProxyKind::Https {
            let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|_| first_tls_error())?;
            Box::new(tokio_rustls::TlsConnector::from(match tls {Some(tls)=>tls,None=>super::tls::client()?}).connect(name, stream).await.map_err(|_| first_tls_error())?)
        } else { Box::new(stream) };
        match hop.kind {
            ProxyKind::Http | ProxyKind::Https => http_connect(&mut stream, hop, &target).await?,
            ProxyKind::Socks5 => socks_connect(&mut stream, hop, &target).await?,
        }
        Ok(stream)
    })
    .await
    .map_err(|_| {
        ChainError::new(
            "system_proxy_timeout",
            "system_proxy",
            "The system proxy did not finish connecting in time.",
        )
    })?
}

fn first_tls_error() -> ChainError {
    ChainError::new("system_proxy_tls_failed", "system_proxy", "TLS to the first-hop HTTPS proxy failed. Its certificate must match the proxy host; no HTTP downgrade or certificate bypass was used.")
}

async fn http_connect(
    stream: &mut Stream,
    hop: &ProxyHop,
    target: &Target,
) -> Result<(), ChainError> {
    let authority = target.authority();
    let mut request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
    if !hop.username.is_empty() || !hop.password.is_empty() {
        let encoded = STANDARD.encode(format!("{}:{}", hop.username, hop.password));
        request.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| io_error())?;

    // Read exactly the header: CONNECT's reply may share a TCP packet with
    // the second proxy's greeting. Returning a raw TcpStream must lose none
    // of those bytes, so a buffered read that overreads is not sufficient.
    let mut head = Vec::with_capacity(512);
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await.map_err(|_| io_error())?;
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
        if head.len() >= MAX_HTTP_HEAD {
            return Err(protocol_error(
                "The system proxy returned an oversized HTTP header.",
            ));
        }
    }
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut response = httparse::Response::new(&mut headers);
    if !matches!(response.parse(&head), Ok(httparse::Status::Complete(_)))
        || !matches!(response.version, Some(0 | 1))
    {
        return Err(protocol_error(
            "The system proxy returned an invalid HTTP CONNECT response.",
        ));
    }
    match response.code {
        Some(200) => Ok(()),
        Some(407) => Err(ChainError::new(
            "system_proxy_auth_failed",
            "system_proxy",
            "The system proxy rejected or requires authentication.",
        )),
        Some(status) => Err(ChainError::new(
            "system_proxy_connect_refused",
            "system_proxy",
            format!("The system proxy refused CONNECT (HTTP {status})."),
        )),
        None => Err(protocol_error(
            "The system proxy returned no HTTP CONNECT status.",
        )),
    }
}

async fn socks_connect(
    stream: &mut Stream,
    hop: &ProxyHop,
    target: &Target,
) -> Result<(), ChainError> {
    let credentials = !hop.username.is_empty() || !hop.password.is_empty();
    let greeting: &[u8] = if credentials {
        &[5, 2, 0, 2]
    } else {
        &[5, 1, 0]
    };
    stream.write_all(greeting).await.map_err(|_| io_error())?;
    let mut answer = [0u8; 2];
    stream
        .read_exact(&mut answer)
        .await
        .map_err(|_| io_error())?;
    if answer[0] != 5 {
        return Err(protocol_error("The system proxy did not answer as SOCKS5."));
    }
    match answer[1] {
        0 => {}
        2 if credentials => {
            let mut auth = Vec::with_capacity(hop.username.len() + hop.password.len() + 3);
            auth.extend_from_slice(&[1, hop.username.len() as u8]);
            auth.extend_from_slice(hop.username.as_bytes());
            auth.push(hop.password.len() as u8);
            auth.extend_from_slice(hop.password.as_bytes());
            stream.write_all(&auth).await.map_err(|_| io_error())?;
            stream
                .read_exact(&mut answer)
                .await
                .map_err(|_| io_error())?;
            if answer[0] != 1 {
                return Err(protocol_error(
                    "The system proxy returned an invalid SOCKS5 authentication response.",
                ));
            }
            if answer[1] != 0 {
                return Err(ChainError::new(
                    "system_proxy_auth_failed",
                    "system_proxy",
                    "The system proxy rejected the username or password.",
                ));
            }
        }
        2 | 255 => {
            return Err(ChainError::new(
                "system_proxy_auth_failed",
                "system_proxy",
                "The system proxy requires valid SOCKS5 credentials.",
            ))
        }
        _ => {
            return Err(protocol_error(
                "The system proxy selected an unsupported SOCKS5 authentication method.",
            ))
        }
    }

    let mut request = vec![5, 1, 0];
    match target.host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            request.push(1);
            request.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            request.push(4);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            request.extend_from_slice(&[3, target.host.len() as u8]);
            request.extend_from_slice(target.host.as_bytes());
        }
    }
    request.extend_from_slice(&target.port.to_be_bytes());
    stream.write_all(&request).await.map_err(|_| io_error())?;

    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|_| io_error())?;
    if header[0] != 5 || header[2] != 0 {
        return Err(protocol_error(
            "The system proxy returned an invalid SOCKS5 CONNECT response.",
        ));
    }
    if header[1] != 0 {
        let reason = match header[1] {
            1 => "general failure",
            2 => "not allowed by its ruleset",
            3 => "network unreachable",
            4 => "host unreachable",
            5 => "connection refused",
            6 => "TTL expired",
            7 => "CONNECT not supported",
            8 => "address type not supported",
            _ => "unknown reply code",
        };
        return Err(ChainError::new(
            "system_proxy_connect_refused",
            "system_proxy",
            format!(
                "The system proxy refused SOCKS5 CONNECT: {reason} (code {}).",
                header[1]
            ),
        ));
    }
    let remaining = match header[3] {
        1 => 6,
        4 => 18,
        3 => {
            let n = stream.read_u8().await.map_err(|_| io_error())? as usize;
            if n == 0 {
                return Err(protocol_error(
                    "The system proxy returned an empty SOCKS5 bound address.",
                ));
            }
            n + 2
        }
        _ => {
            return Err(protocol_error(
                "The system proxy returned an unsupported SOCKS5 bound address.",
            ))
        }
    };
    let mut rest = vec![0u8; remaining];
    stream.read_exact(&mut rest).await.map_err(|_| io_error())?;
    Ok(())
}
