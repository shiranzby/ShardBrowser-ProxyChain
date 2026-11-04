//! Explicitly opt-in Windows acceptance of the public product launch path.
//! This test never reads the user's profiles. Its only secret input is a
//! current-user DPAPI file outside the checkout. The encrypted JSON has the
//! same fields as the local network probe: host, port, username, password,
//! system_host and system_port. No proxy object, raw error or argv is printed.
//!
//! Run this exact ignored test alone, with `SHARD_PROXY_CHAIN_RUN_LIVE=1`.
//! Optional overrides: SHARD_PROXY_CHAIN_LIVE_CREDENTIALS, _ENGINE,
//! _FINGERPRINT and _REPORT. Both HTTP and SOCKS5 must be served by the
//! explicitly supplied first-hop port. The test uses headless mode and no CDP.

use super::config::ProxyChainSettings;
use super::credentials;
use super::error::ChainError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

type Result<T> = std::result::Result<T, &'static str>;
const SESSION_WAIT: Duration = Duration::from_secs(45);

#[derive(Deserialize)]
struct LiveInput {
    host: String,
    port: u16,
    username: String,
    password: String,
    system_host: String,
    system_port: u16,
}

#[derive(Deserialize)]
struct OwnedProcess {
    pid: u32,
    port: u16,
    has_raw_cdp: bool,
}

#[derive(Default, Serialize)]
struct Report {
    kind: &'static str,
    state: &'static str,
    stage: &'static str,
    error_code: Option<&'static str>,
    engine_file_sha256: String,
    fingerprint_template_sha256: String,
    saved_profiles: usize,
    http_public_launch: bool,
    socks5_public_launch: bool,
    three_independent_live_sessions: bool,
    stopping_one_preserves_two: bool,
    first_hop_failure_no_spawn: bool,
    second_hop_auth_failure_no_spawn: bool,
    second_hop_observed_error_code: Option<&'static str>,
    second_hop_unexpected_spawn: bool,
    second_hop_bound_proxy_password_changed: bool,
    old_settings_default_off: bool,
    invalid_settings_fail_closed: bool,
    saved_proxy_bytes_unchanged: bool,
    fingerprint_payloads_unchanged: bool,
    untouched_profile_data_hashes_unchanged: bool,
    synthetic_cookie_preserved: bool,
    all_test_sessions_stopped: bool,
    isolated_store_removed: bool,
    no_raw_cdp: bool,
    headless_no_foreground: bool,
    browser_exit_ip_verified_by_this_test: bool,
}

fn check(condition: bool, code: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(code)
    }
}

/// Only stable labels cross the diagnostic boundary. Never put an anyhow
/// source, response body, proxy object or rendered command line in the report.
fn launch_error(error: &anyhow::Error) -> &'static str {
    if let Some(error) = error.downcast_ref::<ChainError>() {
        return error.code;
    }
    let label = error.to_string();
    if label.contains("its live exit IP could not be verified") {
        "live_test_exit_verification_unavailable"
    } else if label.contains("start the ShardBrowser engine") {
        "live_test_engine_spawn_failed"
    } else if label.contains("already running") || label.contains("already open in a browser") {
        "live_test_profile_already_open"
    } else if label.contains("moving or resetting") || label.contains("switching spaces") {
        "live_test_store_busy"
    } else {
        "live_test_public_launch_failed"
    }
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|_| "live_test_file_unreadable")?;
    let mut hash = Sha256::new();
    let mut buf = [0; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|_| "live_test_file_unreadable")?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn env_path(name: &str, fallback: PathBuf) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or(fallback)
}

