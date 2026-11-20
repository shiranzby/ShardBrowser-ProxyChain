// User-managed Fingerprint Library.
//
// Each entry is a full FingerprintConfig JSON stored under
// `$CONFIG/shardx-launcher/fingerprints/<id>.json`.  The GPU select in
// the profile editor pulls its options from here — i.e. the user can
// curate which GPUs/devices show up by importing their own JSON files
// (or by deleting bundled ones).
//
// On first run we seed the directory with the bundled gpu presets so
// the launcher is usable out of the box.

use crate::store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

/// One row in the library UI; also what the profile editor uses to
/// populate the GPU select.  `payload` is the verbatim FingerprintConfig
/// (without the `_meta` envelope launcher profiles use).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    pub label: String,
    pub platform: String,
    pub chrome: String,
    pub gpu: String,
    pub tag_color: String,
    /// True for the bundled starter set — UI marks them so the user
    /// knows they can re-seed by deleting/reimporting.
    #[serde(default)]
    pub builtin: bool,
    /// Stable content identity for duplicate detection and future sync.
    /// Older library files may omit it and are repaired when listed.
    #[serde(default)]
    pub payload_sha256: String,
    /// User-managed library tags. These stay in the launcher envelope and
    /// are intentionally excluded from fingerprint exports.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Full FingerprintConfig JSON.  Stored inline so the frontend can
    /// preview it and the profile editor can read derived fields
    /// (screen, platform, webgl) without a second round-trip.
    pub payload: Value,
}

/// The library IPC computes these from the current payload. The disk format
/// and exported FingerprintConfig do not include the assessment.
#[derive(Serialize)]
pub struct LibraryView {
    #[serde(flatten)]
    entry: LibraryEntry,
    field_checks: std::collections::BTreeMap<&'static str, FieldCheck>,
    /// Imported payloads are not runtime hardware evidence.
    field_sources: std::collections::BTreeMap<&'static str, FieldSource>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum FieldCheck {
    Missing,
    StructureValid,
    Invalid,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum FieldSource {
    Imported,
    Unavailable,
}

impl From<LibraryEntry> for LibraryView {
    fn from(entry: LibraryEntry) -> Self {
        let fields = ["webgl", "webrtc", "canvas", "fonts", "navigator", "timezone", "screen"];
        let field_checks = fields
            .into_iter()
            .map(|field| {
                let subset = if field == "navigator" {
                    // Only language belongs to this row. An unrelated user agent
                    // or platform error must not mark the language invalid.
                    match entry.payload.get(field) {
                        Some(Value::Object(nav)) => {
                            let language: serde_json::Map<String, Value> = ["language", "languages"]
                                .into_iter()
                                .filter_map(|key| nav.get(key).map(|v| (key.into(), v.clone())))
                                .collect();
                            (!language.is_empty()).then_some(Value::Object(language))
                        }
                        other => other.cloned(),
                    }
                } else {
                    entry.payload.get(field).cloned()
                };
                let state = match subset {
                    None => FieldCheck::Missing,
                    Some(value) => {
                        let payload = Value::Object([(field.into(), value)].into_iter().collect());
                        if validate_known_field_types(&payload).is_ok()
                            && validate_field_consistency(&payload).is_ok()
                        {
                            FieldCheck::StructureValid
                        } else {
                            FieldCheck::Invalid
                        }
                    }
                };
                (field, state)
            })
            .collect();
        let field_sources = fields
            .into_iter()
            .map(|field| {
                let source = if entry.payload.get(field).is_some() {
                    FieldSource::Imported
                } else {
                    FieldSource::Unavailable
                };
                (field, source)
            })
            .collect();
        Self { entry, field_checks, field_sources }
    }
}

fn safe_id(id: &str) -> Result<String> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        anyhow::bail!("invalid fingerprint id");
    }
    Ok(id.to_string())
}

fn path_for(id: &str) -> Result<PathBuf> {
    let id = safe_id(id)?;
    Ok(store::fingerprints_dir()?.join(format!("{id}.json")))
}

/// Seed the library with the bundled GPU presets on the first call
fn tag_color_for(platform: &str) -> String {
    match platform {
        "macOS" => "#8b5cf6".into(),
        "Windows" => "#5dade2".into(),
        "Linux" => "#4ade80".into(),
        // navigator.platform on a phone is "Linux armv8l", so a handset is
        // matched on the prefix rather than on equality.
        p if p.starts_with("Android") || p.starts_with("Linux arm") => "#fb923c".into(),
        _ => "#a78bfa".into(),
    }
}

