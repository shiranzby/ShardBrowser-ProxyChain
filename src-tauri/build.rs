fn main() {
    println!("cargo:rerun-if-env-changed=SHARDBROWSER_PATCH_REVISION");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if msvc {
        // Embed one common manifest at link time for both the launcher and
        // Rust unit-test executables. The default resource manifest would
        // otherwise collide with the linker's manifest in a release build.
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new_without_app_manifest(),
        )).expect("failed to build Tauri resources");
    } else {
        tauri_build::build();
    }
    // The launcher manifest covers its main binary; Rust's Windows unit-test
    // executable also links rfd/TaskDialogIndirect and needs Common Controls 6.
    // Without this it fails in the loader before any proxy-chain test can run.
    if msvc {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
