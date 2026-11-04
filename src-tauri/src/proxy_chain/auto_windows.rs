//! Cancellable Windows PAC/WPAD resolution for one fixed CONNECT destination.
//! The callback receives an integer request ID, never a pointer to Rust state.

use super::{
    authority, auto_error, ChainError, ProxyHop, ProxyKind, ResolvedProxy, SystemSettings, Target,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex, OnceLock,
};
use std::time::Duration;
use tokio::sync::{oneshot, Semaphore};
use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
use windows_sys::Win32::Networking::WinHttp::*;

const DEADLINE: Duration = Duration::from_secs(15);
static SLOTS: Semaphore = Semaphore::const_new(4);
static NEXT_REQUEST: AtomicUsize = AtomicUsize::new(1);
type Completion = oneshot::Sender<bool>;
static REQUESTS: OnceLock<Mutex<HashMap<usize, Completion>>> = OnceLock::new();

fn requests() -> &'static Mutex<HashMap<usize, Completion>> {
    REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct Registration(usize);
impl Registration {
    fn new() -> Result<(Self, oneshot::Receiver<bool>), ChainError> {
        let id = NEXT_REQUEST
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| failed())?;
        let (tx, rx) = oneshot::channel();
        requests()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        Ok((Self(id), rx))
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        requests()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

fn complete(id: usize, success: bool) {
    if let Some(tx) = requests()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id)
    {
        let _ = tx.send(success);
    }
}

unsafe extern "system" fn callback(
    _handle: *mut c_void,
    context: usize,
    status: u32,
    _information: *mut c_void,
    _length: u32,
) {
    match status {
        WINHTTP_CALLBACK_STATUS_GETPROXYFORURL_COMPLETE => complete(context, true),
        WINHTTP_CALLBACK_STATUS_REQUEST_ERROR => complete(context, false),
        _ => {}
    }
}

// WinHTTP handles are usable across threads. Owning their integer value keeps
// this future Send without holding any borrowed/native-memory pointer in Rust.
struct Handle(usize);
impl Handle {
    fn raw(&self) -> *mut c_void {
        self.0 as *mut c_void
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // Closing an asynchronous resolver cancels it. Late callbacks can only
        // look up the retired integer ID and cannot touch freed Rust memory.
        unsafe {
            WinHttpCloseHandle(self.raw());
        }
    }
}

fn failed() -> ChainError {
    auto_error("proxy_chain_auto_resolve_failed", "Windows could not resolve an automatic proxy for this destination. No direct connection was attempted.")
}
fn timed_out() -> ChainError {
    auto_error(
        "proxy_chain_auto_resolve_timeout",
        "Windows automatic proxy resolution timed out. No direct connection was attempted.",
    )
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn target_url(target: &Target) -> String {
    // A CONNECT tunnel is represented as an HTTPS authority for PAC matching;
    // it contains the actual second-hop host and port, never its credentials.
    format!("https://{}/", authority(target))
}

struct Candidate {
    proxy: bool,
    bypass: bool,
    scheme: i32,
    host: String,
    port: u16,
}

fn select(entries: Vec<Candidate>) -> Result<ProxyHop, ChainError> {
    // fBypass is meaningful only when fProxy is FALSE. An undefined bypass
    // bit on an explicit proxy must not reject a valid proxy result.
    if entries.iter().any(|entry| !entry.proxy && entry.bypass) {
        return Err(auto_error("proxy_chain_auto_direct_refused", "Windows automatic proxy configuration bypassed this destination. Proxy chaining will not connect directly."));
    }
    if entries.is_empty() || entries.iter().any(|entry| !entry.proxy) {
        return Err(auto_error("proxy_chain_auto_direct_refused", "Windows automatic proxy configuration returned DIRECT for this destination. Proxy chaining will not connect directly."));
    }
    let mut choices = Vec::new();
    for entry in entries {
        // WINHTTP_INTERNET_SCHEME_SOCKS does not distinguish SOCKS4 from SOCKS5.
        // Do not guess it, or mistake an HTTPS proxy transport for plain HTTP.
        if entry.scheme != WINHTTP_INTERNET_SCHEME_HTTP {
            return Err(auto_error("proxy_chain_auto_protocol_unsupported", "Windows automatic proxy configuration returned an unsupported or ambiguous proxy protocol. Use an explicit HTTP or SOCKS5 first hop."));
        }
        let target = Target::new(&entry.host, entry.port).map_err(|_| failed())?;
        if !choices.contains(&target) {
            choices.push(target);
        }
    }
    if choices.len() != 1 {
        return Err(auto_error("proxy_chain_auto_ambiguous", "Windows automatic proxy configuration returned multiple different proxies. Select an explicit first hop; no direct connection was attempted."));
    }
    let target = choices.remove(0);
    Ok(ProxyHop {
        kind: ProxyKind::Http,
        host: target.host,
        port: target.port,
        username: String::new(),
        password: String::new(),
    })
}

unsafe fn copy_text(pointer: *const u16) -> Result<String, ChainError> {
    if pointer.is_null() {
        return Err(failed());
    }
    let mut length = 0;
    while length < 32768 && unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    if length == 32768 {
        return Err(failed());
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) }).map_err(|_| failed())
}

