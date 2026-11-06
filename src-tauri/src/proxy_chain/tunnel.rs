//! A per-profile transparent TCP entry. It never accepts a destination from
//! the browser: every connection goes through the first hop to one target.

use super::connector::{self, ProxyHop, Target};
use super::error::ChainError;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::Instant;
use std::{future::Future,pin::Pin};

pub(super) type SystemResolver = Arc<dyn Fn(Target)->Pin<Box<dyn Future<Output=Result<super::detect_windows::ResolvedProxy,ChainError>>+Send>>+Send+Sync>;

const MAX_CONNECTIONS: usize = 64;
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

pub struct Tunnel {
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    finished: watch::Receiver<bool>,
    worker: Option<JoinHandle<()>>,
    active: Arc<AtomicUsize>,
    error: Arc<Mutex<Option<ChainError>>>,
}

impl Tunnel {
    pub async fn bind(hop: ProxyHop, target: Target) -> Result<Self, ChainError> {
        Self::bind_with_limits(
            hop,
            target,
            connector::CONNECT_TIMEOUT,
            IDLE_TIMEOUT,
            MAX_CONNECTIONS,
        )
        .await
    }

    pub(super) async fn bind_with_limits(
        hop: ProxyHop,
        target: Target,
        connect_timeout: Duration,
        idle_timeout: Duration,
        max_connections: usize,
    ) -> Result<Self, ChainError> {
        Self::bind_transport(Some(hop), target, connect_timeout, idle_timeout, max_connections, None, None).await
    }

    pub(crate) async fn bind_tls(hop: ProxyHop, target: Target, tls: Arc<rustls::ClientConfig>) -> Result<Self, ChainError> {
        Self::bind_transport(Some(hop), target, connector::CONNECT_TIMEOUT, IDLE_TIMEOUT, MAX_CONNECTIONS, Some(tls), None).await
    }

    pub(crate) async fn bind_assigned(target: Target, tls: Option<Arc<rustls::ClientConfig>>) -> Result<Self, ChainError> {
        Self::bind_transport(None, target, connector::CONNECT_TIMEOUT, IDLE_TIMEOUT, MAX_CONNECTIONS, tls, None).await
    }

    /// A local SOCKS5 entry resolves the Windows proxy for each browser CONNECT
    /// destination. PAC is never frozen using a different probe destination.
    pub(crate) async fn bind_system() -> Result<Self, ChainError> {
        let resolver:SystemResolver=Arc::new(|target|Box::pin(async move {super::detect_windows::resolve_for(&target).await}));
        Self::bind_system_with_resolver(resolver).await
    }

    pub(super) async fn bind_system_with_resolver(resolver:SystemResolver) -> Result<Self,ChainError> {
        Self::bind_transport(None,Target::new("example.com",443)?,connector::CONNECT_TIMEOUT,IDLE_TIMEOUT,MAX_CONNECTIONS,None,Some(resolver)).await
    }

