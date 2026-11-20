//! Consistent, inspectable snapshots of the launcher-owned data roots.
//!
//! A snapshot is a directory containing `manifest.json`, `config/` and
//! `data/`.  The manifest stores a SHA-256 for every copied file.  Chromium
//! caches are omitted because they are recreated on the next launch.  DPAPI
//! files are copied as encrypted bytes and are explicitly marked as bound to
//! the Windows user that created them.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FORMAT: u32 = 1;
const MANIFEST: &str = "manifest.json";
const SKIP_DIRS: &[&str] = &[
    "Cache",
    "CacheStorage",
    "Code Cache",
    "GPUCache",
    "Crashpad",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub root: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotManifest {
    pub format: u32,
    pub product: String,
    pub created_at: u64,
    pub config_root: String,
    pub data_root: String,
    pub entries: Vec<SnapshotEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotResult {
    pub path: String,
    pub files: usize,
    pub bytes: u64,
    pub manifest: SnapshotManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotInspection {
    pub path: String,
    pub files: usize,
    pub verified: usize,
    pub missing: usize,
    pub changed: usize,
    pub bytes: u64,
    pub warnings: Vec<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn digest(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut bytes = 0u64;
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        h.update(&buf[..n]);
    }
    Ok((bytes, format!("{:x}", h.finalize())))
}

fn skip_dir(name: &std::ffi::OsStr) -> bool {
    SKIP_DIRS.iter().any(|n| name == *n)
}

fn collect(root: &Path, label: &str, out: &mut Vec<SnapshotEntry>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut stack = vec![PathBuf::from("")];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        for item in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
            let item = item?;
            let name = item.file_name();
            let kind = item.file_type()?;
            let child = if rel.as_os_str().is_empty() {
                PathBuf::from(&name)
            } else {
                rel.join(&name)
            };
            if kind.is_dir() {
                if !skip_dir(&name) {
                    stack.push(child);
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let (bytes, sha256) = digest(&item.path())?;
            out.push(SnapshotEntry {
                root: label.to_string(),
                path: child.to_string_lossy().replace('\\', "/"),
                bytes,
                sha256,
            });
        }
    }
    out.sort_by(|a, b| a.root.cmp(&b.root).then_with(|| a.path.cmp(&b.path)));
    Ok(())
}

fn roots() -> Result<(PathBuf, PathBuf)> {
    Ok((crate::store::config_root()?, crate::store::data_root()?))
}

pub fn preview() -> Result<SnapshotManifest> {
    let (config, data) = roots()?;
    let mut entries = Vec::new();
    collect(&config, "config", &mut entries)?;
    if fs::canonicalize(&config).ok() != fs::canonicalize(&data).ok() {
        collect(&data, "data", &mut entries)?;
    }
    Ok(SnapshotManifest {
        format: FORMAT,
        product: "ShyShard".into(),
        created_at: now(),
        config_root: config.display().to_string(),
        data_root: data.display().to_string(),
        entries,
        warnings: vec![
            "浏览器缓存目录未纳入快照，启动后会自动重建。".into(),
            "DPAPI 凭据按原 Windows 用户加密，换用户或换电脑后需要重新录入。".into(),
        ],
    })
}

fn ensure_destination(destination: &Path, roots: &(PathBuf, PathBuf)) -> Result<()> {
    fs::create_dir_all(destination)?;
    let dest = fs::canonicalize(destination)?;
    for root in [roots.0.as_path(), roots.1.as_path()] {
        if let Ok(root) = fs::canonicalize(root) {
            if dest == root || dest.starts_with(&root) {
                bail!("snapshot destination cannot be inside a ShyShard data root");
            }
        }
    }
    Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst).with_context(|| format!("copy {}", src.display()))?;
    Ok(())
}

fn safe_relative_path(path: &str) -> Result<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("snapshot entry path escapes its root");
    }
    Ok(candidate.to_path_buf())
}

fn copy_tree(root: &Path, label: &str, snapshot: &Path, entries: &[SnapshotEntry]) -> Result<()> {
    for entry in entries.iter().filter(|e| e.root == label) {
        let relative = safe_relative_path(&entry.path)?;
        copy_file(&root.join(&relative), &snapshot.join(label).join(&relative))?;
    }
    Ok(())
}