fn input() -> Result<LiveInput> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("live_test_local_app_data_missing")?;
    let path = env_path(
        "SHARD_PROXY_CHAIN_LIVE_CREDENTIALS",
        local.join("ShardBrowser-ProxyChain-Dev/live-test.credentials"),
    );
    let path = path
        .canonicalize()
        .map_err(|_| "live_test_protected_input_missing")?;
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("live_test_workspace_unknown")?
        .canonicalize()
        .map_err(|_| "live_test_workspace_unknown")?;
    check(
        !path.starts_with(workspace),
        "live_test_secret_input_must_be_outside_workspace",
    )?;
    let sealed = std::fs::read(path).map_err(|_| "live_test_protected_input_unreadable")?;
    let clear = credentials::unprotect(&STANDARD.encode(sealed))
        .map_err(|_| "live_test_dpapi_unavailable")?;
    serde_json::from_str(&clear).map_err(|_| "live_test_protected_json_invalid")
}

/// Query only the exact synthetic user-data directory and return numbers and
/// a flag. Command lines (which may contain proxy authentication) stay inside
/// PowerShell and are never returned to the test or written to a log.
fn owned_processes(udd: &Path) -> Result<Vec<OwnedProcess>> {
    let escaped = udd.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"
$wanted=[regex]::Escape('{escaped}');
$rows=@(Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" | Where-Object {{
    $_.CommandLine -match ('--user-data-dir="?' + $wanted + '"?($|\s)') -and $_.CommandLine -notmatch '--type='
}} | ForEach-Object {{
    $m=[regex]::Match($_.CommandLine,'(?i)--proxy-server=[^\s"]*127\.0\.0\.1:(\d+)');
    [pscustomobject]@{{pid=[int]$_.ProcessId;port=$(if($m.Success){{[int]$m.Groups[1].Value}}else{{0}});has_raw_cdp=[bool]($_.CommandLine -match '--remote-debugging-')}}
}});
ConvertTo-Json -InputObject $rows -Compress
"#
    );
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| "live_test_owned_process_query_failed")?;
    check(out.status.success(), "live_test_owned_process_query_failed")?;
    serde_json::from_slice(&out.stdout).map_err(|_| "live_test_owned_process_query_failed")
}

/// A panic/error guard owns only this test's unique directories and processes.
/// It never kills by executable name. The original encrypted input is untouched.
struct Scope {
    root: crate::store::TestRoot,
    ids: Vec<String>,
}
impl Scope {
    fn udd(&self, id: &str) -> PathBuf {
        self.root.path.join("user-data").join(id)
    }
    fn scrub_proxy_file(&self) {
        let _ = std::fs::write(
            self.root.path.join("config/proxies.json"),
            b"{\"proxies\":[]}",
        );
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        // Scrub the only temporary file containing real proxy credentials even
        // if a failed cleanup leaves other engine files locked on Windows.
        self.scrub_proxy_file();
        // Only these three IDs are ever passed to the product launch API.
        for id in self.ids.iter().take(3) {
            if let Ok(processes) = owned_processes(&self.udd(id)) {
                for process in processes {
                    let _ = std::process::Command::new("taskkill.exe")
                        .args(["/F", "/T", "/PID", &process.pid.to_string()])
                        .creation_flags(0x08000000)
                        .output();
                }
            }
        }
        // TestRoot's Drop removes only the unique temporary directory it made.
    }
}

fn write_settings(engine: &Path, config: ProxyChainSettings) -> Result<()> {
    crate::settings::save(&crate::settings::Settings {
        browser_path: Some(engine.to_string_lossy().into_owned()),
        proxy_chain: config,
        extra_args:
            "--headless=new --disable-background-networking --disable-component-update about:blank"
                .into(),
        ext_update_check: false,
        helper_enabled: false,
        camera_enabled: false,
        ..Default::default()
    })
    .map_err(|_| "live_test_settings_write_failed")
}

fn chain(credentials: &LiveInput, mode: &str) -> ProxyChainSettings {
    ProxyChainSettings {
        enabled: true,
        mode: mode.into(),
        host: credentials.system_host.clone(),
        port: credentials.system_port,
        ..Default::default()
    }
}