fn result(resolver: &Handle) -> Result<ProxyHop, ChainError> {
    struct ResultGuard(WINHTTP_PROXY_RESULT);
    impl Drop for ResultGuard {
        fn drop(&mut self) {
            unsafe {
                WinHttpFreeProxyResult(&mut self.0);
            }
        }
    }
    let mut native = ResultGuard(unsafe { std::mem::zeroed() });
    if unsafe { WinHttpGetProxyResult(resolver.raw(), &mut native.0) } != 0 {
        return Err(failed());
    }
    if native.0.cEntries > 64 || (native.0.cEntries != 0 && native.0.pEntries.is_null()) {
        return Err(failed());
    }
    let mut entries = Vec::new();
    for n in 0..native.0.cEntries as usize {
        // The array and strings belong to ResultGuard until after they are copied.
        let entry = unsafe { &*native.0.pEntries.add(n) };
        entries.push(Candidate {
            proxy: entry.fProxy != 0,
            bypass: entry.fBypass != 0,
            scheme: entry.ProxyScheme,
            host: if entry.fProxy != 0 {
                unsafe { copy_text(entry.pwszProxy) }?
            } else {
                String::new()
            },
            port: entry.ProxyPort,
        });
    }
    select(entries)
}

struct Pending {
    // Fields drop in declaration order: retire callback ID, cancel resolver,
    // close session, and only then release input strings.
    _registration: Registration,
    resolver: Handle,
    _session: Handle,
    _options: NativeOptions,
    _pac_url: Vec<u16>,
    _destination: Vec<u16>,
}

struct NativeOptions(Box<WINHTTP_AUTOPROXY_OPTIONS>);
// SAFETY: initialized once before the native call, then never mutated or
// dereferenced by Rust. The only non-null pointer refers to Pending's owned
// PAC UTF-16 buffer. It remains allocated until after resolver cancellation;
// moving the future between executor threads moves no allocation/native data.
unsafe impl Send for NativeOptions {}

