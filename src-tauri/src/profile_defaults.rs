//! Creation defaults are copied into new profiles; existing profiles never
//! consult them at launch, on import, or when cloned.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileDefaults {
    pub startup_url: String,
    pub language: String,
    pub ui_language: String,
    pub timezone: String,
    pub webrtc: String,
    pub noise_canvas: String,
    pub noise_webgl: String,
    pub noise_audio: String,
    pub noise_client_rects: String,
    pub noise_sensors: String,
    pub noise_fonts: String,
}

impl Default for ProfileDefaults {
    fn default() -> Self {
        Self {
            startup_url: "https://www.browserscan.net/zh/".into(),
            language: "zh-CN".into(),
            ui_language: "profile".into(),
            timezone: "auto".into(),
            webrtc: "auto".into(),
            noise_canvas: "real".into(),
            noise_webgl: "real".into(),
            noise_audio: "real".into(),
            noise_client_rects: "real".into(),
            noise_sensors: "real".into(),
            noise_fonts: "real".into(),
        }
    }
}

impl ProfileDefaults {
    pub fn validate(&self) -> Result<()> {
        validate_startup_url(&self.startup_url)?;
        if !matches!(self.ui_language.as_str(), "system" | "profile") {
            bail!("Invalid default browser UI language");
        }
        if !matches!(self.webrtc.as_str(), "auto" | "tcp_only" | "block") {
            bail!("Invalid default WebRTC mode");
        }
        if self.language.trim().is_empty() || self.timezone.trim().is_empty() {
            bail!("Default language and timezone must not be empty");
        }
        for (_, mode) in self.noise_modes() {
            if !matches!(mode, "real" | "auto") {
                bail!("Invalid default noise mode");
            }
        }
        Ok(())
    }

    fn noise_modes(&self) -> [(&str, &str); 6] {
        [
            ("canvas", &self.noise_canvas), ("webgl", &self.noise_webgl),
            ("audio", &self.noise_audio), ("client_rects", &self.noise_client_rects),
            ("sensors", &self.noise_sensors), ("fonts", &self.noise_fonts),
        ]
    }

    /// Explicit API or form values win, including an empty startup URL.
    pub fn apply_missing(&self, cfg: &mut Map<String, Value>) {
        cfg.entry("startup_url").or_insert_with(|| json!(self.startup_url.trim()));
        cfg.entry("timezone").or_insert_with(|| json!(self.timezone));
        cfg.entry("ui_language").or_insert_with(|| json!(self.ui_language));
        cfg.entry("webrtc").or_insert_with(|| json!(self.webrtc));
        let nav = cfg.entry("navigator").or_insert_with(|| json!({}));
        if let Some(nav) = nav.as_object_mut() {
            if !nav.contains_key("language") {
                nav.insert("language".into(), json!(self.language));
                let language = &self.language;
                let base = language.split('-').next().unwrap_or(language);
                let (accept, languages) = if language == "auto" {
                    (Value::Null, Value::Null)
                } else if language == "en-US" {
                    (json!("en-US,en;q=0.9"), json!(["en-US", "en"]))
                } else {
                    (json!(format!("{language},{base};q=0.9,en-US;q=0.8,en;q=0.7")), json!([language, base, "en-US", "en"]))
                };
                nav.entry("accept_language").or_insert(accept);
                nav.entry("languages").or_insert(languages);
                cfg.entry("icu_locale").or_insert_with(|| if language == "auto" { Value::Null } else { json!(language) });
            }
        }
        let noise = cfg.entry("noise").or_insert_with(|| json!({}));
        if let Some(noise) = noise.as_object_mut() {
            for (key, mode) in self.noise_modes() {
                noise.entry(key).or_insert_with(|| {
                    let enabled = mode == "auto";
                    match key {
                        "webgl" => json!({ "enabled": enabled, "seed": 0, "intensity": if enabled { 0.0005 } else { 0.0 } }),
                        "client_rects" => json!({ "enabled": enabled, "seed": 0, "max_offset": if enabled { 1 } else { 0 } }),
                        _ => json!({ "enabled": enabled, "seed": 0 }),
                    }
                });
            }
        }
    }

    /// Library fingerprints supply hardware, while the creation settings own
    /// these choices. This is called only before a new profile is created.
    pub fn apply_to_template(&self, cfg: &mut Map<String, Value>) {
        for key in ["startup_url", "timezone", "ui_language", "webrtc", "noise", "icu_locale"] {
            cfg.remove(key);
        }
        if let Some(nav) = cfg.get_mut("navigator").and_then(Value::as_object_mut) {
            for key in ["language", "accept_language", "languages"] { nav.remove(key); }
        }
        self.apply_missing(cfg);
    }
}

pub fn validate_startup_url(raw: &str) -> Result<()> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "about:blank" { return Ok(()); }
    if !raw.chars().any(char::is_control) {
        if let Ok(url) = url::Url::parse(raw) {
            if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() { return Ok(()); }
        }
    }
    bail!("Startup URL must be an HTTP or HTTPS URL, about:blank, or empty")
}

