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
        Self::bind_transport(hop, target, connect_timeout, idle_timeout, max_connections, None).await
    }

    pub(crate) async fn bind_tls(hop: ProxyHop, target: Target, tls: Arc<rustls::ClientConfig>) -> Result<Self, ChainError> {
        Self::bind_transport(hop, target, connector::CONNECT_TIMEOUT, IDLE_TIMEOUT, MAX_CONNECTIONS, Some(tls)).await
    }

    async fn bind_transport(hop: ProxyHop, target: Target, connect_timeout: Duration, idle_timeout: Duration, max_connections: usize, tls: Option<Arc<rustls::ClientConfig>>) -> Result<Self, ChainError> {
        connector::validate(&hop, &target)?;
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
                            flows.spawn(async move {
                                let _slot = slot;
                                let result = async {
                                    if let Some(tls) = tls {
                                        let remote = super::tls::connect(&hop, &target, tls, connect_timeout).await?;
                                        relay(local, remote, idle_timeout).await
                                    } else {
                                        let remote = connector::connect_with_timeout(&hop, &target, connect_timeout).await?;
                                        relay(local, remote, idle_timeout).await
                                    }
                                }.await;
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