fn start(
    settings: &SystemSettings,
    target: &Target,
) -> Result<(Pending, oneshot::Receiver<bool>), ChainError> {
    let user_agent = wide("ShardBrowser proxy resolution");
    let session = Handle(unsafe {
        WinHttpOpen(
            user_agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            ptr::null(),
            ptr::null(),
            WINHTTP_FLAG_ASYNC,
        )
    } as usize);
    if session.0 == 0 {
        return Err(failed());
    }
    // Bound PAC fetch stages as well as the outer end-to-end future deadline.
    if unsafe { WinHttpSetTimeouts(session.raw(), 5000, 5000, 5000, 5000) } == 0 {
        return Err(failed());
    }
    let previous = unsafe {
        WinHttpSetStatusCallback(
            session.raw(),
            Some(callback),
            WINHTTP_CALLBACK_FLAG_REQUEST_ERROR | WINHTTP_CALLBACK_FLAG_GETPROXYFORURL_COMPLETE,
            0,
        )
    };
    if previous.is_some_and(|value| value as usize == usize::MAX) {
        return Err(failed());
    }
    let mut resolver_raw = ptr::null_mut();
    if unsafe { WinHttpCreateProxyResolver(session.raw(), &mut resolver_raw) } != 0 {
        return Err(failed());
    }
    let resolver = Handle(resolver_raw as usize);
    let (registration, completion) = Registration::new()?;

    if !settings.pac_url.is_empty() {
        let url = url::Url::parse(&settings.pac_url).map_err(|_| failed())?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(failed());
        }
    }
    let pac_url = wide(&settings.pac_url);
    let destination = wide(&target_url(target));
    let options = NativeOptions(Box::new(WINHTTP_AUTOPROXY_OPTIONS {
        dwFlags: if settings.pac_url.is_empty() {
            WINHTTP_AUTOPROXY_AUTO_DETECT
        } else {
            WINHTTP_AUTOPROXY_CONFIG_URL
        },
        dwAutoDetectFlags: if settings.pac_url.is_empty() {
            WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A
        } else {
            0
        },
        lpszAutoConfigUrl: if settings.pac_url.is_empty() {
            ptr::null()
        } else {
            pac_url.as_ptr()
        },
        lpvReserved: ptr::null_mut(),
        dwReserved: 0,
        // PAC retrieval must not send this user's Windows/domain credentials.
        fAutoLogonIfChallenged: 0,
    }));
    if unsafe {
        WinHttpGetProxyForUrlEx(
            resolver.raw(),
            destination.as_ptr(),
            &*options.0,
            registration.0,
        )
    } != ERROR_IO_PENDING
    {
        return Err(failed());
    }
    Ok((
        Pending {
            _registration: registration,
            resolver,
            _session: session,
            _options: options,
            _pac_url: pac_url,
            _destination: destination,
        },
        completion,
    ))
}

async fn request(settings: SystemSettings, target: &Target) -> Result<ResolvedProxy, ChainError> {
    let _slot = SLOTS.acquire().await.map_err(|_| failed())?;
    let source = if settings.pac_url.is_empty() {
        "windows_wpad"
    } else {
        "windows_pac"
    };
    let (pending, completion) = start(&settings, target)?;
    if !completion.await.map_err(|_| failed())? {
        return Err(failed());
    }
    let hop = result(&pending.resolver)?;
    Ok(ResolvedProxy {
        hop,
        source,
        resolved_for: Some(authority(target)),
        has_bypass: settings.has_bypass,
    })
}