async fn start(scope: &Scope, id: &str) -> Result<u16> {
    crate::launch::launch_profile(id, false, false)
        .await
        .map_err(|error| launch_error(&error))?;
    let until = Instant::now() + SESSION_WAIT;
    loop {
        check(
            crate::process::Tracker::shared().is_running(id),
            "live_test_engine_exited_early",
        )?;
        if let Some(process) = owned_processes(&scope.udd(id))?.first() {
            check(
                process.port > 0 && !process.has_raw_cdp,
                "live_test_engine_proxy_or_cdp_policy_wrong",
            )?;
            check(
                crate::process::Tracker::shared().is_chained(id),
                "live_test_session_not_chained",
            )?;
            return Ok(process.port);
        }
        check(Instant::now() < until, "live_test_owned_engine_not_found")?;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

async fn stop(scope: &Scope, id: &str) -> Result<()> {
    let until = Instant::now() + SESSION_WAIT;
    loop {
        if !crate::process::Tracker::shared().is_running(id)
            && owned_processes(&scope.udd(id))?.is_empty()
        {
            return Ok(());
        }
        let _ = crate::process::Tracker::shared().kill(id).await;
        check(
            Instant::now() < until,
            "live_test_owned_engine_did_not_stop",
        )?;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

async fn exercise(
    scope: &Scope,
    cfg: &LiveInput,
    engine: &Path,
    template: &Path,
    report: &mut Report,
) -> Result<()> {
    report.stage = "engine_and_template_identity";
    report.engine_file_sha256 = hash_file(engine)?;
    report.fingerprint_template_sha256 = hash_file(template)?;
    let original: serde_json::Value = serde_json::from_slice(
        &std::fs::read(template).map_err(|_| "live_test_fingerprint_unreadable")?,
    )
    .map_err(|_| "live_test_fingerprint_invalid")?;
    check(
        original
            .get("navigator")
            .and_then(|v| v.get("platform"))
            .and_then(|v| v.as_str())
            .is_some_and(|v| v.to_ascii_lowercase().starts_with("win")),
        "live_test_requires_windows_fingerprint",
    )?;

    report.stage = "create_thirty_isolated_profiles";
    let mut proxies = Vec::new();
    let mut payloads = Vec::new();
    let mut preserved = Vec::new();
    for (n, id) in scope.ids.iter().enumerate() {
        let proxy_id = format!("live-synthetic-proxy-{n}");
        proxies.push(json!({"id":proxy_id,"name":format!("Live test {n}"),"kind":"socks5","host":cfg.host,"port":cfg.port,"username":cfg.username,"password":cfg.password}));
        let mut profile = original.clone();
        profile["name"] = json!(format!("Synthetic live test {n}"));
        profile["_meta"] =
            json!({"id":id,"proxy_id":proxy_id,"created_at":"@1","rev":1,"screen_custom":true});
        let profile_path = crate::store::profiles_dir()
            .map_err(|_| "live_test_store_failed")?
            .join(format!("{id}.json"));
        std::fs::write(
            &profile_path,
            serde_json::to_vec(&profile).map_err(|_| "live_test_store_failed")?,
        )
        .map_err(|_| "live_test_store_failed")?;
        // Use the product's normal save before taking the baseline: it turns
        // template noise seed sentinels into this profile's stable values.
        let stored =
            crate::space::profile::load_raw(id).map_err(|_| "live_test_profile_read_failed")?;
        crate::space::profile::save_here(stored, Default::default())
            .map_err(|_| "live_test_store_failed")?;
        payloads.push(
            crate::space::profile::load_raw(id)
                .map_err(|_| "live_test_profile_read_failed")?
                .config,
        );
        if n >= 3 {
            preserved.push((profile_path.clone(), hash_file(&profile_path)?));
            for rel in [
                "Default/Cookies",
                "Default/Local Storage/leveldb/000001.log",
                "Default/IndexedDB/000001.log",
            ] {
                let path = scope.udd(id).join(rel);
                std::fs::create_dir_all(path.parent().ok_or("live_test_store_failed")?)
                    .map_err(|_| "live_test_store_failed")?;
                std::fs::write(&path, format!("synthetic-preserved-{n}-{rel}"))
                    .map_err(|_| "live_test_store_failed")?;
                preserved.push((path.clone(), hash_file(&path)?));
            }
        }
    }
    let proxy_path = crate::store::proxies_path().map_err(|_| "live_test_store_failed")?;
    let proxy_bytes =
        serde_json::to_vec(&json!({"proxies":proxies})).map_err(|_| "live_test_store_failed")?;
    std::fs::write(&proxy_path, &proxy_bytes).map_err(|_| "live_test_store_failed")?;
    report.saved_profiles = crate::space::profile::list_all()
        .map_err(|_| "live_test_profile_read_failed")?
        .len();
    check(report.saved_profiles == 30, "live_test_profile_count_wrong")?;

    report.stage = "old_settings_compatibility";
    // An old settings file has no proxy_chain; no defaults/data-root are reset.
    let settings_path = crate::store::settings_path().map_err(|_| "live_test_store_failed")?;
    std::fs::write(
        &settings_path,
        json!({"browser_path":engine,"theme":"light","extra_args":"--lang=en"}).to_string(),
    )
    .map_err(|_| "live_test_settings_write_failed")?;
    let old = super::saved_settings().map_err(|_| "live_test_old_settings_unreadable")?;
    check(
        !old.proxy_chain.enabled && old.theme == "light" && old.extra_args == "--lang=en",
        "live_test_old_settings_changed",
    )?;
    report.old_settings_default_off = true;
    report.stage = "invalid_settings_fail_closed";
    std::fs::write(
        &settings_path,
        json!({"browser_path":engine,"proxy_chain":{"enabled":true,"port":"invalid"}}).to_string(),
    )
    .map_err(|_| "live_test_settings_write_failed")?;
    let invalid = crate::launch::launch_profile(&scope.ids[0], false, false).await;
    check(
        invalid.is_err()
            && !crate::process::Tracker::shared().is_running(&scope.ids[0])
            && owned_processes(&scope.udd(&scope.ids[0]))?.is_empty(),
        "live_test_invalid_settings_did_not_refuse",
    )?;
    report.invalid_settings_fail_closed = true;

    report.stage = "synthetic_cookie_import";
    // Import a synthetic persistent cookie into the modern engine DB path.
    let cookie_path = scope.udd(&scope.ids[0]).join("Default/Network/Cookies");
    std::fs::create_dir_all(cookie_path.parent().ok_or("live_test_store_failed")?)
        .map_err(|_| "live_test_store_failed")?;
    std::fs::write(&cookie_path, []).map_err(|_| "live_test_store_failed")?;
    let pass = crate::space::perm::cookies(&scope.ids[0])
        .map_err(|_| "live_test_cookie_permission_failed")?;
    crate::space::cookies::import(
        &pass,
        &[crate::space::cookies::Cookie {
            domain: ".example.invalid".into(),
            name: "proxy-chain-live-preservation".into(),
            value: "synthetic-cookie-value".into(),
            path: "/".into(),
            expires: Some(
                (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "live_test_clock_failed")?
                    .as_secs()
                    + 86400) as f64,
            ),
            secure: true,
            http_only: true,
            same_site: Some("Lax".into()),
        }],
    )
    .map_err(|_| "live_test_cookie_import_failed")?;

    for mode in ["http", "socks5"] {
        write_settings(engine, chain(cfg, mode))?;
        report.stage = if mode == "http" {
            "http_single_profile_start"
        } else {
            "socks5_single_profile_start"
        };
        start(scope, &scope.ids[0]).await?;
        report.stage = if mode == "http" {
            "http_single_profile_stop"
        } else {
            "socks5_single_profile_stop"
        };
        stop(scope, &scope.ids[0]).await?;
        if mode == "http" {
            report.http_public_launch = true;
        } else {
            report.socks5_public_launch = true;
        }
    }

    write_settings(engine, chain(cfg, "http"))?;
    let mut ports = HashSet::new();
    for (index, id) in scope.ids[..3].iter().enumerate() {
        report.stage = [
            "three_profiles_first_start",
            "three_profiles_second_start",
            "three_profiles_third_start",
        ][index];
        check(
            ports.insert(start(scope, id).await?),
            "live_test_shared_listener_port",
        )?;
    }
    report.three_independent_live_sessions = ports.len() == 3;
    report.stage = "three_profiles_stop_middle";
    stop(scope, &scope.ids[1]).await?;
    for id in [&scope.ids[0], &scope.ids[2]] {
        check(
            crate::process::Tracker::shared().is_running(id)
                && !owned_processes(&scope.udd(id))?.is_empty(),
            "live_test_stopping_one_affected_another",
        )?;
    }
    report.stopping_one_preserves_two = true;
    report.stage = "three_profiles_stop_remaining";
    stop(scope, &scope.ids[0]).await?;
    stop(scope, &scope.ids[2]).await?;

    report.stage = "first_hop_failure_no_spawn";
    let failing = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "live_test_fixture_bind_failed")?;
    let addr = failing
        .local_addr()
        .map_err(|_| "live_test_fixture_bind_failed")?;
    let refusal = tokio::spawn(async move {
        if let Ok((mut socket, _)) = failing.accept().await {
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") && head.len() < 16384 {
                let Ok(byte) = socket.read_u8().await else {
                    return;
                };
                head.push(byte);
            }
            let _ = socket.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
        }
    });
    write_settings(
        engine,
        ProxyChainSettings {
            enabled: true,
            mode: "http".into(),
            host: addr.ip().to_string(),
            port: addr.port(),
            ..Default::default()
        },
    )?;
    let refused = crate::launch::launch_profile(&scope.ids[0], false, false).await;
    refusal.abort();
    check(
        refused
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<ChainError>())
            .is_some_and(|e| e.code == "system_proxy_connect_refused")
            && !crate::process::Tracker::shared().is_running(&scope.ids[0])
            && owned_processes(&scope.udd(&scope.ids[0]))?.is_empty(),
        "live_test_first_hop_failure_did_not_refuse",
    )?;
    report.first_hop_failure_no_spawn = true;

    report.stage = "second_hop_auth_failure_no_spawn";
    write_settings(engine, chain(cfg, "http"))?;
    let mut wrong = proxies.clone();
    // Some gateways accept a changed password for a recently authenticated
    // account. An account that does not exist is an unambiguous auth rejection.
    wrong[0]["username"] = json!("deliberately-invalid-synthetic-user");
    wrong[0]["password"] = json!("deliberately-invalid-synthetic-password");
    std::fs::write(&proxy_path, json!({"proxies":wrong}).to_string())
        .map_err(|_| "live_test_store_failed")?;
    let bound = crate::launch::proxy_for_test(&scope.ids[0])
        .map_err(|_| "live_test_second_hop_binding_unreadable")?;
    report.second_hop_bound_proxy_password_changed = bound.password != cfg.password
        && bound.password == "deliberately-invalid-synthetic-password";
    drop(bound);
    let rejected = crate::launch::launch_profile(&scope.ids[0], false, false).await;
    report.second_hop_observed_error_code = rejected.as_ref().err().map(launch_error);
    report.second_hop_unexpected_spawn = rejected.is_ok()
        || crate::process::Tracker::shared().is_running(&scope.ids[0])
        || !owned_processes(&scope.udd(&scope.ids[0]))?.is_empty();
    std::fs::write(&proxy_path, &proxy_bytes).map_err(|_| "live_test_store_failed")?;
    check(
        report.second_hop_bound_proxy_password_changed,
        "live_test_second_hop_password_was_not_changed",
    )?;
    check(
        !report.second_hop_unexpected_spawn,
        "live_test_second_hop_unexpected_spawn",
    )?;
    check(
        report.second_hop_observed_error_code == Some("proxy_chain_profile_auth_failed"),
        report
            .second_hop_observed_error_code
            .unwrap_or("live_test_second_hop_failure_did_not_refuse"),
    )?;
    report.second_hop_auth_failure_no_spawn = true;

    report.stage = "saved_proxy_bytes_preserved";
    check(
        std::fs::read(&proxy_path).map_err(|_| "live_test_store_failed")? == proxy_bytes,
        "live_test_original_proxy_changed",
    )?;
    report.saved_proxy_bytes_unchanged = true;
    report.stage = "fingerprint_payloads_preserved";
    for (id, payload) in scope.ids.iter().zip(payloads) {
        let stored =
            crate::space::profile::load_raw(id).map_err(|_| "live_test_profile_read_failed")?;
        check(
            stored.config == payload && stored.meta.id == *id,
            "live_test_fingerprint_payload_changed",
        )?;
    }
    report.fingerprint_payloads_unchanged = true;
    report.stage = "untouched_profile_data_preserved";
    for (path, expected) in preserved {
        check(
            hash_file(&path)? == expected,
            "live_test_untouched_profile_data_changed",
        )?;
    }
    report.untouched_profile_data_hashes_unchanged = true;
    report.stage = "synthetic_cookie_preserved";
    let cookies =
        crate::space::cookies::export(&pass).map_err(|_| "live_test_cookie_export_failed")?;
    check(
        cookies.iter().any(|c| {
            c.name == "proxy-chain-live-preservation" && c.value == "synthetic-cookie-value"
        }),
        "live_test_synthetic_cookie_changed",
    )?;
    report.synthetic_cookie_preserved = true;
    report.stage = "final_template_and_profile_count";
    check(
        hash_file(template)? == report.fingerprint_template_sha256,
        "live_test_source_template_changed",
    )?;
    check(
        crate::space::profile::list_all()
            .map_err(|_| "live_test_profile_read_failed")?
            .len()
            == 30,
        "live_test_profile_count_changed",
    )?;
    report.no_raw_cdp = true;
    report.headless_no_foreground = true;
    report.stage = "completed_exercise";
    Ok(())
}

