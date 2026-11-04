//! Opt-in acceptance of the public detection entry against real proxies.
//! This is backend network evidence, not native UI or browser-engine evidence.
//! Run this exact ignored test alone with SHARD_PROXY_CHAIN_RUN_LIVE_GEO=1.
//! Credentials use the existing current-user DPAPI input format; overrides:
//! SHARD_PROXY_CHAIN_LIVE_CREDENTIALS and SHARD_PROXY_CHAIN_LIVE_GEO_REPORT.

use super::{config::ProxyChainSettings, credentials, TestResult};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

type Result<T> = std::result::Result<T, &'static str>;

#[derive(Deserialize)]
struct LiveInput {
    host: String,
    port: u16,
    username: String,
    password: String,
    system_host: String,
    system_port: u16,
}

#[derive(Serialize)]
struct Exit {
    exit_ip: String,
    exit_geo: Geo,
    exit_ip_matches_geo_ip: bool,
}

#[derive(Serialize)]
struct Geo {
    ip: String,
    country: String,
    country_code: String,
    region: String,
    city: String,
    provider: String,
}

#[derive(Serialize)]
struct ModeReport {
    first_hop: &'static str,
    system_exit: Option<Exit>,
    profile_exit: Option<Exit>,
    system_and_profile_ips_differ: bool,
}

#[derive(Serialize)]
struct Report {
    kind: &'static str,
    evidence_scope: &'static str,
    state: &'static str,
    stage: &'static str,
    error_code: Option<&'static str>,
    modes: Vec<ModeReport>,
    original_synthetic_files_unchanged: bool,
    isolated_store_removed: bool,
}

fn check(condition: bool, code: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(code)
    }
}

fn workspace() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("live_geo_workspace_unknown")?
        .canonicalize()
        .map_err(|_| "live_geo_workspace_unknown")
}

fn input() -> Result<LiveInput> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("live_geo_local_app_data_missing")?;
    let path = std::env::var_os("SHARD_PROXY_CHAIN_LIVE_CREDENTIALS")
        .map(PathBuf::from)
        .unwrap_or_else(|| local.join("ShardBrowser-ProxyChain-Dev/live-test.credentials"))
        .canonicalize()
        .map_err(|_| "live_geo_protected_input_missing")?;
    check(
        !path.starts_with(workspace()?),
        "live_geo_secret_input_must_be_outside_workspace",
    )?;
    let sealed = std::fs::read(path).map_err(|_| "live_geo_protected_input_unreadable")?;
    let clear = credentials::unprotect(&STANDARD.encode(sealed))
        .map_err(|_| "live_geo_dpapi_unavailable")?;
    serde_json::from_str(&clear).map_err(|_| "live_geo_protected_json_invalid")
}

/// TestRoot owns one fresh directory; the only plaintext proxy file is wiped
/// before its Drop removes that directory, including on panic or cancellation.
struct Scope(crate::store::TestRoot);
impl Drop for Scope {
    fn drop(&mut self) {
        let path = self.0.path.join("config/proxies.json");
        let _ = std::fs::write(&path, b"{\"proxies\":[]}");
        let _ = std::fs::remove_file(path);
    }
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && octets[0] != 0
                && octets[0] < 224
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_multicast()
                && segments[0] & 0xfe00 != 0xfc00
                && segments[0] & 0xffc0 != 0xfe80
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
                && ip
                    .to_ipv4_mapped()
                    .is_none_or(|v4| public_ip(IpAddr::V4(v4)))
        }
    }
}

