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
    pub error_code: Option<&'static str>,
    pub error_message: Option<String>,
}
fn manual() -> ChainError {
    ChainError::new("proxy_chain_manual_required", "configuration", "Windows uses PAC/WPAD or an ambiguous proxy mapping. Select manual HTTP or SOCKS5 configuration.")
}

fn parse(raw: &str, automatic: bool) -> Result<ProxyHop, ChainError> {
    if automatic {
        return Err(manual());
    }
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

#[cfg(windows)]
fn current() -> Result<(ProxyHop, bool), ChainError> {
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
        let raw = text(config.lpszProxy);
        let bypass = !text(config.lpszProxyBypass).is_empty();
        let automatic = config.fAutoDetect != 0 || !text(config.lpszAutoConfigUrl).is_empty();
        for p in [
            config.lpszProxy,
            config.lpszProxyBypass,
            config.lpszAutoConfigUrl,
        ] {
            if !p.is_null() {
                GlobalFree(p as _);
            }
        }
        Ok((parse(&raw, automatic)?, bypass))
    }
}
#[cfg(not(windows))]
fn current() -> Result<(ProxyHop, bool), ChainError> {
    Err(ChainError::new(
        "proxy_chain_windows_required",
        "configuration",
        "System proxy chaining is available on Windows only.",
    ))
}

pub fn resolve() -> Result<ProxyHop, ChainError> {
    current().map(|(hop, _)| hop)
}
pub fn detect() -> Detection {
    match current() {
        Ok((hop, has_bypass)) => Detection {
            state: "ok",
            source: "windows_current_user_internet_settings",
            kind: Some(if hop.kind == ProxyKind::Http {
                "http"
            } else {
                "socks5"
            }),
            host: Some(hop.host),
            port: Some(hop.port),
            has_bypass,
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
            error_code: Some(e.code),
            error_message: Some(e.message),
        },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn static_mapping_is_http_but_ambiguous_socks_and_pac_are_refused() {
        let a = super::parse("http=127.0.0.1:7890;https=127.0.0.1:7890", false).unwrap();
        assert_eq!(a.kind, super::ProxyKind::Http);
        assert_eq!(a.port, 7890);
        assert!(super::parse("socks=127.0.0.1:7890", false).is_err());
        assert!(super::parse("127.0.0.1:7890", true).is_err());
        assert!(super::parse("http=one:80;https=two:80", false).is_err());
    }
}