pub(super) async fn resolve(
    settings: SystemSettings,
    target: &Target,
) -> Result<ResolvedProxy, ChainError> {
    tokio::time::timeout(DEADLINE, request(settings, target))
        .await
        .map_err(|_| timed_out())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proxy(host: &str) -> Candidate {
        Candidate {
            proxy: true,
            bypass: false,
            scheme: WINHTTP_INTERNET_SCHEME_HTTP,
            host: host.into(),
            port: 7890,
        }
    }
    #[test]
    fn pac_deduplicates_one_http_proxy_but_never_uses_direct_or_ambiguous_routes() {
        let selected = select(vec![proxy("127.0.0.1"), proxy("127.0.0.1")]).unwrap();
        assert_eq!(selected.host, "127.0.0.1");
        assert_eq!(
            select(vec![proxy("one.invalid"), proxy("two.invalid")])
                .err()
                .unwrap()
                .code,
            "proxy_chain_auto_ambiguous"
        );
        let direct = Candidate {
            proxy: false,
            bypass: false,
            scheme: 0,
            host: String::new(),
            port: 0,
        };
        assert_eq!(
            select(vec![proxy("127.0.0.1"), direct]).err().unwrap().code,
            "proxy_chain_auto_direct_refused"
        );
        let mut socks = proxy("127.0.0.1");
        let mut bypass = proxy("127.0.0.1");
        bypass.bypass = true;
        assert!(select(vec![bypass]).is_ok());
        let bypass = Candidate { proxy: false, bypass: true, scheme: 0, host: String::new(), port: 0 };
        assert_eq!(
            select(vec![bypass]).err().unwrap().code,
            "proxy_chain_auto_direct_refused"
        );
        socks.scheme = WINHTTP_INTERNET_SCHEME_SOCKS;
        assert_eq!(
            select(vec![socks]).err().unwrap().code,
            "proxy_chain_auto_protocol_unsupported"
        );
    }
    #[test]
    fn pac_uses_the_actual_second_hop_host_and_port_including_ipv6() {
        assert_eq!(
            target_url(&Target::new("second.invalid", 4104).unwrap()),
            "https://second.invalid:4104/"
        );
        assert_eq!(
            target_url(&Target::new("::1", 4104).unwrap()),
            "https://[::1]:4104/"
        );
    }
    #[test]
    fn automatic_resolution_future_is_send() {
        fn require_send(_: impl Send) {}
        let target = Target::new("second.invalid", 4104).unwrap();
        let settings = SystemSettings {
            static_proxy: String::new(),
            pac_url: "http://127.0.0.1/proxy.pac".into(),
            auto_detect: false,
            has_bypass: false,
        };
        require_send(resolve(settings, &target));
    }
    #[tokio::test]
    async fn cancelled_request_ids_ignore_late_callbacks_and_do_not_affect_other_requests() {
        let (cancelled, cancelled_rx) = Registration::new().unwrap();
        let old_id = cancelled.0;
        let (active, active_rx) = Registration::new().unwrap();
        drop(cancelled);
        complete(old_id, true);
        assert!(cancelled_rx.await.is_err());
        complete(active.0, true);
        assert!(active_rx.await.unwrap());
        assert!(!requests().lock().unwrap().contains_key(&old_id));
        assert!(!requests().lock().unwrap().contains_key(&active.0));
    }

    #[tokio::test]
    #[ignore = "Native WinHTTP PAC execution against an owned loopback fixture; run explicitly on Windows."]
    async fn windows_pac_executes_for_actual_targets_and_rejects_direct() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let script = r#"function FindProxyForURL(url, host) {
                if (host === 'first.invalid') return 'PROXY 127.0.0.1:17890';
                if (host === 'second.invalid') return 'PROXY 127.0.0.1:27890';
                return 'DIRECT';
            }"#;
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") && head.len() < 16384 {
                    let Ok(byte) = stream.read_u8().await else {
                        break;
                    };
                    head.push(byte);
                }
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{script}", script.len());
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        struct Stop(tokio::task::JoinHandle<()>);
        impl Drop for Stop {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _stop = Stop(task);
        let settings = || SystemSettings {
            static_proxy: String::new(),
            pac_url: format!("http://127.0.0.1:{port}/proxy.pac"),
            auto_detect: false,
            has_bypass: false,
        };
        for (host, expected) in [("first.invalid", 17890), ("second.invalid", 27890)] {
            let route = resolve(settings(), &Target::new(host, 4104).unwrap())
                .await
                .unwrap();
            assert_eq!(route.hop.port, expected);
            assert_eq!(route.source, "windows_pac");
            assert_eq!(
                route.resolved_for.as_deref(),
                Some(format!("{host}:4104").as_str())
            );
        }
        let error = resolve(settings(), &Target::new("direct.invalid", 4104).unwrap())
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "proxy_chain_auto_direct_refused");
        assert!(requests().lock().unwrap().is_empty());
    }
}
