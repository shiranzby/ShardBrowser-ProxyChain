//! Provider configuration is local to this Windows account. Credential pairs
//! are protected with DPAPI; view responses never include saved passwords.
use crate::{
    proxy_chain::credentials,
    space::proxy::{ProxyEntry, ProxyKind},
    store,
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, sync::Mutex};
static WRITE: Mutex<()> = Mutex::new(());
#[derive(Default, Serialize, Deserialize)]
pub struct Credential {
    pub username: String,
    pub password: String,
}
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    relay: String,
    username_template: String,
    tnb_server: String,
    tnb_plan: String,
    tnb_kind: String,
    account: Option<String>,
    tnb_api: Option<String>,
    tnb_proxy: Option<String>,
}
impl Default for Stored {
    fn default() -> Self {
        Self {
            relay: "us.1024proxy.io:3000".into(),
            username_template: "{username}{location}".into(),
            tnb_server: "http://usdata.adv.tnbproxy.com:9090".into(),
            tnb_plan: "advanced_traffic".into(),
            tnb_kind: "http".into(),
            account: None,
            tnb_api: None,
            tnb_proxy: None,
        }
    }
}
#[derive(Serialize)]
pub struct ProviderView {
    pub relay: String,
    pub username: String,
    pub username_template: String,
    pub password_saved: bool,
    pub tnb_server: String,
    pub tnb_plan: String,
    pub tnb_kind: String,
    pub tnb_api_username: String,
    pub tnb_api_password_saved: bool,
    pub tnb_proxy_username: String,
    pub tnb_proxy_password_saved: bool,
}
#[derive(Deserialize)]
pub struct ProviderUpdate {
    pub relay: String,
    pub username: String,
    pub username_template: String,
    pub password: Option<String>,
    pub tnb_server: String,
    pub tnb_plan: String,
    pub tnb_kind: String,
    #[serde(default)]
    pub tnb_api_username: String,
    pub tnb_api_password: Option<String>,
    #[serde(default)]
    pub tnb_proxy_username: String,
    pub tnb_proxy_password: Option<String>,
}
fn path() -> Result<std::path::PathBuf> {
    Ok(store::config_root()?.join("proxy-provider-defaults.json"))
}
fn load() -> Result<Stored> {
    let path = path()?;
    if !path.exists() {
        return Ok(Stored::default());
    }
    serde_json::from_slice(&fs::read(path)?).map_err(|_| anyhow!("provider_defaults_unreadable"))
}
fn decode(raw: Option<&str>) -> Result<Credential> {
    match raw {
        None => Ok(Credential::default()),
        Some(raw) => {
            let text = credentials::unprotect(raw)
                .map_err(|_| anyhow!("provider_credentials_unavailable"))?;
            serde_json::from_str(&text).map_err(|_| anyhow!("provider_credentials_unavailable"))
        }
    }
}
fn view(v: Stored) -> Result<ProviderView> {
    let a = decode(v.account.as_deref())?;
    let api = decode(v.tnb_api.as_deref())?;
    let proxy = decode(v.tnb_proxy.as_deref())?;
    Ok(ProviderView {
        relay: v.relay,
        username: a.username,
        username_template: v.username_template,
        password_saved: !a.password.is_empty(),
        tnb_server: v.tnb_server,
        tnb_plan: v.tnb_plan,
        tnb_kind: v.tnb_kind,
        tnb_api_username: api.username,
        tnb_api_password_saved: !api.password.is_empty(),
        tnb_proxy_username: proxy.username,
        tnb_proxy_password_saved: !proxy.password.is_empty(),
    })
}
pub fn get() -> Result<ProviderView> {
    view(load()?)
}
fn valid_text(raw: &str, max: usize) -> bool {
    raw.len() <= max && !raw.chars().any(char::is_control)
}
fn relay(raw: &str) -> Result<(String, u16)> {
    let u = url::Url::parse(&format!("socks5://{}", raw.trim()))
        .map_err(|_| anyhow!("provider_invalid_relay"))?;
    if !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || !matches!(u.path(), "" | "/")
    {
        return Err(anyhow!("provider_invalid_relay"));
    }
    Ok((
        u.host_str()
            .ok_or_else(|| anyhow!("provider_invalid_relay"))?
            .to_owned(),
        u.port().ok_or_else(|| anyhow!("provider_invalid_relay"))?,
    ))
}
fn seal(username: String, password: Option<String>, old: Option<&str>) -> Result<Option<String>> {
    if !valid_text(&username, 1024) {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    let password = match password {
        Some(p) => p,
        None => decode(old)?.password,
    };
    if !valid_text(&password, 4096) {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    if username.is_empty() && password.is_empty() {
        return Ok(None);
    }
    let text = serde_json::to_string(&Credential { username, password })?;
    Ok(Some(credentials::protect(&text).map_err(|_| {
        anyhow!("provider_credentials_unavailable")
    })?))
}
pub fn save(v: ProviderUpdate) -> Result<ProviderView> {
    let _guard = WRITE
        .lock()
        .map_err(|_| anyhow!("provider_defaults_busy"))?;
    relay(&v.relay)?;
    if !valid_text(&v.username_template, 2048)
        || !v.username_template.contains("{username}")
        || !matches!(v.tnb_kind.as_str(), "http" | "socks5")
    {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    let rest = v
        .username_template
        .replace("{username}", "")
        .replace("{country}", "")
        .replace("{state}", "")
        .replace("{location}", "");
    if rest.contains('{') || rest.contains('}') {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    let url =
        url::Url::parse(v.tnb_server.trim()).map_err(|_| anyhow!("provider_defaults_invalid"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/" | "/gen")
        || v.tnb_plan.is_empty()
        || v.tnb_plan.len() > 64
        || !v
            .tnb_plan
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    let old = load()?;
    let saved = Stored {
        relay: v.relay.trim().into(),
        username_template: v.username_template,
        tnb_server: v.tnb_server.trim().into(),
        tnb_plan: v.tnb_plan,
        tnb_kind: v.tnb_kind,
        account: seal(v.username, v.password, old.account.as_deref())?,
        tnb_api: seal(
            v.tnb_api_username,
            v.tnb_api_password,
            old.tnb_api.as_deref(),
        )?,
        tnb_proxy: seal(
            v.tnb_proxy_username,
            v.tnb_proxy_password,
            old.tnb_proxy.as_deref(),
        )?,
    };
    // Only ciphertext enters the temporary file; preserve the old file on errors.
    let target = path()?;
    let tmp = target.with_extension("pending");
    fs::write(&tmp, serde_json::to_vec_pretty(&saved)?)?;
    replace_file(&tmp, &target)?;
    view(saved)
}

#[cfg(not(windows))]
fn replace_file(tmp: &std::path::Path, target: &std::path::Path) -> Result<()> {
    fs::rename(tmp, target)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(tmp: &std::path::Path, target: &std::path::Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let from: Vec<u16> = tmp.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let to: Vec<u16> = target.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) };
    if ok == 0 {
        let error = std::io::Error::last_os_error();
        let _ = fs::remove_file(tmp);
        return Err(error.into());
    }
    Ok(())
}
/// Separate authentication domains. Neither is inferred from the other.
pub fn tnb_credentials(api: bool) -> Result<Credential> {
    let v = load()?;
    decode(if api {
        v.tnb_api.as_deref()
    } else {
        v.tnb_proxy.as_deref()
    })
}
#[derive(Deserialize)]
pub struct GenerateRequest {
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub state: String,
}
pub fn generate(v: GenerateRequest) -> Result<ProxyEntry> {
    let c = load()?;
    let account = decode(c.account.as_deref())?;
    if account.username.is_empty() || account.password.is_empty() {
        return Err(anyhow!("provider_credentials_required"));
    }
    for value in [&v.country, &v.state] {
        if value.len() > 128
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' '))
        {
            return Err(anyhow!("provider_region_invalid"));
        }
    }
    if (!v.country.is_empty()
        && !c.username_template.contains("{country}")
        && !c.username_template.contains("{location}"))
        || (!v.state.is_empty()
            && !c.username_template.contains("{state}")
            && !c.username_template.contains("{location}"))
    {
        return Err(anyhow!("provider_region_template_required"));
    }
    if (!v.country.is_empty()
        && (v.country.len() != 2 || !v.country.bytes().all(|b| b.is_ascii_alphabetic())))
        || (!v.state.is_empty() && v.country.is_empty())
    {
        return Err(anyhow!("provider_region_invalid"));
    }
    let location = if v.country.is_empty() {
        String::new()
    } else {
        format!(
            "-region-{}{}",
            v.country.to_ascii_uppercase(),
            if v.state.is_empty() {
                String::new()
            } else {
                format!("-st-{}", v.state)
            }
        )
    };
    let (host, port) = relay(&c.relay)?;
    let username = c
        .username_template
        .replace("{username}", &account.username)
        .replace("{country}", &v.country.to_ascii_lowercase())
        .replace("{state}", &v.state)
        .replace("{location}", &location);
    Ok(ProxyEntry {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!(
            "1024Proxy {}",
            if v.country.is_empty() {
                "Global"
            } else {
                &v.country
            }
        ),
        kind: ProxyKind::Socks5,
        host,
        port,
        username,
        password: account.password,
        provider_id: "1024proxy".into(),
        credential_ref: String::new(),
        provider_options: HashMap::from([
            ("country".into(), v.country.clone()),
            ("state".into(), v.state),
        ]),
        capabilities: crate::space::proxy::ProxyCapabilities {
            socks5: true,
            ..Default::default()
        },
        country: v.country.to_ascii_uppercase(),
        notes: String::new(),
        proxy_chain_policy: Default::default(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_defaults_are_non_secret_and_tnb_use_us() {
        let v = view(Stored::default()).unwrap();
        assert_eq!(v.tnb_server, "http://usdata.adv.tnbproxy.com:9090");
        let json = serde_json::to_value(v).unwrap();
        assert!(json.get("account").is_none());
        assert_eq!(json["password_saved"], false);
    }
    #[test]
    fn relay_rejects_credentials_and_missing_port() {
        assert!(relay("u:p@example.com:3000").is_err());
        assert!(relay("example.com").is_err());
        assert_eq!(
            relay("us.1024proxy.io:3000").unwrap(),
            ("us.1024proxy.io".into(), 3000)
        );
    }
    #[cfg(windows)]
    #[test]
    fn provider_defaults_encrypt_separate_credentials_and_restore_after_edit() {
        let _root = store::test_root("provider-defaults");
        let update = |password| ProviderUpdate {
            relay: "us.1024proxy.io:3000".into(),
            username: "synthetic-user".into(),
            username_template: "{username}-country-{country}".into(),
            password,
            tnb_server: "http://usdata.adv.tnbproxy.com:9090".into(),
            tnb_plan: "advanced_traffic".into(),
            tnb_kind: "socks5".into(),
            tnb_api_username: "extract-user".into(),
            tnb_api_password: Some("extract-secret".into()),
            tnb_proxy_username: "exit-user".into(),
            tnb_proxy_password: Some("exit-secret".into()),
        };
        save(update(Some("provider-secret".into()))).unwrap();
        let body = fs::read_to_string(path().unwrap()).unwrap();
        for s in [
            "provider-secret",
            "synthetic-user",
            "extract-secret",
            "exit-secret",
        ] {
            assert!(!body.contains(s));
        }
        assert!(save(update(None)).unwrap().password_saved);
        let e = generate(GenerateRequest {
            country: "EG".into(),
            state: String::new(),
        })
        .unwrap();
        assert_eq!(e.username, "synthetic-user-country-eg");
        assert_eq!(e.password, "provider-secret");
        assert_eq!(tnb_credentials(true).unwrap().username, "extract-user");
        assert_eq!(tnb_credentials(false).unwrap().username, "exit-user");
        assert!(!serde_json::to_string(&get().unwrap())
            .unwrap()
            .contains("secret"));
    }
}
