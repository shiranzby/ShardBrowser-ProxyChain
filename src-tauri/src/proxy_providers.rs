//! Provider configuration is local to this Windows account. Credential pairs
//! are protected with DPAPI; view responses never include saved passwords.
use crate::{
    proxy_chain::credentials,
    space::proxy::{ProxyEntry, ProxyKind},
    store,
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, HashMap}, fs, sync::Mutex};
static WRITE: Mutex<()> = Mutex::new(());
#[derive(Default, Serialize, Deserialize)]
pub struct Credential {
    pub username: String,
    pub password: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    #[serde(default)]
    name: String,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    city: String,
    #[serde(default = "default_session_type")]
    session_type: String,
    #[serde(default = "default_session_minutes")]
    session_minutes: u32,
    #[serde(default = "default_generation_count")]
    generation_count: u32,
    relay: String,
    username_template: String,
    tnb_server: String,
    tnb_plan: String,
    tnb_kind: String,
    account: Option<String>,
    tnb_api: Option<String>,
    tnb_proxy: Option<String>,
    custom_name: String,
    custom_server: String,
    custom_extract_url: String,
    custom_plan: String,
    custom_kind: String,
    custom_website: String,
    custom_whitelist: String,
    custom_api: Option<String>,
    custom_proxy: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
struct ProviderFile {
    #[serde(flatten)]
    defaults: Stored,
    #[serde(default)]
    custom_providers: BTreeMap<uuid::Uuid, Stored>,
    #[serde(default)]
    tnb_generation: Option<GenerationDefaults>,
}

#[derive(Serialize, Deserialize)]
struct GenerationDefaults {
    folder: String,
    country: String,
    state: String,
    city: String,
    session_type: String,
    session_minutes: u32,
    generation_count: u32,
}

impl GenerationDefaults {
    fn from_stored(value: &Stored) -> Self {
        Self {
            folder: value.folder.clone(),
            country: value.country.clone(),
            state: value.state.clone(),
            city: value.city.clone(),
            session_type: value.session_type.clone(),
            session_minutes: value.session_minutes,
            generation_count: value.generation_count,
        }
    }

