use super::{
    connector::{ProxyHop, ProxyKind, Target},
    error::ChainError,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct Detection {
    pub state: &'static str,
    pub source: &'static str,
    pub kind: Option<&'static str>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub has_bypass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_for: Option<String>,
    pub error_code: Option<&'static str>,
    pub error_message: Option<String>,
}
fn manual() -> ChainError {
    ChainError::new("proxy_chain_manual_required", "configuration", "Windows returned an ambiguous static proxy mapping. Select manual HTTP or SOCKS5 configuration.")
}

fn parse(raw: &str) -> Result<ProxyHop, ChainError> {
    if raw.trim().is_empty() {
        return Err(ChainError::new(
            "proxy_chain_system_not_configured",
            "configuration",
            "No static system proxy is configured. Select a manual proxy.",
        ));
    }
    let mut candidates = Vec::new();
    for item in raw.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let value = if let Some((scheme, value)) = item.split_once('=') {
            if !matches!(
                scheme.trim().to_ascii_lowercase().as_str(),
                "http" | "https"
            ) {
                return Err(manual());
            }
            value.trim()
        } else {
            item
        };
        let url = if value.contains("://") {
            value.to_string()
        } else {
            format!("http://{value}")
        };
        let parsed = url::Url::parse(&url).map_err(|_| manual())?;
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(manual());
        }
        let kind = match parsed.scheme() {
            "http" => ProxyKind::Http,
            "socks5" => ProxyKind::Socks5,
            _ => return Err(manual()),
        };
        let target = Target::new(
            parsed.host_str().ok_or_else(manual)?,
            parsed
                .port_or_known_default()
                .unwrap_or(if kind == ProxyKind::Socks5 { 1080 } else { 80 }),
        )?;
        let candidate = (kind, target.host, target.port);
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    if candidates.len() != 1 {
        return Err(manual());
    }
    let (kind, host, port) = candidates.pop().unwrap();
    Ok(ProxyHop {
        kind,
        host,
        port,
        username: String::new(),
        password: String::new(),
    })
}

// Do not derive Debug: a PAC URL can contain a private query token.
struct SystemSettings {
    static_proxy: String,
    pac_url: String,
    auto_detect: bool,
    has_bypass: bool,
}

pub struct ResolvedProxy {
    pub hop: ProxyHop,
    pub source: &'static str,
    pub resolved_for: Option<String>,
    pub has_bypass: bool,
}

impl ResolvedProxy {
    pub fn manual(hop: ProxyHop) -> Self {
        Self {
            hop,
            source: "manual",
            resolved_for: None,
            has_bypass: false,
        }
    }
}

fn auto_error(code: &'static str, message: &'static str) -> ChainError {
    ChainError::new(code, "configuration", message)
}

fn authority(target: &Target) -> String {
    if target.host.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{}]:{}", target.host, target.port)
    } else {
        format!("{}:{}", target.host, target.port)
    }
}

impl SystemSettings {
    fn static_proxy(&self) -> Result<Option<ResolvedProxy>, ChainError> {
        // WinHTTP only returns lpszProxy when the user's static proxy is enabled.
        // This feature selects an explicit first hop (as AdsPower does), rather
        // than silently allowing PAC/bypass rules to turn that hop into DIRECT.
        if !self.static_proxy.trim().is_empty() {
            return Ok(Some(ResolvedProxy {
                hop: parse(&self.static_proxy)?,
                source: "windows_current_user_internet_settings",
                resolved_for: None,
                has_bypass: self.has_bypass,
            }));
        }
        if self.pac_url.is_empty() && !self.auto_detect {
            return Err(auto_error(
                "proxy_chain_system_not_configured",
                "No system proxy is configured. Enable a system proxy or select a manual proxy.",
            ));
        }
        Ok(None)
    }
}

#[cfg(windows)]
fn current() -> Result<SystemSettings, ChainError> {
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        Networking::WinHttp::{
            WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
        },
    };
    unsafe fn text(p: *mut u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut len = 0;
        while len < 32768 && unsafe { *p.add(len) } != 0 {
            len += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
    }
    // SAFETY: API initializes the struct. Its strings are copied, then freed.
    unsafe {
        let mut config: WINHTTP_CURRENT_USER_IE_PROXY_CONFIG = std::mem::zeroed();
        if WinHttpGetIEProxyConfigForCurrentUser(&mut config) == 0 {
            return Err(ChainError::new(
                "proxy_chain_system_detect_failed",
                "configuration",
                "Windows system proxy settings could not be read. Select a manual proxy.",
            ));
        }
        let settings = SystemSettings {
            static_proxy: text(config.lpszProxy),
            has_bypass: !text(config.lpszProxyBypass).is_empty(),
            pac_url: text(config.lpszAutoConfigUrl),
            auto_detect: config.fAutoDetect != 0,
        };
        for p in [
            config.lpszProxy,
            config.lpszProxyBypass,
            config.lpszAutoConfigUrl,
        ] {
            if !p.is_null() {
                GlobalFree(p as _);
            }
        }
        Ok(settings)
    }
}
#[cfg(not(windows))]
fn current() -> Result<SystemSettings, ChainError> {
    Err(ChainError::new(
        "proxy_chain_windows_required",
        "configuration",
        "System proxy chaining is available on Windows only.",
    ))
}