pub fn validate_config(cfg: &Map<String, Value>) -> Result<()> {
    if let Some(value) = cfg.get("startup_url") {
        let raw = value.as_str().ok_or_else(|| anyhow::anyhow!("Startup URL must be text"))?;
        validate_startup_url(raw)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_get_the_new_defaults_and_explicit_choices_survive() {
        let defaults: ProfileDefaults = serde_json::from_value(json!({})).unwrap();
        let mut cfg = Map::new();
        defaults.apply_missing(&mut cfg);
        assert_eq!(cfg["startup_url"], "https://www.browserscan.net/zh/");
        assert_eq!(cfg["navigator"]["language"], "zh-CN");
        assert_eq!(cfg["navigator"]["languages"], json!(["zh-CN", "zh", "en-US", "en"]));
        assert_eq!(cfg["icu_locale"], "zh-CN");
        assert_eq!(cfg["ui_language"], "profile");
        let mut explicit = json!({ "startup_url": "", "timezone": "UTC", "ui_language": "system", "webrtc": "block", "navigator": { "language": "auto" }, "noise": { "canvas": { "enabled": true, "seed": 42 } } }).as_object().unwrap().clone();
        defaults.apply_missing(&mut explicit);
        assert_eq!(explicit["startup_url"], "");
        assert_eq!(explicit["timezone"], "UTC");
        assert_eq!(explicit["ui_language"], "system");
        assert_eq!(explicit["webrtc"], "block");
        assert_eq!(explicit["navigator"]["language"], "auto");
        assert_eq!(explicit["noise"]["canvas"]["seed"], 42);
    }

    #[test]
    fn custom_defaults_replace_donor_choices_and_seed_noise_per_profile() {
        let defaults = ProfileDefaults { startup_url: "https://example.com/".into(), language: "en-US".into(), timezone: "UTC".into(), ui_language: "system".into(), webrtc: "tcp_only".into(), noise_canvas: "auto".into(), ..Default::default() };
        let mut cfg = json!({"navigator":{"language":"de-DE","hardware_concurrency":8},"noise":{"canvas":{"enabled":false,"seed":99}}}).as_object().unwrap().clone();
        defaults.apply_to_template(&mut cfg);
        assert_eq!(cfg["startup_url"], "https://example.com/");
        assert_eq!(cfg["navigator"]["language"], "en-US");
        assert_eq!(cfg["navigator"]["hardware_concurrency"], 8);
        assert_eq!(cfg["noise"]["canvas"], json!({"enabled":true,"seed":0}));
        assert_eq!(cfg["webrtc"], "tcp_only");
    }

    #[test]
    fn profile_defaults_keep_explicit_language_lists_when_primary_language_is_missing() {
        let mut cfg = json!({"navigator":{"accept_language":"fr-FR,fr;q=0.9","languages":["fr-FR","fr"]}}).as_object().unwrap().clone();
        ProfileDefaults::default().apply_missing(&mut cfg);
        assert_eq!(cfg["navigator"]["language"], "zh-CN");
        assert_eq!(cfg["navigator"]["accept_language"], "fr-FR,fr;q=0.9");
        assert_eq!(cfg["navigator"]["languages"], json!(["fr-FR", "fr"]));
    }

    #[test]
    fn startup_urls_cannot_be_switches_or_executable_schemes() {
        for url in ["", "about:blank", "https://example.com/?x=1&y=2", "http://localhost:8080/", "https://user:pass@example.com/a b", "\u{85}https://example.com/\u{85}"] { assert!(validate_startup_url(url).is_ok(), "{url}"); }
        for url in ["--proxy-server=x", "javascript:alert(1)", "file:///C:/private", "https://example.com/\n--no-sandbox", "https://example.com/\tpage", "https://example.com/\u{85}page", "example.com"] { assert!(validate_startup_url(url).is_err(), "{url}"); }
    }

    #[tokio::test]
    async fn profile_defaults_do_not_rewrite_existing_imported_or_cloned_profiles() {
        let _serial = crate::migrate::serial();
        let _root = crate::store::test_root("profile-defaults-preserve").with_config();
        let made = crate::save_profile_here(None, json!({"_meta":{"id":""},"name":"New"}), true).unwrap();
        let original = crate::space::profile::load_raw(&made.id).unwrap();
        assert_eq!(original.config["startup_url"], "https://www.browserscan.net/zh/");
        let mut settings = crate::settings::load().unwrap();
        settings.profile_defaults.startup_url = "https://changed.example/".into();
        settings.profile_defaults.language = "de-DE".into();
        crate::settings::save(&settings).unwrap();
        let unchanged = crate::space::profile::load_raw(&made.id).unwrap();
        assert_eq!(original.config, unchanged.config);
        let copied = crate::profile_clone(made.id, None).await.unwrap();
        let copy = crate::space::profile::load_raw(&copied.id).unwrap();
        assert_eq!(copy.config["startup_url"], original.config["startup_url"]);
        assert_eq!(copy.config["navigator"]["language"], "zh-CN");
        let legacy = json!({"name":"Imported legacy","navigator":{"language":"fr-FR"}});
        assert_eq!(crate::profile_import(vec![legacy], None).await.unwrap(), 1);
        let imported = crate::space::profile::list_all().unwrap().into_iter().find(|p| p.name == "Imported legacy").unwrap();
        let imported = crate::space::profile::load_raw(&imported.id).unwrap();
        assert!(!imported.config.contains_key("startup_url"));
        assert!(!imported.config.contains_key("ui_language"));
        assert_eq!(imported.config["navigator"]["language"], "fr-FR");
        let fresh = crate::save_profile_here(None, json!({"_meta":{"id":""},"name":"Next"}), true).unwrap();
        let fresh = crate::space::profile::load_raw(&fresh.id).unwrap();
        assert_eq!(fresh.config["startup_url"], "https://changed.example/");
        assert_eq!(fresh.config["navigator"]["language"], "de-DE");
    }
}