/// Only an explicit allowlist reaches the report. Never serialize the full
/// TestResult, proxy entry, credentials, provider response, or raw error.
fn verified_exit(result: TestResult, cfg: &LiveInput, profile: bool) -> Result<Exit> {
    check(
        result.system_proxy.state == "ok",
        "live_geo_system_stage_failed",
    )?;
    if profile {
        check(
            result.profile_proxy.state == "ok",
            "live_geo_profile_stage_failed",
        )?;
    }
    check(
        result.exit_error.is_none() && result.exit_error_code.is_none(),
        "live_geo_exit_failed",
    )?;
    let ip = result.exit_ip.ok_or("live_geo_exit_ip_missing")?;
    let geo = result.exit_geo.ok_or("live_geo_exit_location_missing")?;
    check(ip == geo.ip, "live_geo_exit_ip_mismatch")?;
    check(
        ip.parse::<IpAddr>().is_ok_and(public_ip),
        "live_geo_exit_ip_not_public",
    )?;
    check(
        !geo.country.trim().is_empty() && !geo.city.trim().is_empty(),
        "live_geo_country_or_city_missing",
    )?;
    check(
        matches!(
            geo.provider.as_str(),
            "ip-api.com" | "ipapi.co" | "ipwho.is"
        ),
        "live_geo_unexpected_provider",
    )?;
    for field in [
        &ip,
        &geo.country,
        &geo.country_code,
        &geo.region,
        &geo.city,
        &geo.provider,
    ] {
        check(
            [&cfg.username, &cfg.password]
                .iter()
                .filter(|secret| !secret.is_empty())
                .all(|secret| !field.contains(secret.as_str())),
            "live_geo_result_contains_secret",
        )?;
    }
    Ok(Exit {
        exit_ip: ip,
        exit_geo: Geo {
            ip: geo.ip,
            country: geo.country,
            country_code: geo.country_code,
            region: geo.region,
            city: geo.city,
            provider: geo.provider,
        },
        exit_ip_matches_geo_ip: true,
    })
}