pub fn resolve() -> Result<ProxyHop, ChainError> {
    current()?
        .static_proxy()?
        .map(|resolved| resolved.hop)
        .ok_or_else(|| {
            auto_error(
        "proxy_chain_auto_target_required",
        "Windows uses automatic proxy configuration. Select a profile to resolve its proxy route.",
    )
        })
}

/// Resolve exactly the second proxy's CONNECT destination, not a website whose
/// PAC route might differ. The result stays fixed for this test or Session.
pub async fn resolve_for(target: &Target) -> Result<ResolvedProxy, ChainError> {
    let settings = current()?;
    if let Some(resolved) = settings.static_proxy()? {
        return Ok(resolved);
    }
    #[cfg(windows)]
    {
        pac::resolve(settings, target).await
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        unreachable!("current returns a Windows-only error on this platform")
    }
}

fn detection(result: Result<ResolvedProxy, ChainError>, target: Option<&Target>) -> Detection {
    match result {
        Ok(ResolvedProxy {
            hop,
            source,
            resolved_for,
            has_bypass,
        }) => Detection {
            state: "ok",
            source,
            kind: Some(if hop.kind == ProxyKind::Http {
                "http"
            } else {
                "socks5"
            }),
            host: Some(hop.host),
            port: Some(hop.port),
            has_bypass,
            resolved_for,
            error_code: None,
            error_message: None,
        },
        Err(e) => Detection {
            state: if e.code == "proxy_chain_manual_required" {
                "manual_required"
            } else {
                "failed"
            },
            source: "windows_current_user_internet_settings",
            kind: None,
            host: None,
            port: None,
            has_bypass: false,
            resolved_for: target.map(authority),
            error_code: Some(e.code),
            error_message: Some(e.message),
        },
    }
}

pub fn detect() -> Detection {
    detection(
        current().and_then(|settings| {
            settings.static_proxy()?.ok_or_else(|| {
                auto_error(
        "proxy_chain_auto_target_required",
        "Windows uses automatic proxy configuration. Select a profile to resolve its proxy route.",
    )
            })
        }),
        None,
    )
}

pub async fn detect_for(target: &Target) -> Detection {
    detection(resolve_for(target).await, Some(target))
}

#[cfg(windows)]
#[path = "auto_windows.rs"]
mod pac;

#[cfg(test)]
mod tests {
    #[test]
    fn static_mapping_is_http_but_ambiguous_socks_and_mappings_are_refused() {
        let a = super::parse("http=127.0.0.1:7890;https=127.0.0.1:7890").unwrap();
        assert_eq!(a.kind, super::ProxyKind::Http);
        assert_eq!(a.port, 7890);
        assert!(super::parse("socks=127.0.0.1:7890").is_err());
        assert!(super::parse("http=one:80;https=two:80").is_err());
    }

    #[test]
    fn enabled_static_proxy_is_not_blocked_by_pac_or_wpad() {
        let settings = super::SystemSettings {
            static_proxy: "127.0.0.1:7890".into(),
            pac_url: "https://pac.invalid/private-token".into(),
            auto_detect: true,
            has_bypass: true,
        };
        let resolved = settings.static_proxy().unwrap().unwrap();
        assert_eq!(resolved.hop.host, "127.0.0.1");
        assert_eq!(resolved.hop.port, 7890);
        assert_eq!(resolved.source, "windows_current_user_internet_settings");
        assert!(resolved.has_bypass);
        let dto = serde_json::to_string(&super::detection(Ok(resolved), None)).unwrap();
        assert!(!dto.contains("private-token"));
    }

    #[test]
    fn absent_static_configuration_requires_target_resolution_or_fails_closed() {
        let mut settings = super::SystemSettings {
            static_proxy: String::new(),
            pac_url: String::new(),
            auto_detect: true,
            has_bypass: false,
        };
        assert!(settings.static_proxy().unwrap().is_none());
        settings.auto_detect = false;
        assert_eq!(
            settings.static_proxy().err().unwrap().code,
            "proxy_chain_system_not_configured"
        );
        settings.static_proxy = "http=one:80;https=two:80".into();
        settings.pac_url = "https://pac.invalid/".into();
        assert_eq!(
            settings.static_proxy().err().unwrap().code,
            "proxy_chain_manual_required"
        );
    }
}