fn verify_tree(root: &Path, label: &str, snapshot: &Path, entries: &[SnapshotEntry]) -> Result<()> {
    for entry in entries.iter().filter(|e| e.root == label) {
        let relative = safe_relative_path(&entry.path)?;
        let source = root.join(&relative);
        let copied = snapshot.join(label).join(&relative);
        let source_digest =
            digest(&source).with_context(|| format!("verify source {}", source.display()))?;
        let copied_digest =
            digest(&copied).with_context(|| format!("verify snapshot {}", copied.display()))?;
        if source_digest != (entry.bytes, entry.sha256.clone()) {
            bail!("source changed while snapshot was being created");
        }
        if copied_digest != (entry.bytes, entry.sha256.clone()) {
            bail!("snapshot copy did not match its manifest");
        }
    }
    Ok(())
}

pub fn create(destination: &Path) -> Result<SnapshotResult> {
    let roots = roots()?;
    let manifest = preview()?;
    ensure_destination(destination, &roots)?;
    let name = format!("ShyShardSnapshot-{}", manifest.created_at);
    let final_dir = destination.join(&name);
    let temp_dir = destination.join(format!(".{name}.tmp"));
    if final_dir.exists() || temp_dir.exists() {
        bail!("snapshot destination already exists");
    }
    fs::create_dir_all(&temp_dir)?;
    let result = (|| -> Result<()> {
        copy_tree(&roots.0, "config", &temp_dir, &manifest.entries)?;
        copy_tree(&roots.1, "data", &temp_dir, &manifest.entries)?;
        verify_tree(&roots.0, "config", &temp_dir, &manifest.entries)?;
        verify_tree(&roots.1, "data", &temp_dir, &manifest.entries)?;
        let text = serde_json::to_vec_pretty(&manifest)?;
        let mut file = fs::File::create(temp_dir.join(MANIFEST))?;
        file.write_all(&text)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp_dir, &final_dir)
            .with_context(|| format!("finish snapshot {}", final_dir.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temp_dir);
    }
    result?;
    Ok(SnapshotResult {
        path: final_dir.display().to_string(),
        files: manifest.entries.len(),
        bytes: manifest.entries.iter().map(|e| e.bytes).sum(),
        manifest,
    })
}

pub fn inspect(path: &Path) -> Result<SnapshotInspection> {
    let manifest_path = if path.is_dir() {
        path.join(MANIFEST)
    } else {
        path.to_path_buf()
    };
    let root = manifest_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("snapshot has no parent"))?;
    let manifest: SnapshotManifest =
        serde_json::from_slice(&fs::read(&manifest_path).context("read snapshot manifest")?)?;
    if manifest.format > FORMAT {
        bail!("snapshot was created by a newer ShyShard");
    }
    let mut verified = 0;
    let mut missing = 0;
    let mut changed = 0;
    for entry in &manifest.entries {
        if entry.root != "config" && entry.root != "data" {
            bail!("snapshot entry has an unknown root");
        }
        let relative = safe_relative_path(&entry.path)?;
        let file = root.join(&entry.root).join(relative);
        if !file.is_file() {
            missing += 1;
            continue;
        }
        match digest(&file) {
            Ok((bytes, sha)) if bytes == entry.bytes && sha == entry.sha256 => verified += 1,
            Ok(_) => changed += 1,
            Err(_) => changed += 1,
        }
    }
    Ok(SnapshotInspection {
        path: root.display().to_string(),
        files: manifest.entries.len(),
        verified,
        missing,
        changed,
        bytes: manifest.entries.iter().map(|e| e.bytes).sum(),
        warnings: manifest.warnings,
    })
}