#[tokio::test]
#[ignore = "Manual Windows acceptance: real system proxy and protected credentials; run this exact test alone."]
async fn real_engine_public_launch_tracker_and_data_acceptance() {
    assert_eq!(
        std::env::var("SHARD_PROXY_CHAIN_RUN_LIVE").as_deref(),
        Ok("1"),
        "live_test_requires_explicit_opt_in"
    );
    let _serial = crate::migrate::serial();
    let root = crate::store::test_root("chain-live-public-launch");
    let root_path = root.path.clone();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let scope = Scope {
        root,
        ids: (0..30)
            .map(|n| format!("live-synthetic-{nonce}-{n:02}"))
            .collect(),
    };
    let mut report = Report {
        kind: "real-engine-public-launch-tracker-acceptance",
        state: "failed",
        stage: "read_protected_input",
        ..Default::default()
    };
    let roaming = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default();
    let engine = env_path(
        "SHARD_PROXY_CHAIN_LIVE_ENGINE",
        roaming.join("shardx-launcher/runtime/ShardX-Windows/chrome.exe"),
    );
    let template = env_path(
        "SHARD_PROXY_CHAIN_LIVE_FINGERPRINT",
        roaming.join("shardx-launcher/fingerprints/win-amd-00001114.json"),
    );
    let result = match input() {
        Ok(input) => match tokio::time::timeout(
            Duration::from_secs(300),
            exercise(&scope, &input, &engine, &template, &mut report),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err("live_test_total_timeout"),
        },
        Err(code) => Err(code),
    };
    let mut clean = true;
    for id in scope.ids.iter().take(3) {
        if crate::process::Tracker::shared().is_running(id) && stop(&scope, id).await.is_err() {
            crate::process::end_browser(&scope.udd(id), None).await;
        }
        clean &= !crate::process::Tracker::shared().is_running(id)
            && owned_processes(&scope.udd(id)).is_ok_and(|v| v.is_empty());
    }
    report.all_test_sessions_stopped = clean;
    scope.scrub_proxy_file();
    drop(scope);
    report.isolated_store_removed = !root_path.exists();
    report.error_code = result.err().or(if clean && report.isolated_store_removed {
        None
    } else {
        Some("live_test_cleanup_incomplete")
    });
    if report.error_code.is_none() {
        report.state = "passed";
        report.stage = "completed_and_cleaned";
    }
    let body =
        serde_json::to_string_pretty(&report).expect("safe_live_report_serialization_failed");
    if let Some(path) = std::env::var_os("SHARD_PROXY_CHAIN_LIVE_REPORT") {
        std::fs::write(path, &body).expect("safe_live_report_write_failed");
    }
    println!("{body}");
    assert!(
        report.error_code.is_none(),
        "real_engine_public_acceptance_failed; see the safe report error_code"
    );
}