pub fn list_all() -> Result<Vec<LibraryEntry>> {
    // Pure filesystem read.  Everything in
    //   $CONFIG/shardx-launcher/fingerprints/*.json
    // becomes a library entry, no matter how it got there — UI
    // imports, drag-and-drop, or the user dumping files in by hand.
    // No bundled set, no compile-time tables, no "builtin" concept.
    let dir = store::fingerprints_dir()?;
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if store::is_os_litter(&entry.file_name())
            || entry.path().extension().and_then(|s| s.to_str()) != Some("json")
        {
            continue;
        }
        // One unreadable file must not take the whole library with it.
        let Ok(body) = fs::read_to_string(entry.path()) else {
            continue;
        };
        // The file name is the entry's id: get and delete find it by that
        // name, and a copy made by hand keeps the id written inside it.
        let stem = entry
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from);
        if let Ok(mut e) = serde_json::from_str::<LibraryEntry>(&body) {
            if let Some(stem) = stem {
                e.id = stem;
            }
            if e.payload_sha256.is_empty() {
                e.payload_sha256 = payload_sha256(&e.payload);
            }
            out.push(e);
        } else if let Ok(payload) = serde_json::from_str::<Value>(&body) {
            // Bare FingerprintConfig (no LibraryEntry wrapper) — wrap on
            // the fly so user-imported files that came straight from
            // ShardX still show up.
            let id = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("imported")
                .to_string();
            out.push(wrap_payload(&id, &payload));
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(out)
}