async fn exercise(scope: &Scope, cfg: &LiveInput, report: &mut Report) -> Result<()> {
    report.stage = "create_one_synthetic_profile";
    let profile_id = format!("live-geo-{}", uuid::Uuid::new_v4().simple());
    let proxy_id = format!("{profile_id}-proxy");
    let proxy_path = crate::store::proxies_path().map_err(|_| "live_geo_store_unavailable")?;
    let profile_path = crate::store::profiles_dir()
        .map_err(|_| "live_geo_store_unavailable")?
        .join(format!("{profile_id}.json"));
    check(
        proxy_path.starts_with(&scope.0.path) && profile_path.starts_with(&scope.0.path),
        "live_geo_store_isolation_failed",
    )?;
    let proxy_bytes = serde_json::to_vec(&json!({"proxies": [{
        "id": proxy_id, "name": "Synthetic live Geo proxy", "kind": "socks5",
        "host": cfg.host, "port": cfg.port,
        "username": cfg.username, "password": cfg.password
    }]}))
    .map_err(|_| "live_geo_fixture_serialization_failed")?;
    let profile_bytes = serde_json::to_vec(&json!({
        "name": "Synthetic live Geo profile",
        "_meta": {"id": profile_id, "proxy_id": proxy_id, "rev": 1, "created_at": "@1"}
    }))
    .map_err(|_| "live_geo_fixture_serialization_failed")?;
    std::fs::write(&proxy_path, &proxy_bytes).map_err(|_| "live_geo_fixture_write_failed")?;
    std::fs::write(&profile_path, &profile_bytes).map_err(|_| "live_geo_fixture_write_failed")?;

    for mode in ["http", "socks5", "auto"] {
        let config = ProxyChainSettings {
            enabled: true,
            mode: mode.into(),
            host: cfg.system_host.clone(),
            port: cfg.system_port,
            ..Default::default()
        };
        crate::settings::save(&crate::settings::Settings {
            proxy_chain: config.clone(),
            ..Default::default()
        })
        .map_err(|_| "live_geo_settings_write_failed")?;
        let settings_path =
            crate::store::settings_path().map_err(|_| "live_geo_store_unavailable")?;
        let settings_bytes =
            std::fs::read(&settings_path).map_err(|_| "live_geo_settings_read_failed")?;
        report.modes.push(ModeReport {
            first_hop: mode,
            system_exit: None,
            profile_exit: None,
            system_and_profile_ips_differ: false,
        });
        let record = report
            .modes
            .last_mut()
            .ok_or("live_geo_report_state_invalid")?;
        report.stage = match mode {
            "http" => "http_system_public_test",
            "socks5" => "socks5_system_public_test",
            _ => "auto_system_public_test",
        };
        let result = super::test("system".into(), config.clone(), None)
            .await
            .map_err(|_| "live_geo_system_public_entry_failed")?;
        record.system_exit = Some(verified_exit(result, cfg, false)?);

        report.stage = match mode {
            "http" => "http_profile_public_test",
            "socks5" => "socks5_profile_public_test",
            _ => "auto_profile_public_test",
        };
        let result = super::test("profile".into(), config, Some(profile_id.clone()))
            .await
            .map_err(|_| "live_geo_profile_public_entry_failed")?;
        record.profile_exit = Some(verified_exit(result, cfg, true)?);
        record.system_and_profile_ips_differ = record
            .system_exit
            .as_ref()
            .zip(record.profile_exit.as_ref())
            .is_some_and(|(system, profile)| system.exit_ip != profile.exit_ip);
        check(
            record.system_and_profile_ips_differ,
            "live_geo_first_and_second_exit_not_distinct",
        )?;
        check(
            std::fs::read(settings_path).map_err(|_| "live_geo_settings_read_failed")?
                == settings_bytes,
            "live_geo_public_test_modified_settings",
        )?;
    }
    report.stage = "verify_synthetic_store_unchanged";
    report.original_synthetic_files_unchanged =
        std::fs::read(proxy_path).map_err(|_| "live_geo_fixture_read_failed")? == proxy_bytes
            && std::fs::read(profile_path).map_err(|_| "live_geo_fixture_read_failed")?
                == profile_bytes;
    check(
        report.original_synthetic_files_unchanged,
        "live_geo_public_test_modified_binding",
    )
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "Requires explicitly authorized real proxies and current-user DPAPI input; run alone."]
async fn real_network_public_detection_returns_distinct_system_and_profile_geo() {
    assert!(
        std::env::var("SHARD_PROXY_CHAIN_RUN_LIVE_GEO").as_deref() == Ok("1"),
        "live_geo_requires_explicit_opt_in"
    );
    let _serial = crate::migrate::serial();
    let scope = Scope(crate::store::test_root("chain-live-public-geo"));
    let root_path = scope.0.path.clone();
    let mut report = Report {
        kind: "public-proxy-test-live-geo-acceptance",
        evidence_scope: "backend_real_network_not_native_ui_or_browser_engine",
        state: "failed",
        stage: "read_protected_input",
        error_code: None,
        modes: Vec::new(),
        original_synthetic_files_unchanged: false,
        isolated_store_removed: false,
    };
    let result = match input() {
        Ok(cfg) => match tokio::time::timeout(
            Duration::from_secs(180),
            exercise(&scope, &cfg, &mut report),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err("live_geo_total_timeout"),
        },
        Err(code) => Err(code),
    };
    drop(scope);
    report.isolated_store_removed = !root_path.exists();
    report.error_code = result.err().or(if report.isolated_store_removed {
        None
    } else {
        Some("live_geo_cleanup_incomplete")
    });
    if report.error_code.is_none() {
        report.state = "passed";
        report.stage = "completed_and_cleaned";
    }
    let report_path = std::env::var_os("SHARD_PROXY_CHAIN_LIVE_GEO_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            workspace()
                .expect("live_geo_workspace_unknown")
                .join(".audit/live-public-detection-geo-acceptance.json")
        });
    if let Some(parent) = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).expect("safe_live_geo_report_directory_failed");
    }
    let body =
        serde_json::to_string_pretty(&report).expect("safe_live_geo_report_serialization_failed");
    std::fs::write(report_path, &body).expect("safe_live_geo_report_write_failed");
    println!("{body}");
    assert!(
        report.error_code.is_none(),
        "live_geo_acceptance_failed; see safe report error_code"
    );
}
