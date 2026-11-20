use anyhow::Context;
use anyhow::Result;
use std::path::PathBuf;

/// An adjacent directory opts a Windows launcher into Portable Mode.
/// Cache discovery for the lifetime of the process, even if the drive is removed.
pub(crate) fn root() -> Result<Option<PathBuf>> {
    #[cfg(all(windows, not(test)))]
    {
        use std::sync::OnceLock;
        static ROOT: OnceLock<Result<Option<PathBuf>, String>> = OnceLock::new();
        let root = ROOT.get_or_init(|| {
            std::env::current_exe()
                .map_err(anyhow::Error::from)
                .and_then(|exe| discover_at(&exe))
                .map_err(|error| format!("{error:#}"))
        });
        root.clone().map_err(anyhow::Error::msg)
    }
    #[cfg(not(all(windows, not(test))))]
    Ok(None)
}

#[cfg(any(windows, test))]
fn discover_at(exe: &std::path::Path) -> Result<Option<PathBuf>> {
    let dir = exe.with_file_name("ShyShardData");
    match dir.symlink_metadata() {
        Ok(_) => {
            let path = dir
                .canonicalize()
                .with_context(|| format!("Open {}", dir.display()))?;
            anyhow::ensure!(path.is_dir(), "{} must be a directory", path.display());
            Ok(Some(path))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Open {}", dir.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::discover_at;
    use std::fs;

    #[test]
    fn portable_root_is_discovered_again_beside_the_moved_launcher() {
        let work = crate::store::test_root("portable-move");
        let original = work.path.join("original");
        let moved = work.path.join("moved");
        fs::create_dir(&original).unwrap();
        let exe = original.join("ShyShard.exe");
        assert_eq!(discover_at(&exe).unwrap(), None);
        let data = original.join("ShyShardData");
        fs::write(&data, b"not a directory").unwrap();
        assert!(discover_at(&exe).is_err());
        fs::remove_file(&data).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(data.join("settings.json"), b"{}").unwrap();
        assert_eq!(
            discover_at(&exe).unwrap(),
            Some(data.canonicalize().unwrap())
        );

        fs::rename(&original, &moved).unwrap();
        let data = discover_at(&moved.join("ShyShard.exe")).unwrap().unwrap();
        assert_eq!(data, moved.join("ShyShardData").canonicalize().unwrap());
        assert_eq!(fs::read(data.join("settings.json")).unwrap(), b"{}");
    }
}

/// The installer and each portable root have independent single-instance IDs.
pub(crate) fn configure(
    context: &mut tauri::Context<tauri::Wry>,
) -> Result<Option<tauri::utils::config::WindowConfig>> {
    use sha2::{Digest, Sha256};
    let Some(root) = root()? else { return Ok(None) };
    let webview = root.join("WebView2");
    std::fs::create_dir_all(&webview)?;
    let data = root.join("shardx-launcher");
    std::fs::create_dir_all(&data)?;
    let probe = data.join(format!(".write-check-{}", uuid::Uuid::new_v4()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?
        .sync_all()?;
    std::fs::remove_file(probe)?;
    let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
    let config = context.config_mut();
    config.identifier = format!("{}.portable.{digest:x}", config.identifier);
    // WebView2 reads this for both the main window and dynamically created panels.
    // This runs before the Tauri runtime starts any threads.
    #[cfg(windows)]
    unsafe {
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview);
    }
    // Build this window in the app's setup hook so launch errors can include
    // the portable data location instead of panicking inside Tauri setup.
    let main = config
        .app
        .windows
        .iter_mut()
        .find(|window| window.label == "main")
        .context("Portable launcher is missing its main window configuration")?;
    main.create = false;
    main.data_directory = Some(webview);
    Ok(Some(main.clone()))
}

pub(crate) fn report_start_error(error: &anyhow::Error) {
    let message =
        format!("无法启动便携模式。请检查 ShyShardData 的目录权限，或将便携目录移到可写位置后重试。\n\n{error:#}");
    eprintln!("[launcher] {message}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};
        let text: Vec<u16> = message.encode_utf16().chain([0]).collect();
        let title: Vec<u16> = "ShyShard".encode_utf16().chain([0]).collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_ICONERROR,
            );
        }
    }
}
