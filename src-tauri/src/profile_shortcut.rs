//! Desktop links carry only a profile ID. The UI starts through its normal guarded path.
use anyhow::{bail, Context, Result};
use std::sync::Mutex;

#[derive(Clone, Debug, serde::Serialize)]
pub struct ShortcutRequest {
    pub id: String,
    pub silent_launcher: bool,
}

static PENDING: Mutex<Vec<ShortcutRequest>> = Mutex::new(Vec::new());

fn requested_id(args: &[String]) -> Option<String> {
    let mut found = None;
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--profile-id" {
            if found.is_some() {
                return None;
            }
            let id = args.next()?;
            if uuid::Uuid::parse_str(id).is_err() {
                return None;
            }
            found = Some(id.clone());
        }
    }
    found
}

pub fn silent_launcher(args: &[String]) -> bool {
    requested_id(args).is_some() && args.iter().any(|arg| arg == "--silent-launcher")
}

#[tauri::command]
pub fn profile_shortcut_launch_is_silent() -> bool {
    silent_launcher(&std::env::args().collect::<Vec<_>>())
}

pub fn receive(args: &[String]) {
    if let Some(id) = requested_id(args) {
        let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        let request = ShortcutRequest {
            id,
            silent_launcher: silent_launcher(args),
        };
        if !pending.iter().any(|item| item.id == request.id) {
            pending.push(request);
        }
        drop(pending);
        crate::notify_event("profile-shortcut-requested", serde_json::Value::Null);
    }
}