#[tokio::test]
#[ignore = "Manual negative authentication check using protected live credentials."]
async fn live_invalid_second_hop_credentials_are_refused() {
    assert_eq!(
        std::env::var("SHARD_PROXY_CHAIN_RUN_LIVE").as_deref(),
        Ok("1")
    );
    let root = crate::store::test_root("chain-live-auth-only");
    let cfg = input().expect("live_test_protected_input_unavailable");
    let entry: crate::space::proxy::ProxyEntry = serde_json::from_value(json!({
        "name":"Synthetic wrong password", "kind":"socks5", "host":cfg.host,
        "port":cfg.port,"username":"deliberately-invalid-synthetic-user","password":"deliberately-invalid-synthetic-password"
    })).expect("synthetic_entry_invalid");
    let prepared = super::prepare(Some(&entry), &chain(&cfg, "http"))
        .await
        .expect("live_test_first_hop_unavailable")
        .expect("chain_enabled");
    let result = crate::space::proxy::probe_on_route(&prepared.effective).await;
    let error_code = result
        .as_ref()
        .err()
        .map(|error| super::profile_failure(&prepared.tunnel, error).code);
    println!(
        "{}",
        json!({"kind":"product-live-negative-auth-probe","rejected":result.is_err(),
        "effective_password_preserved":prepared.effective.password == entry.password,"error_code":error_code})
    );
    prepared.tunnel.shutdown().await;
    assert!(result.is_err(), "live_test_wrong_password_was_accepted");

    let id = format!("live-auth-only-{}", uuid::Uuid::new_v4().simple());
    let scope = Scope {
        root,
        ids: vec![id.clone()],
    };
    let roaming = PathBuf::from(std::env::var_os("APPDATA").expect("roaming_app_data"));
    let engine = roaming.join("shardx-launcher/runtime/ShardX-Windows/chrome.exe");
    let template = roaming.join("shardx-launcher/fingerprints/win-amd-00001114.json");
    let mut profile: serde_json::Value =
        serde_json::from_slice(&std::fs::read(template).expect("template_read"))
            .expect("template_json");
    profile["_meta"] = json!({"id":id,"rev":1,"proxy_id":"wrong-auth-only","screen_custom":true});
    std::fs::write(
        crate::store::profiles_dir()
            .expect("profiles_dir")
            .join(format!("{id}.json")),
        profile.to_string(),
    )
    .expect("profile_save");
    let mut entry = entry;
    entry.id = "wrong-auth-only".into();
    std::fs::write(
        crate::store::proxies_path().expect("proxy_path"),
        json!({"proxies":[entry]}).to_string(),
    )
    .expect("proxy_save");
    write_settings(&engine, chain(&cfg, "http")).expect("settings_save");
    let actual = super::saved_settings().expect("settings_read");
    let rejected = crate::launch::launch_profile(&id, false, false).await;
    let code = rejected.as_ref().err().map(launch_error);
    let spawned = rejected.is_ok();
    if spawned {
        let _ = stop(&scope, &id).await;
    }
    println!(
        "{}",
        json!({"kind":"product-live-negative-auth-public-launch","enabled":actual.proxy_chain.enabled,
        "rejected":rejected.is_err(),"error_code":code,"unexpected_spawn":spawned})
    );
    assert!(
        rejected.is_err(),
        "live_test_public_wrong_password_was_accepted"
    );
}
