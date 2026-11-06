//! A local, tabs-only bootstrap reuses a restored startup tab without a CDP port.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const ID: &str = "cdaadhlkdbidpoihgpljlkjkhhpccbih";
const KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA6ne1qxnihJzQj7TsDakk/RsEi3xrxA1FmKNi9nNMb402a94ib0EPAC18wyg35gp+iu5qKAgkB1l20G/grEs8u4q+RS7KH4ycVk72c7FeypIrpyYvCEvpgeH/FQ/WNUGLs1wR5F67oQbdStA3t+fr82c/dUl//2/v2QuJzI0SEPV1wwcEaxwcezx1pkzTk1aZcK3Chk3pakNczUs1w6JIh8NGjdNpOFGZ9phfwNg9hb6Rp7P/eKMCWgKeMihMwbdD4dC2Cc28Cv8oNdAEF7ZjXJhTlpvgBxkfcWDt08EQdt4u3X5E2feaWGqIGPKnWccmDgwIU9ARD6rbYsYwt8xHZwIDAQAB";
const HTML: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>ShardBrowser</title></head><body><p id="status">正在检查启动标签… / Checking startup tabs…</p><button id="retry" hidden>重试 / Retry</button><script src="config.js"></script><script type="module" src="start.js"></script></body></html>"#;

pub fn url() -> String { format!("chrome-extension://{ID}/start.html") }

pub fn is_installed(udd: &Path) -> bool { udd.join(".shardbrowser-startup").exists() }

pub fn configured(config: &serde_json::Map<String, serde_json::Value>) -> Option<&str> {
    config.get("startup_url").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty())
}

pub fn install(udd: &Path, target: Option<&str>) -> Result<Option<PathBuf>> {
    let dir = udd.join(".shardbrowser-startup");
    if let Some(target) = target { crate::profile_defaults::validate_startup_url(target)?; }
    if target.is_none() && !dir.exists() { return Ok(None); }
    std::fs::create_dir_all(&dir).context("prepare startup tab helper")?;
    let manifest = serde_json::json!({
        "manifest_version":3,"name":"ShardBrowser startup tab","version":"1.0.0","key":KEY,
        "permissions":["tabs"],"content_security_policy":{"extension_pages":"script-src 'self'; object-src 'none'; connect-src 'none'"}
    });
    // No target URL enters HTML or executable code: JSON is an external data assignment.
    let config = format!("globalThis.SHARD_STARTUP_URL = {};\n", serde_json::to_string(&target)?);
    for (name, content) in [
        ("manifest.json", serde_json::to_string(&manifest)?), ("config.js", config),
        ("start.html", HTML.into()), ("start.js", include_str!("startup_page/start.js").into()),
        ("logic.js", include_str!("startup_page/logic.js").into()),
    ] { std::fs::write(dir.join(name), content).context("write startup tab helper")?; }
    // Command-line extensions are not reloaded from Chromium's preferences.
    // Keep loading an existing helper so a crash-restored entry can retire
    // itself after the preference is cleared; no new entry URL is appended.
    Ok(Some(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_helper_is_local_and_url_is_data_not_html_or_arguments() {
        let root = crate::store::test_root("startup-helper");
        let value = "https://example.com/?q=</script>&next=\"quoted\"#section";
        let path = install(&root.path, Some(value)).unwrap().unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["permissions"], serde_json::json!(["tabs"]));
        assert!(manifest.get("host_permissions").is_none());
        assert!(manifest.get("background").is_none());
        assert!(!std::fs::read_to_string(path.join("start.html")).unwrap().contains(value));
        let data = std::fs::read_to_string(path.join("config.js")).unwrap();
        let value_back: Option<String> = serde_json::from_str(data.trim().strip_prefix("globalThis.SHARD_STARTUP_URL = ").unwrap().trim_end_matches(';')).unwrap();
        assert_eq!(value_back.as_deref(), Some(value));
        assert_eq!(url(), format!("chrome-extension://{ID}/start.html"));
        // Clearing the preference also retires any restored bootstrap from a crash.
        assert_eq!(install(&root.path, None).unwrap(), Some(path.clone()));
        assert!(is_installed(&root.path));
        assert!(std::fs::read_to_string(path.join("config.js")).unwrap().contains("= null;"));
    }
    #[test]
    fn old_profiles_create_no_helper_and_invalid_targets_write_nothing() {
        let root = crate::store::test_root("startup-helper-empty");
        assert!(install(&root.path, None).unwrap().is_none());
        assert!(!root.path.join(".shardbrowser-startup").exists());
        assert!(install(&root.path, Some("javascript:alert(1)")).is_err());
        assert!(!root.path.join(".shardbrowser-startup").exists());
    }
}