/// Pull a label / platform / chrome / GPU description out of a raw
/// FingerprintConfig.  Used both at import time and when listing
/// bare-JSON files in the library dir.
fn wrap_payload(id: &str, p: &Value) -> LibraryEntry {
    let label = p
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(id)
        .to_string();
    let platform = p
        .get("navigator")
        .and_then(|n| n.get("platform"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let chrome = p
        .get("client_hints")
        .and_then(|c| c.get("brand_version"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let gpu = p
        .get("webgl")
        .and_then(|w| w.get("renderer"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    LibraryEntry {
        id: id.to_string(),
        label,
        platform: platform.clone(),
        chrome,
        gpu,
        tag_color: tag_color_for(&platform),
        builtin: false,
        payload_sha256: payload_sha256(p),
        tags: Vec::new(),
        payload: p.clone(),
    }
}

fn payload_sha256(payload: &Value) -> String {
    let bytes = serde_json::to_vec(payload).expect("serializing a JSON value cannot fail");
    format!("{:x}", Sha256::digest(bytes))
}

/// Reject malformed known fingerprint fields while keeping legacy, partial
/// payloads importable. Missing fields are allowed because older libraries
/// intentionally contain reduced fingerprints.
fn validate_known_field_types(payload: &Value) -> Result<()> {
    let object_fields = [
        "navigator",
        "screen",
        "webgl",
        "client_hints",
        "canvas",
        "fonts",
    ];
    for field in object_fields {
        if let Some(value) = payload.get(field) {
            if !value.is_object() {
                anyhow::bail!("fingerprint field has invalid type: {field}");
            }
        }
    }
    let string_fields = [
        ("navigator", "user_agent"),
        ("navigator", "platform"),
        ("navigator", "language"),
        ("webgl", "vendor"),
        ("webgl", "renderer"),
        ("client_hints", "platform_version"),
    ];
    for (parent, field) in string_fields {
        if let Some(value) = payload
            .get(parent)
            .and_then(Value::as_object)
            .and_then(|object| object.get(field))
        {
            if !value.is_string() {
                anyhow::bail!("fingerprint field has invalid type: {parent}.{field}");
            }
        }
    }
    let number_fields = [
        ("screen", "width"),
        ("screen", "height"),
        ("screen", "avail_width"),
        ("screen", "avail_height"),
        ("screen", "device_pixel_ratio"),
        ("navigator", "hardware_concurrency"),
        ("navigator", "device_memory"),
    ];
    for (parent, field) in number_fields {
        if let Some(value) = payload
            .get(parent)
            .and_then(Value::as_object)
            .and_then(|object| object.get(field))
        {
            if !value.is_number() {
                anyhow::bail!("fingerprint field has invalid type: {parent}.{field}");
            }
        }
    }
    if let Some(value) = payload.get("timezone") {
        if !value.is_string() {
            anyhow::bail!("fingerprint field has invalid type: timezone");
        }
    }
    if let Some(value) = payload.get("webrtc") {
        if !value.is_string() {
            anyhow::bail!("fingerprint field has invalid type: webrtc");
        }
    }
    for field in ["languages", "platform_versions"] {
        if let Some(value) = payload
            .get("navigator")
            .and_then(Value::as_object)
            .and_then(|object| object.get(field))
        {
            let Some(items) = value.as_array() else {
                anyhow::bail!("fingerprint field has invalid type: navigator.{field}");
            };
            if items.iter().any(|item| !item.is_string()) {
                anyhow::bail!("fingerprint field has invalid type: navigator.{field}");
            }
        }
    }
    for field in ["extensions", "extensions_v2"] {
        if let Some(value) = payload
            .get("webgl")
            .and_then(Value::as_object)
            .and_then(|object| object.get(field))
        {
            let Some(items) = value.as_array() else {
                anyhow::bail!("fingerprint field has invalid type: webgl.{field}");
            };
            if items.iter().any(|item| !item.is_string()) {
                anyhow::bail!("fingerprint field has invalid type: webgl.{field}");
            }
        }
    }
    Ok(())
}

/// Reject combinations that are structurally impossible even when every
/// individual value has the right JSON type. These checks do not claim to
/// prove that a fingerprint came from real hardware. They only prevent an
/// import from recording values that contradict each other.
fn validate_field_consistency(payload: &Value) -> Result<()> {
    let screen = payload.get("screen").and_then(Value::as_object);
    let number = |field: &str| {
        screen
            .and_then(|object| object.get(field))
            .and_then(Value::as_f64)
    };
    let positive = |field: &str| {
        if let Some(value) = number(field) {
            if !value.is_finite() || value <= 0.0 {
                anyhow::bail!("fingerprint field must be positive: screen.{field}");
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    for field in ["width", "height", "avail_width", "avail_height", "device_pixel_ratio"] {
        positive(field)?;
    }
    if let (Some(width), Some(avail_width)) = (number("width"), number("avail_width")) {
        if avail_width > width {
            anyhow::bail!("screen.avail_width cannot exceed screen.width");
        }
    }
    if let (Some(height), Some(avail_height)) = (number("height"), number("avail_height")) {
        if avail_height > height {
            anyhow::bail!("screen.avail_height cannot exceed screen.height");
        }
    }

    let navigator = payload.get("navigator").and_then(Value::as_object);
    if let (Some(language), Some(languages)) = (
        navigator.and_then(|object| object.get("language")).and_then(Value::as_str),
        navigator
            .and_then(|object| object.get("languages"))
            .and_then(Value::as_array),
    ) {
        if let Some(first) = languages.first().and_then(Value::as_str) {
            if first != language {
                anyhow::bail!("navigator.languages[0] must match navigator.language");
            }
        }
    }

    Ok(())
}

fn clean_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.split_whitespace().collect::<Vec<_>>().join(" ");
        let tag = tag.chars().take(32).collect::<String>();
        if tag.is_empty() || out.iter().any(|existing| existing.eq_ignore_ascii_case(&tag)) {
            continue;
        }
        out.push(tag);
    }
    out
}

pub fn get(id: &str) -> Result<Option<LibraryEntry>> {
    Ok(list_all()?.into_iter().find(|e| e.id == id))
}

/// Import a raw FingerprintConfig JSON.  Accepts the user's text and
/// returns the saved entry.  If `id` is empty a slug is derived from
/// `payload.name` (or a UUID if no name).
pub fn import(json_text: &str, id_hint: Option<String>) -> Result<LibraryEntry> {
    let payload: Value =
        serde_json::from_str(json_text).context("not a valid JSON FingerprintConfig")?;
    if !payload.is_object() {
        anyhow::bail!("FingerprintConfig must be a JSON object");
    }
    validate_known_field_types(&payload)?;
    validate_field_consistency(&payload)?;
    let digest = payload_sha256(&payload);
    if id_hint.is_none() && list_all()?.iter().any(|entry| entry.payload_sha256 == digest || entry.payload == payload) {
        anyhow::bail!("fingerprint already exists");
    }
    let raw_id = id_hint.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        payload
            .get("name")
            .and_then(|v| v.as_str())
            .map(slugify)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
    });
    let id = ensure_unique_id(&raw_id)?;
    let entry = wrap_payload(&id, &payload);
    let path = path_for(&id)?;
    fs::write(path, serde_json::to_string_pretty(&entry)?)?;
    Ok(entry)
}

/// Export the original FingerprintConfig payload without the launcher-only
/// library envelope. This keeps exports portable between ShyShard instances
/// and compatible with the raw JSON importer.
pub fn export(id: &str) -> Result<String> {
    let entry = get(id)?.ok_or_else(|| anyhow::anyhow!("unknown fingerprint id"))?;
    Ok(serde_json::to_string_pretty(&entry.payload)?)
}

/// Replace an imported fingerprint payload while keeping its library id.
///
/// Profile metadata stores this id in `gpu_preset_id`, so editing in place
/// preserves every existing Profile reference. Library-only tags also remain
/// attached to the edited entry.
pub fn update(id: &str, json_text: &str) -> Result<LibraryEntry> {
    let path = path_for(id)?;
    let body = fs::read_to_string(&path)
        .with_context(|| format!("read fingerprint {}", path.display()))?;
    let mut current: LibraryEntry = serde_json::from_str(&body)
        .with_context(|| format!("{} is not a library entry", path.display()))?;
    if current.builtin {
        anyhow::bail!("builtin fingerprint cannot be edited");
    }
    let payload: Value =
        serde_json::from_str(json_text).context("not a valid JSON FingerprintConfig")?;
    if !payload.is_object() {
        anyhow::bail!("FingerprintConfig must be a JSON object");
    }
    validate_known_field_types(&payload)?;
    validate_field_consistency(&payload)?;
    let digest = payload_sha256(&payload);
    if list_all()?
        .into_iter()
        .any(|entry| entry.id != id && (entry.payload_sha256 == digest || entry.payload == payload))
    {
        anyhow::bail!("fingerprint already exists");
    }
    let mut updated = wrap_payload(id, &payload);
    updated.tags = current.tags;
    updated.builtin = current.builtin;
    current = updated;
    let backup = path.with_extension("json.bak");
    fs::copy(&path, &backup)
        .with_context(|| format!("backup fingerprint {}", path.display()))?;
    let tmp = path.with_extension("json.tmp");
    let serialized = serde_json::to_string_pretty(&current)?;
    if let Err(error) = fs::write(&tmp, serialized) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(current)
}

/// Restore the immediately preceding payload saved by `update`.
pub fn rollback(id: &str) -> Result<LibraryEntry> {
    let path = path_for(id)?;
    let backup = path.with_extension("json.bak");
    let body = fs::read_to_string(&backup)
        .with_context(|| format!("no previous fingerprint version for {}", id))?;
    let restored: LibraryEntry = serde_json::from_str(&body)
        .with_context(|| format!("invalid fingerprint backup for {}", id))?;
    if restored.id != id {
        anyhow::bail!("fingerprint backup id mismatch");
    }
    let tmp = path.with_extension("json.tmp");
    if let Err(error) = fs::write(&tmp, serde_json::to_string_pretty(&restored)?) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    let _ = fs::remove_file(&backup);
    Ok(restored)
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '_' || c == '-' || c == '.' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        trimmed
    }
}

fn ensure_unique_id(base: &str) -> Result<String> {
    let dir = store::fingerprints_dir()?;
    if !dir.join(format!("{base}.json")).exists() {
        return Ok(base.into());
    }
    for n in 2..1000 {
        let cand = format!("{base}-{n}");
        if !dir.join(format!("{cand}.json")).exists() {
            return Ok(cand);
        }
    }
    Ok(format!("{base}-{}", uuid::Uuid::new_v4()))
}

pub fn delete(id: &str) -> Result<()> {
    let path = path_for(id)?;
    let references = profile_references(id)?;
    if !references.is_empty() {
        anyhow::bail!(
            "fingerprint is used by profiles: {}",
            references.join(", ")
        );
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Return local Profile ids that still point at this library fingerprint.
///
/// Read the metadata envelope directly so legacy profiles and fields added by
/// newer engines remain compatible with the deletion guard.
fn profile_references(id: &str) -> Result<Vec<String>> {
    let dir = store::profiles_dir()?;
    let mut references = Vec::new();
    for item in fs::read_dir(dir)? {
        let item = item?;
        if item.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = fs::read_to_string(item.path()) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        let Some(meta) = value.get("_meta").and_then(Value::as_object) else {
            continue;
        };
        if meta.get("gpu_preset_id").and_then(Value::as_str) != Some(id) {
            continue;
        }
        let profile_id = meta
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                item.path()
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned)
            });
        if let Some(profile_id) = profile_id {
            references.push(profile_id);
        }
    }
    references.sort();
    references.dedup();
    Ok(references)
}

/// Save library-only metadata without modifying the original fingerprint
/// payload. Tags are kept in the launcher envelope and are absent from
/// exported FingerprintConfig JSON.
pub fn save_tags(id: &str, tags: Vec<String>) -> Result<LibraryEntry> {
    let path = path_for(id)?;
    let body = fs::read_to_string(&path)
        .with_context(|| format!("read fingerprint {}", path.display()))?;
    let mut entry: LibraryEntry = serde_json::from_str(&body)
        .with_context(|| format!("{} is not a library entry", path.display()))?;
    entry.tags = clean_tags(tags);
    let tmp = path.with_extension("json.tmp");
    let serialized = serde_json::to_string_pretty(&entry)?;
    if let Err(error) = fs::write(&tmp, serialized) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exported_fingerprint_can_be_reimported_without_library_metadata() {
        let _root = store::test_root("fingerprint-export");
        let payload = json!({"name":"Example", "navigator":{"platform":"Win32"}});
        let saved = import(&payload.to_string(), None).unwrap();

        let exported = export(&saved.id).unwrap();
        let restored = import(&exported, Some("restored".into())).unwrap();
        assert_eq!(restored.payload, payload);
        assert!(export("missing").is_err());
    }

    #[test]
    fn import_rejects_non_object_json_before_writing() {
        let _root = store::test_root("fingerprint-invalid");
        for text in ["null", "[]", "\"text\"", "23"] {
            assert!(import(text, Some("invalid".into())).is_err());
        }
        assert!(get("invalid").unwrap().is_none());
    }

    #[test]
    fn import_rejects_invalid_known_field_types_but_keeps_partial_legacy_payloads() {
        let _root = store::test_root("fingerprint-field-types");
        assert!(import(
            &json!({"name":"Bad", "screen":{"width":"1920"}}).to_string(),
            None
        )
        .is_err());
        let partial = import(&json!({"name":"Legacy"}).to_string(), None).unwrap();
        assert_eq!(partial.payload["name"], "Legacy");
    }

    #[test]
    fn import_rejects_invalid_source_fields() {
        let _root = store::test_root("fingerprint-source-fields");
        let invalid = [
            json!({"navigator":{"languages":"en-US"}}),
            json!({"navigator":{"hardware_concurrency":"8"}}),
            json!({"webgl":{"extensions":[1]}}),
            json!({"timezone": false}),
            json!({"webrtc": {"mode":"tcp_only"}}),
        ];
        for payload in invalid {
            assert!(
                import(&payload.to_string(), None).is_err(),
                "expected source field validation to reject {payload}"
            );
        }
    }

    #[test]
    fn library_view_separates_imported_fields_from_runtime_evidence() {
        let _root = store::test_root("fingerprint-field-provenance");
        let entry = import(
            &json!({"name":"Imported", "webgl":{"renderer":"Example GPU"}}).to_string(),
            None,
        )
        .unwrap();
        let view = serde_json::to_value(LibraryView::from(entry)).unwrap();
        assert_eq!(view["field_sources"]["webgl"], "imported");
        assert_eq!(view["field_sources"]["webrtc"], "unavailable");
        assert_eq!(view["field_checks"]["webgl"], "structure_valid");
        assert_eq!(view["field_checks"]["webrtc"], "missing");
    }

    #[test]
    fn import_rejects_inconsistent_screen_language_and_webgl_values() {
        let _root = store::test_root("fingerprint-field-consistency");
        let invalid = [
            json!({"screen":{"width":1280,"avail_width":1440}}),
            json!({"navigator":{"language":"en-US","languages":["zh-CN"]}}),
        ];
        for payload in invalid {
            assert!(
                import(&payload.to_string(), None).is_err(),
                "expected inconsistent fingerprint to be rejected: {payload}"
            );
        }
        let valid = json!({
            "screen": {"width": 1440, "height": 900, "avail_width": 1440, "avail_height": 860, "device_pixel_ratio": 1},
            "navigator": {"language": "en-US", "languages": ["en-US", "en"]},
            "webgl": {"vendor": "Vendor", "renderer": "Renderer"}
        });
        assert!(import(&valid.to_string(), None).is_ok());
    }

    #[test]
    fn import_rejects_duplicate_payload_instead_of_creating_a_suffix() {
        let _root = store::test_root("fingerprint-duplicate");
        let payload = json!({"name":"Example", "navigator":{"platform":"Win32"}});
        let first = import(&payload.to_string(), None).unwrap();
        let duplicate = import(&payload.to_string(), None);
        assert!(duplicate.is_err());
        assert_eq!(list_all().unwrap().iter().filter(|e| e.payload == payload).count(), 1);
        assert_eq!(first.payload_sha256, payload_sha256(&payload));
    }

    #[test]
    fn tags_are_saved_as_library_metadata_and_cleaned() {
        let _root = store::test_root("fingerprint-tags");
        let payload = json!({"name":"Example", "navigator":{"platform":"Win32"}});
        let saved = import(&payload.to_string(), None).unwrap();
        let updated = save_tags(saved.id.as_str(), vec![
            " warm-up ".into(),
            "Warm-UP".into(),
            "needs 2FA".into(),
        ]).unwrap();
        assert_eq!(updated.tags, vec!["warm-up", "needs 2FA"]);
        assert_eq!(updated.payload, payload);
        assert_eq!(get(&saved.id).unwrap().unwrap().tags, updated.tags);
    }

    #[test]
    fn tags_are_deduplicated_after_length_normalization() {
        let _root = store::test_root("fingerprint-tags-normalization");
        let payload = json!({"name":"Example"});
        let saved = import(&payload.to_string(), None).unwrap();
        let first = format!("{}A", "x".repeat(32));
        let second = format!("{}B", "x".repeat(32));
        let updated = save_tags(&saved.id, vec![first, second]).unwrap();
        assert_eq!(updated.tags, vec!["x".repeat(32)]);
    }

    #[test]
    fn update_keeps_id_and_tags_for_profile_references() {
        let _root = store::test_root("fingerprint-update");
        let payload = json!({"name":"Example", "navigator":{"platform":"Win32"}});
        let saved = import(&payload.to_string(), None).unwrap();
        save_tags(&saved.id, vec!["keep".into()]).unwrap();
        let replacement = json!({"name":"Edited", "navigator":{"platform":"Win32"}, "webgl":{"renderer":"GPU"}});
        let updated = update(&saved.id, &replacement.to_string()).unwrap();
        assert_eq!(updated.id, saved.id);
        assert_eq!(updated.tags, vec!["keep"]);
        assert_eq!(updated.payload, replacement);
        assert_eq!(get(&saved.id).unwrap().unwrap().label, "Edited");
    }

    #[test]
    fn update_rejects_duplicate_payload_without_replacing_source() {
        let _root = store::test_root("fingerprint-update-duplicate");
        let first = import(&json!({"name":"First"}).to_string(), None).unwrap();
        let second = import(&json!({"name":"Second"}).to_string(), None).unwrap();
        let result = update(&second.id, &first.payload.to_string());
        assert!(result.is_err());
        assert_eq!(get(&second.id).unwrap().unwrap().label, "Second");
    }

    #[test]
    fn update_can_roll_back_to_the_previous_payload_without_changing_id_or_tags() {
        let _root = store::test_root("fingerprint-rollback");
        let original = json!({"name":"Original", "navigator":{"platform":"Win32"}});
        let saved = import(&original.to_string(), None).unwrap();
        save_tags(&saved.id, vec!["keep".into()]).unwrap();
        let replacement = json!({"name":"Replacement", "navigator":{"platform":"Linux"}});
        update(&saved.id, &replacement.to_string()).unwrap();

        let restored = rollback(&saved.id).unwrap();
        assert_eq!(restored.id, saved.id);
        assert_eq!(restored.tags, vec!["keep"]);
        assert_eq!(restored.payload, original);
        assert!(rollback(&saved.id).is_err());
    }

    #[test]
    fn referenced_fingerprint_cannot_be_deleted() {
        let _root = store::test_root("fingerprint-reference");
        let payload = json!({"name":"Referenced", "navigator":{"platform":"Win32"}});
        let saved = import(&payload.to_string(), None).unwrap();
        let profiles = store::profiles_dir().unwrap();
        fs::write(
            profiles.join("profile-1.json"),
            serde_json::json!({
                "_meta": {
                    "id": "profile-1",
                    "gpu_preset_id": saved.id
                }
            })
            .to_string(),
        )
        .unwrap();

        let error = delete(&saved.id).unwrap_err().to_string();
        assert!(error.contains("profile-1"));
        assert!(path_for(&saved.id).unwrap().exists());
    }
}