    fn apply(self, value: &mut Stored) {
        value.folder = self.folder;
        value.country = self.country;
        value.state = self.state;
        value.city = self.city;
        value.session_type = self.session_type;
        value.session_minutes = self.session_minutes;
        value.generation_count = self.generation_count;
    }
}

#[derive(Serialize)]
pub struct CustomProviderSummary {
    id: uuid::Uuid,
    name: String,
}
fn default_session_type() -> String { "sticky".into() }
fn default_session_minutes() -> u32 { 180 }
fn default_generation_count() -> u32 { 1 }
fn custom_provider_name(value: &Stored) -> &str {
    let name = value.name.trim();
    if name.is_empty() {
        value.custom_name.trim()
    } else {
        name
    }
}
fn normalize_custom_provider(value: &mut Stored) {
    let name = custom_provider_name(value).to_owned();
    value.name = name.clone();
    value.custom_name = name;
}
impl Default for Stored {
    fn default() -> Self {
        Self {
            name: "1024Proxy".into(),
            folder: String::new(),
            country: String::new(),
            state: String::new(),
            city: String::new(),
            session_type: default_session_type(),
            session_minutes: default_session_minutes(),
            generation_count: default_generation_count(),
            relay: "us.1024proxy.io:3000".into(),
            username_template: "{username}{location}".into(),
            tnb_server: "http://usdata.adv.tnbproxy.com:9090".into(),
            tnb_plan: "advanced_traffic".into(),
            tnb_kind: "http".into(),
            account: None,
            tnb_api: None,
            tnb_proxy: None,
            custom_name: "Custom provider".into(),
            custom_server: String::new(),
            custom_extract_url: String::new(),
            custom_plan: String::new(),
            custom_kind: "http".into(),
            custom_website: String::new(),
            custom_whitelist: String::new(),
            custom_api: None,
            custom_proxy: None,
        }
    }
}
#[derive(Serialize)]
pub struct ProviderView {
    pub custom_providers: Vec<CustomProviderSummary>,
    pub name: String,
    pub folder: String,
    pub country: String,
    pub state: String,
    pub city: String,
    pub session_type: String,
    pub session_minutes: u32,
    pub generation_count: u32,
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
    pub custom_name: String,
    pub custom_server: String,
    pub custom_extract_url: String,
    pub custom_plan: String,
    pub custom_kind: String,
    pub custom_website: String,
    pub custom_whitelist: String,
    pub custom_api_username: String,
    pub custom_api_password_saved: bool,
    pub custom_proxy_username: String,
    pub custom_proxy_password_saved: bool,
}
#[derive(Deserialize)]
pub struct ProviderUpdate {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub city: String,
    #[serde(default = "default_session_type")]
    pub session_type: String,
    #[serde(default = "default_session_minutes")]
    pub session_minutes: u32,
    #[serde(default = "default_generation_count")]
    pub generation_count: u32,
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
    #[serde(default)]
    pub custom_name: String,
    #[serde(default)]
    pub custom_server: String,
    #[serde(default)]
    pub custom_extract_url: String,
    #[serde(default)]
    pub custom_plan: String,
    #[serde(default)]
    pub custom_kind: String,
    #[serde(default)]
    pub custom_website: String,
    #[serde(default)]
    pub custom_whitelist: String,
    #[serde(default)]
    pub custom_api_username: String,
    pub custom_api_password: Option<String>,
    #[serde(default)]
    pub custom_proxy_username: String,
    pub custom_proxy_password: Option<String>,
}
fn path() -> Result<std::path::PathBuf> {
    Ok(store::config_root()?.join("proxy-provider-defaults.json"))
}
fn load_file() -> Result<ProviderFile> {
    let path = path()?;
    if !path.exists() {
        return Ok(ProviderFile::default());
    }
    let mut file: ProviderFile =
        serde_json::from_slice(&fs::read(path)?).map_err(|_| anyhow!("provider_defaults_unreadable"))?;
    for provider in file.custom_providers.values_mut() {
        normalize_custom_provider(provider);
    }
    Ok(file)
}
fn load() -> Result<Stored> {
    Ok(load_file()?.defaults)
}

impl ProviderFile {
    fn summaries(&self) -> Vec<CustomProviderSummary> {
        self.custom_providers.iter().map(|(&id, provider)| CustomProviderSummary {
            id,
            name: custom_provider_name(provider).to_owned(),
        }).collect()
    }
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
    let custom_api = decode(v.custom_api.as_deref())?;
    let custom_proxy = decode(v.custom_proxy.as_deref())?;
    Ok(ProviderView {
        custom_providers: Vec::new(),
        name: v.name,
        folder: v.folder,
        country: v.country,
        state: v.state,
        city: v.city,
        session_type: v.session_type,
        session_minutes: v.session_minutes,
        generation_count: v.generation_count,
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
        custom_name: v.custom_name,
        custom_server: v.custom_server,
        custom_extract_url: v.custom_extract_url,
        custom_plan: v.custom_plan,
        custom_kind: v.custom_kind,
        custom_website: v.custom_website,
        custom_whitelist: v.custom_whitelist,
        custom_api_username: custom_api.username,
        custom_api_password_saved: !custom_api.password.is_empty(),
        custom_proxy_username: custom_proxy.username,
        custom_proxy_password_saved: !custom_proxy.password.is_empty(),
    })
}
pub fn get() -> Result<ProviderView> {
    get_for(None, false, false)
}

pub fn get_for(id: Option<uuid::Uuid>, create: bool, tnb: bool) -> Result<ProviderView> {
    let mut file = load_file()?;
    let summaries = file.summaries();
    let mut value = match id {
        Some(_) if create => Stored::default(),
        Some(id) => file.custom_providers.remove(&id).ok_or_else(|| anyhow!("provider_not_found"))?,
        None => file.defaults,
    };
    if id.is_some() {
        if create {
            value.name = value.custom_name.clone();
        }
        normalize_custom_provider(&mut value);
    }
    if tnb && id.is_none() {
        if let Some(defaults) = file.tnb_generation {
            defaults.apply(&mut value);
        }
    } else if !tnb && id.is_none() {
        // The built-in 1024Proxy route supports sticky sessions up to 120
        // minutes. Keep the displayed and saved default aligned with the
        // value used by generation, including legacy files that used 180.
        value.session_minutes = value.session_minutes.clamp(1, 120);
    }
    let mut result = view(value)?;
    result.custom_providers = summaries;
    Ok(result)
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
    save_for(None, false, false, v)
}

pub fn save_for(id: Option<uuid::Uuid>, create: bool, tnb: bool, v: ProviderUpdate) -> Result<ProviderView> {
    let _guard = WRITE
        .lock()
        .map_err(|_| anyhow!("provider_defaults_busy"))?;
    relay(&v.relay)?;
    if !matches!(v.session_type.as_str(), "sticky" | "rotating" | "per_request")
        || !(1..=1440).contains(&v.session_minutes)
        || !(1..=1000).contains(&v.generation_count)
    {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    for text in [&v.folder, &v.country, &v.state, &v.city] {
        if !valid_text(text, 256) {
            return Err(anyhow!("provider_defaults_invalid"));
        }
    }
    if !valid_text(&v.name, 256) {
        return Err(anyhow!("provider_defaults_invalid"));
    }
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
        .replace("{city}", "")
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
    for text in [
        &v.custom_name,
        &v.custom_server,
        &v.custom_plan,
        &v.custom_website,
        &v.custom_whitelist,
    ] {
        if !valid_text(text, 2048) {
            return Err(anyhow!("provider_defaults_invalid"));
        }
    }
    if !v.custom_kind.is_empty() && !matches!(v.custom_kind.as_str(), "http" | "https" | "socks5") {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    if !v.custom_server.is_empty() {
        let custom = url::Url::parse(v.custom_server.trim())
            .map_err(|_| anyhow!("provider_defaults_invalid"))?;
        if !matches!(custom.scheme(), "http" | "https" | "socks5")
            || custom.host_str().is_none()
            || !custom.username().is_empty()
            || custom.password().is_some()
            || custom.query().is_some()
            || custom.fragment().is_some()
        {
            return Err(anyhow!("provider_defaults_invalid"));
        }
    }
    for url_text in [&v.custom_website, &v.custom_whitelist] {
        if !url_text.is_empty() {
            let u = url::Url::parse(url_text).map_err(|_| anyhow!("provider_defaults_invalid"))?;
            if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() {
                return Err(anyhow!("provider_defaults_invalid"));
            }
        }
    }
    let extract_url = v.custom_extract_url.trim();
    if !extract_url.is_empty() {
        custom_extract_url(extract_url)?;
    }
    let mut v = v;
    let custom_provider_name = if id.is_some() {
        if v.name.trim().is_empty() {
            v.custom_name.trim().to_owned()
        } else {
            v.name.trim().to_owned()
        }
    } else {
        String::new()
    };
    if id.is_some()
        && (custom_provider_name.is_empty()
            || (v.custom_server.trim().is_empty() && extract_url.is_empty()))
    {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    if !tnb && id.is_none() {
        v.session_minutes = v.session_minutes.clamp(1, 120);
    }
    let mut file = load_file()?;
    let empty = Stored::default();
    let old = match id {
        Some(id) if create => {
            if file.custom_providers.contains_key(&id) {
                return Err(anyhow!("provider_defaults_invalid"));
            }
            &empty
        }
        Some(id) => file.custom_providers.get(&id).ok_or_else(|| anyhow!("provider_not_found"))?,
        None => &file.defaults,
    };
    let mut saved = Stored {
        name: if id.is_some() {
            custom_provider_name.clone()
        } else if v.name.trim().is_empty() {
            old.name.clone()
        } else {
            v.name.trim().into()
        },
        folder: v.folder.trim().into(),
        country: v.country.trim().to_uppercase(),
        state: v.state.trim().into(),
        city: v.city.trim().into(),
        session_type: v.session_type,
        session_minutes: v.session_minutes,
        generation_count: v.generation_count,
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
        custom_name: if id.is_some() {
            custom_provider_name
        } else {
            v.custom_name.trim().into()
        },
        custom_server: v.custom_server.trim().into(),
        custom_extract_url: v.custom_extract_url.trim().into(),
        custom_plan: v.custom_plan.trim().into(),
        custom_kind: if v.custom_kind.is_empty() {
            "http".into()
        } else {
            v.custom_kind
        },
        custom_website: v.custom_website.trim().into(),
        custom_whitelist: v.custom_whitelist.trim().into(),
        custom_api: seal(
            v.custom_api_username,
            v.custom_api_password,
            old.custom_api.as_deref(),
        )?,
        custom_proxy: seal(
            v.custom_proxy_username,
            v.custom_proxy_password,
            old.custom_proxy.as_deref(),
        )?,
    };
    let mut result = view(saved.clone())?;
    match id {
        Some(id) => { file.custom_providers.insert(id, saved); }
        None => {
            if tnb {
                file.tnb_generation = Some(GenerationDefaults::from_stored(&saved));
                GenerationDefaults::from_stored(&file.defaults).apply(&mut saved);
            } else if file.tnb_generation.is_none() {
                // Both built-ins inherit the old shared settings on their first edit.
                file.tnb_generation = Some(GenerationDefaults::from_stored(&file.defaults));
            }
            file.defaults = saved;
        }
    }
    result.custom_providers = file.summaries();
    write_file(&file)?;
    Ok(result)
}

fn write_file(file: &ProviderFile) -> Result<()> {
    let target = path()?;
    let tmp = target.with_extension("pending");
    fs::write(&tmp, serde_json::to_vec_pretty(file)?)?;
    replace_file(&tmp, &target)
}

pub fn delete_custom(id: uuid::Uuid) -> Result<()> {
    let _guard = WRITE.lock().map_err(|_| anyhow!("provider_defaults_busy"))?;
    let mut file = load_file()?;
    file.custom_providers.remove(&id);
    write_file(&file)
}

#[cfg(not(windows))]
fn replace_file(tmp: &std::path::Path, target: &std::path::Path) -> Result<()> {
    fs::rename(tmp, target)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(tmp: &std::path::Path, target: &std::path::Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from: Vec<u16> = tmp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let to: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
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
    pub custom: bool,
    #[serde(default)]
    pub custom_provider_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub session_minutes: Option<u32>,
    #[serde(default = "default_generation_count")]
    pub count: u32,
}

impl GenerateRequest {
    fn relay_session_minutes(&self, defaults: &Stored) -> Result<u32> {
        match self.session_minutes {
            Some(minutes @ 0..=120) => Ok(minutes),
            Some(_) => Err(anyhow!("provider_defaults_invalid")),
            None if defaults.session_type == "sticky" => Ok(defaults.session_minutes.clamp(1, 120)),
            None => Ok(0),
        }
    }
}
pub fn generate(v: GenerateRequest) -> Result<ProxyEntry> {
    let mut file = load_file()?;
    let c = match v.custom_provider_id {
        Some(id) => file.custom_providers.remove(&id).ok_or_else(|| anyhow!("provider_not_found"))?,
        None => file.defaults,
    };
    generate_entry(&c, &v, uuid::Uuid::new_v4().as_u128() as u32)
}

fn generate_entry(c: &Stored, v: &GenerateRequest, session_id: u32) -> Result<ProxyEntry> {
    if v.custom || v.custom_provider_id.is_some() {
        let mut entry = generate_custom(c)?;
        if let Some(id) = v.custom_provider_id {
            entry.provider_options.insert("custom_provider_id".into(), id.to_string());
        }
        return Ok(entry);
    }
    let account = decode(c.account.as_deref())?;
    if account.username.is_empty() || account.password.is_empty() {
        return Err(anyhow!("provider_credentials_required"));
    }
    for value in [&v.country, &v.state, &v.city] {
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
        || (!v.city.is_empty()
            && !c.username_template.contains("{city}")
            && !c.username_template.contains("{location}"))
    {
        return Err(anyhow!("provider_region_template_required"));
    }
    if (!v.country.is_empty()
        && (v.country.len() != 2 || !v.country.bytes().all(|b| b.is_ascii_alphabetic())))
        || (!v.state.is_empty() && v.country.is_empty())
        || (!v.city.is_empty() && v.state.is_empty())
    {
        return Err(anyhow!("provider_region_invalid"));
    }
    let location = if v.country.is_empty() {
        String::new()
    } else {
        format!(
            "-region-{}{}{}",
            v.country.to_ascii_uppercase(),
            if v.state.is_empty() {
                String::new()
            } else {
                format!("-st-{}", v.state)
            },
            if v.city.is_empty() {
                String::new()
            } else {
                format!("-city-{}", v.city)
            },
        )
    };
    let (host, port) = relay(&c.relay)?;
    let mut username = c
        .username_template
        .replace("{username}", &account.username)
        .replace("{country}", &v.country.to_ascii_lowercase())
        .replace("{state}", &v.state)
        .replace("{city}", &v.city)
        .replace("{location}", &location);
    let session_minutes = v.relay_session_minutes(c)?;
    if session_minutes > 0 {
        // 1024Proxy dynamic residential sessions use -sid-<id>-t-<minutes>.
        username.push_str(&format!("-sid-{session_id:08x}-t-{session_minutes}"));
    }
    let session_type = match v.session_minutes {
        Some(0) => "per_request",
        Some(_) => "sticky",
        None => &c.session_type,
    };
    if username.len() > 255 || account.password.len() > 255 {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    Ok(ProxyEntry {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!(
            "{} {}",
            if c.name.trim().is_empty() {
                "1024Proxy"
            } else {
                c.name.trim()
            },
            if v.country.is_empty() { "Global" } else { &v.country }
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
            ("state".into(), v.state.clone()),
            ("city".into(), v.city.clone()),
            ("folder".into(), c.folder.clone()),
            ("session_type".into(), session_type.into()),
            ("session_minutes".into(), session_minutes.to_string()),
        ]),
        capabilities: crate::space::proxy::ProxyCapabilities {
            socks5: true,
            ..Default::default()
        },
        country: v.country.to_ascii_uppercase(),
        folder: c.folder.clone(),
        notes: String::new(),
        proxy_chain_policy: Default::default(),
    })
}

pub fn generate_many(v: GenerateRequest) -> Result<Vec<ProxyEntry>> {
    let mut file = load_file()?;
    let c = match v.custom_provider_id {
        Some(id) => file
            .custom_providers
            .remove(&id)
            .ok_or_else(|| anyhow!("provider_not_found"))?,
        None => file.defaults,
    };
    if !(1..=1000).contains(&v.count) {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    let count = v.count;
    if count > 1 && !v.custom && v.relay_session_minutes(&c)? == 0 {
        return Err(anyhow!("provider_single_gateway"));
    }
    if count > 1
        && (v.custom || v.custom_provider_id.is_some())
        && c.custom_extract_url.trim().is_empty()
    {
        return Err(anyhow!("provider_single_gateway"));
    }
    let first_session_id = uuid::Uuid::new_v4().as_u128() as u32;
    (0..count)
        .map(|index| generate_entry(&c, &v, first_session_id.wrapping_add(index)))
        .collect()
}

fn custom_extract_url(raw: &str) -> Result<url::Url> {
    let invalid = || anyhow!("provider_invalid_extract_url");
    if !valid_text(raw, 2048) {
        return Err(invalid());
    }
    let url = url::Url::parse(raw).map_err(|_| invalid())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port() == Some(0)
    {
        return Err(invalid());
    }
    Ok(url)
}

pub async fn extract_custom(id: Option<uuid::Uuid>, count: u16) -> Result<Vec<ProxyEntry>> {
    if !(1..=1000).contains(&count) {
        return Err(anyhow!("provider_defaults_invalid"));
    }
    let mut file = load_file()?;
    let provider = match id {
        Some(id) => file.custom_providers.remove(&id).ok_or_else(|| anyhow!("provider_not_found"))?,
        None => file.defaults,
    };
    let url = custom_extract_url(&provider.custom_extract_url)?;
    let kind = match provider.custom_kind.as_str() {
        "http" => ProxyKind::Http,
        "https" => ProxyKind::Https,
        "socks5" => ProxyKind::Socks5,
        _ => return Err(anyhow!("provider_defaults_invalid")),
    };
    let api = decode(provider.custom_api.as_deref())?;
    let api = match (api.username.is_empty(), api.password.is_empty()) {
        (true, true) => None,
        (false, false) => Some(api),
        _ => return Err(anyhow!("tnb_api_credentials_required")),
    };
    let exit = decode(provider.custom_proxy.as_deref())?;
    let body = crate::tnbproxy::fetch_proxy_list(url, api)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let text = std::str::from_utf8(&body).map_err(|_| anyhow!("tnb_invalid_response"))?;
    let mut entries = Vec::new();
    for line in text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if entries.len() == usize::from(count) {
            break;
        }
        let mut entry = crate::tnbproxy::parse_proxy_line(line, kind.clone(), &provider.custom_kind)
            .map_err(|e| anyhow!("{e}"))?;
        apply_custom_metadata(&mut entry, &provider, id);
        if entry.username.is_empty() && entry.password.is_empty() {
            entry.username.clone_from(&exit.username);
            entry.password.clone_from(&exit.password);
        }
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err(anyhow!("tnb_no_proxies"));
    }
    Ok(entries)
}

fn apply_custom_metadata(entry: &mut ProxyEntry, provider: &Stored, id: Option<uuid::Uuid>) {
    entry.provider_id = "custom".into();
    if let Some(id) = id {
        entry.provider_options.insert("custom_provider_id".into(), id.to_string());
    }
    entry.folder = provider.folder.clone();
    entry.country = provider.country.trim().to_ascii_uppercase();
    entry.provider_options.insert("folder".into(), provider.folder.clone());
    entry.provider_options.insert("country".into(), entry.country.clone());
    entry.provider_options.insert("state".into(), provider.state.clone());
    entry.provider_options.insert("city".into(), provider.city.clone());
    entry.provider_options.insert("session_type".into(), provider.session_type.clone());
    entry.provider_options.insert("session_minutes".into(), provider.session_minutes.to_string());
    if !provider.custom_name.trim().is_empty() {
        let provider_name = provider.custom_name.trim();
        entry.name = if entry.name.trim().is_empty() {
            provider_name.to_owned()
        } else {
            format!("{provider_name} · {}", entry.name.trim())
        };
    }
}

fn generate_custom(c: &Stored) -> Result<ProxyEntry> {
    let invalid = || anyhow!("provider_invalid_relay");
    let gateway = url::Url::parse(&c.custom_server).map_err(|_| invalid())?;
    let kind = match c.custom_kind.as_str() {
        "http" => ProxyKind::Http,
        "https" => ProxyKind::Https,
        "socks5" => ProxyKind::Socks5,
        _ => return Err(invalid()),
    };
    // Existing custom presets may point to an API. Only a bare proxy endpoint
    // using the selected transport can be imported as a gateway.
    if gateway.scheme() != c.custom_kind
        || !matches!(gateway.path(), "" | "/")
        || !gateway.username().is_empty()
        || gateway.password().is_some()
        || gateway.query().is_some()
        || gateway.fragment().is_some()
    {
        return Err(invalid());
    }
    let host = gateway.host_str().ok_or_else(invalid)?.to_owned();
    let port = gateway
        .port_or_known_default()
        .filter(|port| *port != 0)
        .ok_or_else(invalid)?;
    let account = decode(c.custom_proxy.as_deref())?;
    let country = c.country.trim().to_uppercase();
    Ok(ProxyEntry {
        id: uuid::Uuid::new_v4().to_string(),
        name: if c.custom_name.trim().is_empty() {
            format!("{host}:{port}")
        } else {
            c.custom_name.trim().to_owned()
        },
        kind,
        host,
        port,
        username: account.username,
        password: account.password,
        provider_id: "custom".into(),
        credential_ref: String::new(),
        provider_options: HashMap::from([
            ("folder".into(), c.folder.clone()),
            ("country".into(), country.clone()),
            ("state".into(), c.state.clone()),
            ("city".into(), c.city.clone()),
            ("session_type".into(), c.session_type.clone()),
            ("session_minutes".into(), c.session_minutes.to_string()),
        ]),
        capabilities: Default::default(),
        country,
        folder: c.folder.clone(),
        notes: String::new(),
        proxy_chain_policy: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_gateway_uses_only_proxy_credentials_and_rejects_api_paths() {
        let mut stored = Stored {
            custom_name: "Custom EU gateway".into(),
            country: "de".into(),
            custom_server: "socks5://[2001:db8::1]:3000".into(),
            custom_kind: "socks5".into(),
            custom_api: seal("api-user".into(), Some("api-secret".into()), None).unwrap(),
            ..Stored::default()
        };
        let entry = generate_custom(&stored).unwrap();
        assert_eq!(entry.name, "Custom EU gateway");
        assert_eq!(entry.kind, ProxyKind::Socks5);
        assert_eq!(entry.country, "DE");
        assert_eq!(entry.provider_options.get("country").map(String::as_str), Some("DE"));
        assert!(entry.username.is_empty() && entry.password.is_empty());
        stored.custom_proxy = seal("proxy-user".into(), Some("proxy-secret".into()), None).unwrap();
        let entry = generate_custom(&stored).unwrap();
        assert_eq!(entry.username, "proxy-user");
        assert_eq!(entry.password, "proxy-secret");
        stored.custom_server.push_str("/gen");
        assert!(generate_custom(&stored).is_err());
    }
    #[test]
    fn custom_extract_metadata_is_uniform_and_keeps_provider_identity() {
        let mut entry = crate::tnbproxy::parse_proxy_line(
            "socks5://node.example:1080",
            ProxyKind::Socks5,
            "socks5",
        )
        .unwrap();
        let provider_id = uuid::Uuid::new_v4();
        let provider = Stored {
            custom_name: "Example provider".into(),
            folder: "Residential/Europe".into(),
            country: "de".into(),
            state: "Bavaria".into(),
            city: "Munich".into(),
            session_type: "sticky".into(),
            session_minutes: 90,
            ..Stored::default()
        };
        apply_custom_metadata(&mut entry, &provider, Some(provider_id));
        assert_eq!(entry.provider_id, "custom");
        assert_eq!(entry.name, "Example provider · node.example:1080");
        assert_eq!(entry.folder, "Residential/Europe");
        assert_eq!(entry.country, "DE");
        assert_eq!(entry.provider_options["custom_provider_id"], provider_id.to_string());
        assert_eq!(entry.provider_options["state"], "Bavaria");
        assert_eq!(entry.provider_options["city"], "Munich");
        assert_eq!(entry.provider_options["session_type"], "sticky");
        assert_eq!(entry.provider_options["session_minutes"], "90");
    }
    #[test]
    fn provider_defaults_are_non_secret_and_tnb_use_us() {
        let v = view(Stored::default()).unwrap();
        assert_eq!(v.tnb_server, "http://usdata.adv.tnbproxy.com:9090");
        assert_eq!(v.session_type, "sticky");
        assert_eq!(v.session_minutes, 180);
        assert_eq!(v.generation_count, 1);
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
            name: "1024Proxy".into(),
            folder: String::new(),
            country: String::new(),
            state: String::new(),
            city: String::new(),
            session_type: default_session_type(),
            session_minutes: default_session_minutes(),
            generation_count: default_generation_count(),
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
            custom_name: "Custom provider".into(),
            custom_server: String::new(),
            custom_extract_url: String::new(),
            custom_plan: String::new(),
            custom_kind: "http".into(),
            custom_website: String::new(),
            custom_whitelist: String::new(),
            custom_api_username: String::new(),
            custom_api_password: None,
            custom_proxy_username: String::new(),
            custom_proxy_password: None,
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
            custom: false,
            custom_provider_id: None,
            country: "EG".into(),
            state: String::new(),
            city: String::new(),
            session_minutes: None,
            count: 1,
        })
        .unwrap();
        assert!(e.username.starts_with("synthetic-user-country-eg-sid-"));
        assert!(e.username.ends_with("-t-120"));
        assert_eq!(e.name, "1024Proxy EG");
        assert_eq!(e.password, "provider-secret");
        assert_eq!(e.provider_options.get("country").map(String::as_str), Some("EG"));
        assert_eq!(e.provider_options.get("session_type").map(String::as_str), Some("sticky"));
        assert_eq!(e.provider_options.get("session_minutes").map(String::as_str), Some("120"));
        let many = generate_many(GenerateRequest {
            custom: false,
            custom_provider_id: None,
            country: "EG".into(),
            state: String::new(),
            city: String::new(),
            session_minutes: None,
            count: 3,
        })
        .unwrap();
        assert_eq!(many.len(), 3);
        assert!(!many[0].same_proxy(&many[1]));
        assert!(!many[1].same_proxy(&many[2]));
        assert!(!many[0].same_proxy(&many[2]));
        for invalid_count in [0, 1001] {
            let invalid = generate_many(GenerateRequest {
                custom: false,
                custom_provider_id: None,
                country: "EG".into(),
                state: String::new(),
                city: String::new(),
                session_minutes: None,
                count: invalid_count,
            });
            assert!(matches!(invalid, Err(error) if error.to_string() == "provider_defaults_invalid"));
        }
        let stored = Stored {
            username_template: "{username}{location}".into(),
            ..load().unwrap()
        };
        let city_entry = generate_entry(&stored, &GenerateRequest {
            custom: false,
            custom_provider_id: None,
            country: "US".into(),
            state: "Louisiana".into(),
            city: "New Orleans".into(),
            session_minutes: None,
            count: 1,
        }, 1).unwrap();
        assert_eq!(city_entry.username, "synthetic-user-region-US-st-Louisiana-city-New Orleans-sid-00000001-t-120");
        assert_eq!(tnb_credentials(true).unwrap().username, "extract-user");
        assert_eq!(tnb_credentials(false).unwrap().username, "exit-user");
        assert!(!serde_json::to_string(&get().unwrap())
            .unwrap()
            .contains("secret"));

        // Exercise migration from the former flat file, including its saved credentials.
        fs::write(path().unwrap(), serde_json::to_vec(&load().unwrap()).unwrap()).unwrap();
        let first = uuid::Uuid::new_v4();
        let second = uuid::Uuid::new_v4();
        let custom = |name: &str, gateway: &str, password: &str| {
            let mut v = update(None);
            v.custom_name = name.into();
            v.custom_server = gateway.into();
            v.custom_proxy_username = name.into();
            v.custom_proxy_password = Some(password.into());
            v
        };
        let request = |id| GenerateRequest {
            custom: true,
            custom_provider_id: Some(id),
            country: String::new(),
            state: String::new(),
            city: String::new(),
            session_minutes: None,
            count: 1,
        };
        save_for(Some(first), true, false, custom("First", "http://first.example:8080", "first-secret")).unwrap();
        save_for(Some(second), true, false, custom("Second", "http://second.example:8081", "second-secret")).unwrap();
        assert_eq!(get().unwrap().custom_providers.len(), 2);
        assert_eq!(generate(request(first)).unwrap().password, "first-secret");
        let entry = generate(request(second)).unwrap();
        assert_eq!(entry.host, "second.example");
        assert_eq!(entry.password, "second-secret");
        assert_eq!(entry.provider_options["custom_provider_id"], second.to_string());
        let static_many = generate_many(GenerateRequest {
            custom: true,
            custom_provider_id: Some(second),
            country: String::new(),
            state: String::new(),
            city: String::new(),
            session_minutes: None,
            count: 2,
        });
        assert!(matches!(static_many, Err(error) if error.to_string() == "provider_single_gateway"));
        assert_eq!(tnb_credentials(false).unwrap().password, "exit-secret");
        let second_view = serde_json::to_string(&get_for(Some(second), false, false).unwrap()).unwrap();
        assert!(!second_view.contains("second-secret"));
        delete_custom(first).unwrap();
        assert_eq!(get().unwrap().custom_providers.len(), 1);
        assert!(generate(request(first)).is_err());
        let stale = save_for(Some(first), false, false, custom("First", "http://first.example:8080", "first-secret"));
        assert!(matches!(stale, Err(error) if error.to_string() == "provider_not_found"));
        assert_eq!(generate(request(second)).unwrap().password, "second-secret");

        let api = uuid::Uuid::new_v4();
        let mut only_api = custom("API only", "", "api-exit-secret");
        only_api.custom_extract_url = "https://feed.example/proxies?format=txt%2Fplain".into();
        only_api.custom_api_username = "api-user".into();
        only_api.custom_api_password = Some("api-basic-secret".into());
        save_for(Some(api), true, false, only_api).unwrap();
        let api_view = get_for(Some(api), false, false).unwrap();
        assert!(api_view.custom_server.is_empty());
        assert_eq!(api_view.custom_extract_url, "https://feed.example/proxies?format=txt%2Fplain");
        assert!(api_view.custom_api_password_saved && api_view.custom_proxy_password_saved);
        let file = load_file().unwrap();
        let stored_api = &file.custom_providers[&api];
        assert_eq!(decode(stored_api.custom_api.as_deref()).unwrap().password, "api-basic-secret");
        assert_eq!(decode(stored_api.custom_proxy.as_deref()).unwrap().password, "api-exit-secret");
        assert_eq!(generate(request(second)).unwrap().password, "second-secret");

        let mut tnb = update(None);
        tnb.country = "EG".into();
        tnb.generation_count = 4;
        save_for(None, false, true, tnb).unwrap();
        let mut relay = update(None);
        relay.country = "US".into();
        relay.generation_count = 2;
        relay.session_minutes = 180;
        save(relay).unwrap();
        let tnb = get_for(None, false, true).unwrap();
        assert_eq!((tnb.country.as_str(), tnb.generation_count), ("EG", 4));
        let relay = get().unwrap();
        assert_eq!((relay.country.as_str(), relay.generation_count), ("US", 2));
        assert_eq!(relay.session_minutes, 120);
    }

    #[test]
    fn custom_extract_url_requires_https_without_url_credentials() {
        let url = custom_extract_url("https://feed.example/proxies?format=txt%2Fplain").unwrap();
        assert_eq!(url.query(), Some("format=txt%2Fplain"));
        for raw in [
            "http://feed.example/proxies",
            "https://user:password@feed.example/proxies",
            "https://feed.example/proxies#fragment",
        ] {
            assert!(custom_extract_url(raw).is_err());
        }
    }
}