#[tauri::command]
pub fn profile_shortcut_take_pending() -> Vec<ShortcutRequest> {
    std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A silent launch shows its error or password prompt without bypassing guards.
#[tauri::command]
pub fn profile_shortcut_show_launcher(app: tauri::AppHandle) {
    crate::show_main_window(&app);
}

fn safe_name(name: &str) -> String {
    let value: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(70)
        .collect();
    let value = value.trim_matches(|c| c == ' ' || c == '.');
    if value.is_empty() {
        "Profile".into()
    } else {
        value.into()
    }
}

#[tauri::command]
pub async fn profile_create_desktop_shortcut(
    profile_id: String,
    silent: Option<bool>,
) -> Result<String, String> {
    create(&profile_id, silent.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

async fn create(id: &str, silent: bool) -> Result<String> {
    crate::space::ready().await;
    if crate::team::active_team().is_some() {
        bail!(crate::space::PERSONAL_ONLY);
    }
    uuid::Uuid::parse_str(id).context("invalid profile ID")?;
    let name = crate::space::profile::list_all()?
        .into_iter()
        .find(|profile| profile.id == id)
        .context("Profile no longer exists")?
        .name;
    let executable = std::env::current_exe()?;
    let desktop = dirs::desktop_dir().context("Desktop directory is unavailable")?;
    let suffix = if silent { " - silent" } else { "" };
    let path = desktop.join(format!(
        "{} - ShyShard{} - {}.lnk",
        safe_name(&name),
        suffix,
        id
    ));
    #[cfg(windows)]
    {
        write_link(&path, &executable, id, &name, silent)?;
        Ok(path.display().to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = (path, executable);
        bail!("Desktop shortcuts are supported on Windows");
    }
}

#[cfg(windows)]
const WRITE_LINK_PS: &str = r#"$ErrorActionPreference='Stop'; $w=New-Object -ComObject WScript.Shell; $s=$w.CreateShortcut($env:SHARD_LINK); $s.TargetPath=$env:SHARD_EXE; $s.Arguments='--profile-id '+$env:SHARD_PROFILE+$env:SHARD_SILENT; $s.WorkingDirectory=[IO.Path]::GetDirectoryName($env:SHARD_EXE); $s.Description=$env:SHARD_TITLE; $s.IconLocation=$env:SHARD_EXE+',0'; $s.Save()"#;

#[cfg(windows)]
fn write_link(
    path: &std::path::Path,
    executable: &std::path::Path,
    id: &str,
    name: &str,
    silent: bool,
) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // Static script; paths and user names never become PowerShell source.
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WRITE_LINK_PS,
        ])
        .env("SHARD_LINK", path)
        .env("SHARD_EXE", executable)
        .env("SHARD_PROFILE", id)
        .env("SHARD_TITLE", format!("ShyShard: {}", name))
        .env(
            "SHARD_SILENT",
            if silent { " --silent-launcher" } else { "" },
        )
        .creation_flags(0x08000000)
        .output()
        .context("create desktop shortcut")?;
    if !output.status.success() {
        bail!("Windows could not create the desktop shortcut");
    }
    if !path.is_file() {
        bail!("Desktop shortcut was not created");
    }
    Ok(())
}

#[cfg(all(test, windows))]
const READ_LINK_PS: &str = r#"$ErrorActionPreference='Stop'; $OutputEncoding=New-Object System.Text.UTF8Encoding($false); [Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false); $w=New-Object -ComObject WScript.Shell; $s=$w.CreateShortcut($env:SHARD_LINK); @{target=$s.TargetPath; arguments=$s.Arguments; directory=$s.WorkingDirectory; description=$s.Description} | ConvertTo-Json -Compress"#;

#[cfg(test)]
mod tests {
    use super::*;
    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn shortcut_arguments_require_one_real_id() {
        let id = "8ff57c18-945b-4114-9237-b72b4334a014";
        assert_eq!(
            requested_id(&args(&["app.exe", "--profile-id", id])),
            Some(id.into())
        );
        assert!(requested_id(&args(&["app.exe", "--profile-id"])).is_none());
        assert!(requested_id(&args(&["app.exe", "--profile-id", "../bad"])).is_none());
        assert!(
            requested_id(&args(&["app.exe", "--profile-id", id, "--profile-id", id])).is_none()
        );
        assert!(requested_id(&args(&["app.exe", "--reap"])).is_none());
        assert!(silent_launcher(&args(&[
            "app.exe",
            "--profile-id",
            id,
            "--silent-launcher"
        ])));
        assert!(!silent_launcher(&args(&["app.exe", "--profile-id", id])));
        assert!(!silent_launcher(&args(&["app.exe", "--silent-launcher"])));
    }
    #[test]
    fn desktop_name_cannot_escape_desktop() {
        assert_eq!(safe_name("../a:b?\\c"), "_a_b__c");
        assert_eq!(safe_name("...  "), "Profile");
        assert_eq!(safe_name("中文环境"), "中文环境");
    }
    #[cfg(windows)]
    #[test]
    fn native_windows_link_roundtrips_only_profile_id() {
        use std::os::windows::process::CommandExt;
        let root = crate::store::test_root("profile-shortcut-com");
        let link = root.path.join("profile-shortcut.lnk");
        let executable = std::env::current_exe().unwrap();
        let id = "8ff57c18-945b-4114-9237-b72b4334a014";
        // Keep the native COM round-trip fixture ASCII: Windows PowerShell runner output is code-page encoded.
        // Unicode profile names are covered by safe_name() above and by the UI tests.
        let name = "profile test \"quoted\" 'literal' $env:USERNAME";
        write_link(&link, &executable, id, name, false).unwrap();
        // Reopen the link without Save or Run; this does not start the target.
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                READ_LINK_PS,
            ])
            .env("SHARD_LINK", &link)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(output.status.success(), "read link with native Windows COM");
        let read: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            read["target"].as_str().unwrap(),
            executable.to_str().unwrap()
        );
        assert_eq!(read["arguments"], format!("--profile-id {id}"));
        assert_eq!(
            read["directory"].as_str().unwrap(),
            executable.parent().unwrap().to_str().unwrap()
        );
        assert_eq!(read["description"], format!("ShyShard: {name}"));
        // The silent link is read back without executing either target.
        write_link(&link, &executable, id, name, true).unwrap();
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                READ_LINK_PS,
            ])
            .env("SHARD_LINK", &link)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(output.status.success());
        let read: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            read["arguments"],
            format!("--profile-id {id} --silent-launcher")
        );
    }
}
