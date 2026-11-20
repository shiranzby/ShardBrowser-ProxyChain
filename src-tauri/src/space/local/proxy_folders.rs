//! Folders kept by the proxy library, separate from profile folders.

use crate::store;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::sync::Mutex;

const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyFolder {
    pub name: String,
    #[serde(default)]
    pub parent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProxyFolderState {
    #[serde(default)]
    pub folders: Vec<ProxyFolder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    version: u32,
    #[serde(flatten)]
    state: ProxyFolderState,
}

static LOCK: Mutex<()> = Mutex::new(());

fn valid_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("a proxy folder needs a name");
    }
    if name.len() > 120 || name.chars().any(|c| c.is_control()) {
        bail!("proxy folder name is invalid");
    }
    Ok(name.to_string())
}

fn read() -> Result<ProxyFolderState> {
    let path = store::proxy_folders_path()?;
    let raw = match fs::read(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ProxyFolderState::default()),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let stored: Stored =
        serde_json::from_slice(&raw).with_context(|| format!("{} is not readable", path.display()))?;
    if stored.version > VERSION {
        bail!("proxy folders were written by a newer launcher");
    }
    Ok(tidy(stored.state))
}

fn write(state: &ProxyFolderState) -> Result<()> {
    let path = store::proxy_folders_path()?;
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(&Stored { version: VERSION, state: state.clone() })?;
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&body)?;
        file.sync_all()?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

fn tidy(mut state: ProxyFolderState) -> ProxyFolderState {
    let mut seen = BTreeMap::new();
    for folder in state.folders.drain(..) {
        if let Ok(name) = valid_name(&folder.name) {
            seen.entry(name.clone()).or_insert(ProxyFolder {
                name,
                parent: folder.parent.trim().to_string(),
            });
        }
    }
    let names: std::collections::BTreeSet<_> = seen.keys().cloned().collect();
    let mut folders: Vec<_> = seen
        .into_values()
        .map(|mut folder| {
            if folder.parent == folder.name || (!folder.parent.is_empty() && !names.contains(&folder.parent)) {
                folder.parent.clear();
            }
            folder
        })
        .collect();
    folders.sort_by(|a, b| a.name.cmp(&b.name));
    state.folders = folders;
    state
}

fn ensure_no_cycle(state: &ProxyFolderState, name: &str, parent: &str) -> Result<()> {
    if parent.is_empty() {
        return Ok(());
    }
    let names: std::collections::BTreeSet<_> = state.folders.iter().map(|f| f.name.as_str()).collect();
    if !names.contains(parent) {
        bail!("the parent proxy folder does not exist");
    }
    let mut current = parent;
    let mut seen = std::collections::BTreeSet::new();
    while !current.is_empty() {
        if current == name || !seen.insert(current) {
            bail!("a proxy folder cannot contain itself or its descendants");
        }
        current = state
            .folders
            .iter()
            .find(|f| f.name == current)
            .map(|f| f.parent.as_str())
            .unwrap_or("");
    }
    Ok(())
}

pub(in crate::space) fn list() -> Result<ProxyFolderState> {
    let mut state = read()?;
    let proxies = super::proxy::list()?;
    let mut changed = false;
    for proxy in proxies {
        let name = proxy.folder.trim();
        if name.is_empty() || state.folders.iter().any(|f| f.name == name) {
            continue;
        }
        state.folders.push(ProxyFolder { name: name.to_string(), parent: String::new() });
        changed = true;
    }
    if changed {
        state = tidy(state);
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        write(&state)?;
    }
    Ok(state)
}

pub(in crate::space) fn add(name: &str, parent: &str) -> Result<ProxyFolderState> {
    let name = valid_name(name)?;
    let parent = parent.trim();
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = read()?;
    if state.folders.iter().any(|f| f.name == name) {
        bail!("a proxy folder with this name already exists");
    }
    ensure_no_cycle(&state, &name, parent)?;
    state.folders.push(ProxyFolder { name, parent: parent.to_string() });
    state = tidy(state);
    write(&state)?;
    Ok(state)
}

