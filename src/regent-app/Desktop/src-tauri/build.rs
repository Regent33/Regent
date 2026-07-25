fn main() {
    // tauri-build only re-runs this script when tauri.conf.json / capabilities
    // change, so an icon regenerated in place (`tauri icon`, same paths) is NOT
    // re-embedded on an incremental build — the exe keeps a STALE icon resource
    // (its Windows taskbar/shortcut icon). Track the embedded icons and the
    // brand source explicitly so a regeneration always reaches the binary,
    // including the installer's incremental `tauri build --no-bundle`.
    for icon in [
        "app-icon.png",
        "icons/icon.ico",
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
    ] {
        println!("cargo:rerun-if-changed={icon}");
    }
    tauri_build::build();
}