    async fn bind_transport(hop: Option<ProxyHop>, target: Target, connect_timeout: Duration, idle_timeout: Duration, max_connections: usize, tls: Option<Arc<rustls::ClientConfig>>, system: Option<SystemResolver>) -> Result<Self, ChainError> {
        if let Some(hop) = &hop { connector::validate(hop, &target)?; }
        else { Target::new(&target.host, target.port)?; }
        if max_connections == 0 || connect_timeout.is_zero() || idle_timeout.is_zero() {
            return Err(ChainError::new(
                "proxy_chain_invalid_config",
                "configuration",
                "Tunnel limits must be greater than zero.",
            ));
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| {
                ChainError::new(
                    "proxy_chain_listen_failed",
                    "tunnel",
                    "Could not open the local proxy-chain listener.",
                )
            })?;
        let addr = listener.local_addr().map_err(|_| {
            ChainError::new(
                "proxy_chain_listen_failed",
                "tunnel",
                "Could not read the local proxy-chain listener address.",
            )
        })?;
        let (stop, mut stopping) = watch::channel(false);
        let (finished_tx, finished) = watch::channel(false);
        let active = Arc::new(AtomicUsize::new(0));
        let error = Arc::new(Mutex::new(None));
        let worker = {
            let active = active.clone();
            let error = error.clone();
            tokio::spawn(async move {
                // Declared before the resources so an abort drops the
                // listener/JoinSet before the completion notification.
                let _finished = Finished(finished_tx);
                let listener = listener;
                let quota = Arc::new(Semaphore::new(max_connections));
                let mut flows = JoinSet::new();
                loop {
                    tokio::select! {
                        biased;
                        _ = stopping.changed() => break,
                        _ = flows.join_next(), if !flows.is_empty() => {},
                        accepted = listener.accept() => {
                            let Ok((local, _)) = accepted else {
                                *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(ChainError::new(
                                    "proxy_chain_accept_failed", "tunnel", "The local proxy-chain listener stopped accepting connections."
                                ));
                                break;
                            };
                            // Reserve before spawning: pending handshakes count too,
                            // and an overloaded listener never creates unbounded tasks.
                            let Ok(permit) = quota.clone().try_acquire_owned() else { continue };
                            let slot = FlowSlot::new(permit, active.clone());
                            let hop = hop.clone();
                            let target = target.clone();
                            let error = error.clone();
                            let tls = tls.clone();
                            let system = system.clone();
                            flows.spawn(async move {
                                let _slot = slot;
                                let result = if let Some(resolver) = system {
                                    match system_connect(local, connect_timeout,resolver,error.clone()).await {
                                        Ok((local, remote)) => relay(local, remote, idle_timeout).await,
                                        Err(_) => return, // system_connect publishes before closing local.
                                    }
                                } else {
                                    // Keep local outside the dialing future: its EOF must
                                    // never reach a probe before the typed route error.
                                    let remote: Result<connector::Stream, ChainError> = async {
                                        if let Some(tls) = tls {
                                            if let Some(hop) = &hop { Ok(Box::new(super::tls::connect(hop, &target, tls, connect_timeout).await?) as connector::Stream) }
                                            else { Ok(Box::new(super::tls::connect_assigned(&target, tls, connect_timeout).await?) as connector::Stream) }
                                        } else if let Some(hop) = &hop {
                                            connector::connect_with_timeout(hop, &target, connect_timeout).await
                                        } else {
                                            Ok(Box::new(connector::connect_assigned(&target, connect_timeout).await?) as connector::Stream)
                                        }
                                    }.await;
                                    match remote {
                                        Ok(remote) => relay(local, remote, idle_timeout).await,
                                        Err(e) => {
                                            *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                                            drop(local);
                                            return;
                                        }
                                    }
                                };
                                if let Err(e) = result {
                                    *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                                }
                            });
                        },
                    }
                }
                // Drop the listener before waiting for cancelled flows, so no
                // new connection can enter while shutdown is being completed.
                drop(listener);
                flows.abort_all();
                while flows.join_next().await.is_some() {}
            })
        };
        Ok(Self {
            addr,
            stop,
            finished,
            worker: Some(worker),
            active,
            error,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn active_connections(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }

    pub fn last_error(&self) -> Option<ChainError> {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    #[cfg(test)]
    pub(super) fn closed_signal(&self) -> watch::Receiver<bool> {
        self.finished.clone()
    }

    /// Request shutdown without waiting. Safe in a synchronous Drop path.
    pub fn close(&self) {
        self.stop.send_replace(true);
    }

    /// Wait until both the listener and all flows have been released. The
    /// tracker uses this before publishing the profile as stopped; merely
    /// sending a cancellation would leave a short window with a live port.
    pub async fn close_and_wait(&self) -> Result<(), ChainError> {
        self.close();
        if let Ok(result) = tokio::time::timeout(Duration::from_secs(2), self.wait_finished()).await
        {
            return result;
        }
        if let Some(worker) = &self.worker {
            worker.abort();
        }
        tokio::time::timeout(Duration::from_secs(2), self.wait_finished())
            .await
            .map_err(|_| cleanup_timeout())?
    }

    async fn wait_finished(&self) -> Result<(), ChainError> {
        let mut finished = self.finished.clone();
        loop {
            if *finished.borrow_and_update() {
                if self.active_connections() == 0 {
                    return Ok(());
                }
                // Aborting the owner drops its JoinSet; its flow cancellations
                // still have to run and return their RAII reservations.
                tokio::time::sleep(Duration::from_millis(5)).await;
            } else if finished.changed().await.is_err() {
                return Err(cleanup_timeout());
            }
        }
    }

    /// Ends listening and all established or pending connections before returning.
    pub async fn shutdown(mut self) {
        let _ = self.close_and_wait().await;
        if let Some(mut worker) = self.worker.take() {
            if tokio::time::timeout(Duration::from_secs(2), &mut worker)
                .await
                .is_err()
            {
                worker.abort();
                let _ = worker.await;
            }
        }
    }
}

pub(super) async fn system_connect(mut local: TcpStream, budget: Duration,resolver:SystemResolver,last_error:Arc<Mutex<Option<ChainError>>>) -> Result<(TcpStream, connector::Stream), ChainError> {
    let work = async {
        let failed = || ChainError::new("proxy_chain_local_protocol_failed", "tunnel", "The local system-route SOCKS5 request was invalid or unsupported.");
        if local.read_u8().await.map_err(|_| failed())? != 5 { return Err(failed()); }
        let count = local.read_u8().await.map_err(|_| failed())? as usize;
        if count == 0 { return Err(failed()); }
        let mut methods=vec![0;count]; local.read_exact(&mut methods).await.map_err(|_| failed())?;
        if !methods.contains(&0) { let _=local.write_all(&[5,255]).await; return Err(failed()); }
        local.write_all(&[5,0]).await.map_err(|_| failed())?;
        let mut head=[0;4]; local.read_exact(&mut head).await.map_err(|_| failed())?;
        if head[0] != 5 || head[2] != 0 { return Err(failed()); }
        let host=match head[3] {
            1 => { let mut v=[0;4]; local.read_exact(&mut v).await.map_err(|_| failed())?; std::net::Ipv4Addr::from(v).to_string() },
            4 => { let mut v=[0;16]; local.read_exact(&mut v).await.map_err(|_| failed())?; std::net::Ipv6Addr::from(v).to_string() },
            3 => { let size=local.read_u8().await.map_err(|_| failed())? as usize; if size==0{return Err(failed());} let mut v=vec![0;size];local.read_exact(&mut v).await.map_err(|_| failed())?;String::from_utf8(v).map_err(|_|failed())? },
            _ => return Err(failed()),
        };
        let port=local.read_u16().await.map_err(|_|failed())?;
        if head[1] != 1 {let _=local.write_all(&[5,7,0,1,0,0,0,0,0,0]).await;return Err(failed());}
        let target=Target::new(host,port)?;
        let remote=async { let resolved=resolver(target.clone()).await?; connector::connect_with_timeout(&resolved.hop,&target,budget).await }.await;
        match remote {
            Ok(remote) => { local.write_all(&[5,0,0,1,0,0,0,0,0,0]).await.map_err(|_|failed())?; Ok(remote) },
            Err(error) => { *last_error.lock().unwrap_or_else(|e|e.into_inner())=Some(error.clone());let _=local.write_all(&[5,1,0,1,0,0,0,0,0,0]).await; Err(error) },
        }
    };
    let result = tokio::time::timeout(budget,work).await
        .map_err(|_|ChainError::new("system_proxy_timeout","system_proxy","The system proxy did not finish resolving or connecting in time."))
        .and_then(|result| result);
    match result {
        Ok(remote) => Ok((local, remote)),
        Err(error) => {
            *last_error.lock().unwrap_or_else(|e|e.into_inner())=Some(error.clone());
            Err(error)
        }
    }
}

fn cleanup_timeout() -> ChainError {
    ChainError::new(
        "proxy_chain_cleanup_timeout",
        "tunnel",
        "The proxy-chain tunnel has not confirmed that all local connections are closed.",
    )
}

struct Finished(watch::Sender<bool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.close();
        // Dropping a JoinHandle detaches its task. Keep cancellation explicit:
        // aborting the owner also drops its JoinSet and aborts every flow.
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

struct FlowSlot {
    _permit: OwnedSemaphorePermit,
    active: Arc<AtomicUsize>,
}

impl FlowSlot {
    fn new(permit: OwnedSemaphorePermit, active: Arc<AtomicUsize>) -> Self {
        active.fetch_add(1, Ordering::AcqRel);
        Self {
            _permit: permit,
            active,
        }
    }
}

impl Drop for FlowSlot {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

async fn relay<S: AsyncRead + AsyncWrite + Unpin>(local: TcpStream, remote: S, idle: Duration) -> Result<(), ChainError> {
    let last = Arc::new(Mutex::new(Instant::now()));
    let (lr, lw) = local.into_split();
    let (rr, rw) = tokio::io::split(remote);
    let transfer = async {
        tokio::try_join!(pump(lr, rw, last.clone()), pump(rr, lw, last.clone()))?;
        Ok(())
    };
    tokio::select! {
        result = transfer => result,
        _ = idle_expired(last.clone(), idle) => Err(ChainError::new("proxy_chain_idle_timeout", "tunnel", "The proxy-chain connection closed after being idle.")),
    }
}

async fn pump<R, W>(
    mut reader: R,
    mut writer: W,
    last: Arc<Mutex<Instant>>,
) -> Result<(), ChainError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let failed = || {
        ChainError::new(
            "proxy_chain_stream_closed",
            "tunnel",
            "The proxy-chain connection was interrupted.",
        )
    };
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = reader.read(&mut buf).await.map_err(|_| failed())?;
        if n == 0 {
            writer.shutdown().await.map_err(|_| failed())?;
            return Ok(());
        }
        *last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
        writer.write_all(&buf[..n]).await.map_err(|_| failed())?;
        *last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }
}

async fn idle_expired(last: Arc<Mutex<Instant>>, idle: Duration) {
    loop {
        let until = *last.lock().unwrap_or_else(|e| e.into_inner()) + idle;
        tokio::time::sleep_until(until).await;
        if last.lock().unwrap_or_else(|e| e.into_inner()).elapsed() >= idle {
            return;
        }
    }
}