pub(in crate::space) fn rename(old: &str, new: &str) -> Result<ProxyFolderState> {
    let old = old.trim();
    let new = valid_name(new)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = read()?;
    if old.is_empty() || !state.folders.iter().any(|f| f.name == old) {
        bail!("proxy folder does not exist");
    }
    if old != new && state.folders.iter().any(|f| f.name == new) {
        bail!("a proxy folder with this name already exists");
    }
    for folder in &mut state.folders {
        if folder.name == old {
            folder.name = new.clone();
        }
        if folder.parent == old {
            folder.parent = new.clone();
        }
    }
    let mut proxies = super::proxy::list()?;
    for proxy in &mut proxies {
        if proxy.folder == old {
            proxy.folder = new.clone();
            super::proxy::upsert(proxy.clone())?;
        }
    }
    state = tidy(state);
    write(&state)?;
    Ok(state)
}

pub(in crate::space) fn delete(name: &str) -> Result<ProxyFolderState> {
    let name = name.trim();
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = read()?;
    if !state.folders.iter().any(|f| f.name == name) {
        bail!("proxy folder does not exist");
    }
    let parent = state.folders.iter().find(|f| f.name == name).map(|f| f.parent.clone()).unwrap_or_default();
    state.folders.retain(|f| f.name != name);
    for folder in &mut state.folders {
        if folder.parent == name {
            folder.parent = parent.clone();
        }
    }
    let mut proxies = super::proxy::list()?;
    for proxy in &mut proxies {
        if proxy.folder == name {
            proxy.folder.clear();
            super::proxy::upsert(proxy.clone())?;
        }
    }
    state = tidy(state);
    write(&state)?;
    Ok(state)
}

pub(in crate::space) fn set_parent(name: &str, parent: &str) -> Result<ProxyFolderState> {
    let name = name.trim();
    let parent = parent.trim();
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = read()?;
    if !state.folders.iter().any(|f| f.name == name) {
        bail!("proxy folder does not exist");
    }
    ensure_no_cycle(&state, name, parent)?;
    let folder = state.folders.iter_mut().find(|f| f.name == name).expect("checked above");
    folder.parent = parent.to_string();
    state = tidy(state);
    write(&state)?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_and_rename_rules_are_stable() {
        let mut state = ProxyFolderState { folders: vec![
            ProxyFolder { name: "US".into(), parent: String::new() },
            ProxyFolder { name: "NY".into(), parent: "US".into() },
        ]};
        assert!(ensure_no_cycle(&state, "US", "NY").is_err());
        state.folders[1].parent = "US".into();
        state.folders[0].name = "United States".into();
        state.folders[1].parent = "United States".into();
        let state = tidy(state);
        assert_eq!(state.folders.len(), 2);
        assert!(valid_name("  ").is_err());
    }

    #[test]
    fn tidy_deduplicates_and_detaches_invalid_parents() {
        let state = tidy(ProxyFolderState { folders: vec![
            ProxyFolder { name: "  Europe ".into(), parent: String::new() },
            ProxyFolder { name: "Europe".into(), parent: String::new() },
            ProxyFolder { name: "London".into(), parent: "Missing".into() },
            ProxyFolder { name: "\u{7f}".into(), parent: String::new() },
        ]});
        assert_eq!(state.folders, vec![
            ProxyFolder { name: "Europe".into(), parent: String::new() },
            ProxyFolder { name: "London".into(), parent: String::new() },
        ]);
    }

    #[test]
    fn parent_must_exist_and_cycles_are_rejected() {
        let state = ProxyFolderState { folders: vec![
            ProxyFolder { name: "Root".into(), parent: String::new() },
            ProxyFolder { name: "Child".into(), parent: "Root".into() },
        ]};
        assert!(ensure_no_cycle(&state, "Leaf", "Missing").is_err());
        assert!(ensure_no_cycle(&state, "Root", "Child").is_err());
        assert!(ensure_no_cycle(&state, "Leaf", "Root").is_ok());
    }
}