/// Read profile payloads for explicit import. The caller decides whether to
/// continue, while `profile_import` creates fresh IDs and never overwrites an
/// existing profile.
pub fn profile_payloads(path: &Path) -> Result<Vec<Value>> {
    let inspection = inspect(path)?;
    if inspection.missing != 0 || inspection.changed != 0 {
        bail!("snapshot integrity check failed before profile import");
    }
    let manifest: SnapshotManifest =
        serde_json::from_slice(&fs::read(path.join(MANIFEST)).context("read snapshot manifest")?)?;
    if manifest.format > FORMAT {
        bail!("snapshot was created by a newer ShyShard");
    }
    let mut payloads = Vec::new();
    for entry in manifest
        .entries
        .iter()
        .filter(|entry| entry.root == "config" || entry.root == "data")
    {
        let relative = safe_relative_path(&entry.path)?;
        if relative.parent() != Some(Path::new("profiles"))
            || relative.extension() != Some(std::ffi::OsStr::new("json"))
        {
            continue;
        }
        let profile_path = path.join(&entry.root).join(&relative);
        let payload: Value = serde_json::from_slice(
            &fs::read(&profile_path)
                .with_context(|| format!("read snapshot profile {}", profile_path.display()))?,
        )
        .with_context(|| format!("parse snapshot profile {}", profile_path.display()))?;
        if !payload.is_object() {
            bail!("snapshot profile is not a JSON object");
        }
        payloads.push(payload);
    }
    Ok(payloads)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_root(name: &str) -> PathBuf {
        let candidate = std::env::temp_dir().join(format!("shyshard-{name}-{}", now()));
        if fs::create_dir_all(&candidate).is_ok() {
            return candidate;
        }
        let local = std::env::current_dir()
            .unwrap()
            .join(".test-roots")
            .join(format!("shyshard-{name}-{}", now()));
        fs::create_dir_all(&local).unwrap();
        local
    }

    #[test]
    fn manifest_entries_are_stable_and_hashes_verify() {
        let base = test_root("snapshot");
        let src = base.join("src");
        let out = base.join("out");
        fs::create_dir_all(src.join("Cache")).unwrap();
        fs::write(src.join("keep.txt"), b"hello").unwrap();
        fs::write(src.join("Cache").join("drop.bin"), b"cache").unwrap();
        let mut entries = Vec::new();
        collect(&src, "config", &mut entries).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "keep.txt");
        assert_eq!(entries[0].bytes, 5);
        fs::create_dir_all(out.join("config")).unwrap();
        copy_tree(&src, "config", &out, &entries).unwrap();
        let (bytes, sha) = digest(&out.join("config/keep.txt")).unwrap();
        assert_eq!((bytes, sha), (entries[0].bytes, entries[0].sha256.clone()));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn inspect_reports_missing_and_changed_files() {
        let base = test_root("inspect");
        let snapshot = base.join("snapshot");
        fs::create_dir_all(snapshot.join("config")).unwrap();
        fs::write(snapshot.join("config/changed.txt"), b"new").unwrap();
        let (bytes, sha) = digest(&snapshot.join("config/changed.txt")).unwrap();
        let manifest = SnapshotManifest {
            format: FORMAT,
            product: "ShyShard".into(),
            created_at: now(),
            config_root: "config".into(),
            data_root: "data".into(),
            entries: vec![
                SnapshotEntry {
                    root: "config".into(),
                    path: "changed.txt".into(),
                    bytes,
                    sha256: sha,
                },
                SnapshotEntry {
                    root: "config".into(),
                    path: "missing.txt".into(),
                    bytes: 1,
                    sha256: "00".repeat(32),
                },
            ],
            warnings: Vec::new(),
        };
        fs::write(
            snapshot.join(MANIFEST),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(snapshot.join("config/changed.txt"), b"old").unwrap();

        let inspection = inspect(&snapshot).unwrap();
        assert_eq!(inspection.verified, 0);
        assert_eq!(inspection.missing, 1);
        assert_eq!(inspection.changed, 1);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn cache_directories_are_excluded_from_manifest() {
        let base = test_root("cache");
        let src = base.join("src");
        fs::create_dir_all(src.join("CacheStorage")).unwrap();
        fs::create_dir_all(src.join("nested/Code Cache")).unwrap();
        fs::write(src.join("keep.txt"), b"keep").unwrap();
        fs::write(src.join("CacheStorage/drop.bin"), b"drop").unwrap();
        fs::write(src.join("nested/Code Cache/drop.bin"), b"drop").unwrap();
        let mut entries = Vec::new();
        collect(&src, "config", &mut entries).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            vec!["keep.txt"]
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn inspect_rejects_manifest_paths_that_escape_root() {
        let base = test_root("unsafe-path");
        let snapshot = base.join("snapshot");
        fs::create_dir_all(&snapshot).unwrap();
        let manifest = SnapshotManifest {
            format: FORMAT,
            product: "ShyShard".into(),
            created_at: now(),
            config_root: "config".into(),
            data_root: "data".into(),
            entries: vec![SnapshotEntry {
                root: "config".into(),
                path: "../outside.txt".into(),
                bytes: 1,
                sha256: "00".repeat(32),
            }],
            warnings: Vec::new(),
        };
        fs::write(
            snapshot.join(MANIFEST),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let error = inspect(&snapshot).unwrap_err().to_string();
        assert!(error.contains("escapes its root"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn profile_payloads_reads_only_profile_json_entries() {
        let base = test_root("profile-payloads");
        let snapshot = base.join("snapshot");
        fs::create_dir_all(snapshot.join("data/profiles")).unwrap();
        fs::create_dir_all(snapshot.join("data/other")).unwrap();
        fs::write(
            snapshot.join("data/profiles/one.json"),
            br#"{"_meta":{"id":"old"},"name":"One"}"#,
        )
        .unwrap();
        fs::write(
            snapshot.join("data/other/not-profile.json"),
            br#"{"name":"Other"}"#,
        )
        .unwrap();
        let (bytes, sha) = digest(&snapshot.join("data/profiles/one.json")).unwrap();
        let (other_bytes, other_sha) =
            digest(&snapshot.join("data/other/not-profile.json")).unwrap();
        let manifest = SnapshotManifest {
            format: FORMAT,
            product: "ShyShard".into(),
            created_at: now(),
            config_root: "config".into(),
            data_root: "data".into(),
            entries: vec![
                SnapshotEntry {
                    root: "data".into(),
                    path: "profiles/one.json".into(),
                    bytes,
                    sha256: sha,
                },
                SnapshotEntry {
                    root: "data".into(),
                    path: "other/not-profile.json".into(),
                    bytes: other_bytes,
                    sha256: other_sha,
                },
            ],
            warnings: Vec::new(),
        };
        fs::write(
            snapshot.join(MANIFEST),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let payloads = profile_payloads(&snapshot).unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["name"], "One");
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn destination_inside_data_root_is_rejected() {
        let base = test_root("destination-boundary");
        let config_root = base.join("config");
        let data_root = base.join("data");
        let destination = data_root.join("snapshots");
        fs::create_dir_all(&config_root).unwrap();
        fs::create_dir_all(&data_root).unwrap();

        let error = ensure_destination(&destination, &(config_root.clone(), data_root.clone()))
            .unwrap_err()
            .to_string();

        assert!(error.contains("inside a ShyShard data root"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn profile_import_rejects_manifest_changed_after_creation() {
        let base = test_root("profile-integrity");
        let snapshot = base.join("snapshot");
        let profile = snapshot.join("data/profiles/one.json");
        fs::create_dir_all(profile.parent().unwrap()).unwrap();
        fs::write(&profile, br#"{"name":"Original"}"#).unwrap();
        let (bytes, sha256) = digest(&profile).unwrap();
        let manifest = SnapshotManifest {
            format: FORMAT,
            product: "ShyShard".into(),
            created_at: now(),
            config_root: "config".into(),
            data_root: "data".into(),
            entries: vec![SnapshotEntry {
                root: "data".into(),
                path: "profiles/one.json".into(),
                bytes,
                sha256,
            }],
            warnings: Vec::new(),
        };
        fs::write(
            snapshot.join(MANIFEST),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let mut tampered = manifest;
        tampered.entries[0].sha256 = "00".repeat(32);
        fs::write(
            snapshot.join(MANIFEST),
            serde_json::to_vec_pretty(&tampered).unwrap(),
        )
        .unwrap();

        let error = profile_payloads(&snapshot).unwrap_err().to_string();
        assert!(error.contains("integrity check failed"));
        let _ = fs::remove_dir_all(base);
    }
}
