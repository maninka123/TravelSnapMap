use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../native/photos-bridge/Sources");
    println!("cargo:rerun-if-changed=../native/photos-bridge/Info.plist");
    println!("cargo:rerun-if-changed=../scripts/build-bridge.sh");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // Compile the Swift photos-bridge sidecar before Tauri validates `externalBin`.
        // The script is incremental, so this is instant when the Swift sources haven't changed.
        let status = Command::new("sh")
            .arg("../scripts/build-bridge.sh")
            .status()
            .expect("failed to run scripts/build-bridge.sh");
        if !status.success() {
            panic!("Building the Swift photos-bridge failed (see output above)");
        }
    } else {
        // The app only runs on macOS, but the pipeline/database logic and its tests are portable.
        // Tauri insists the sidecar exists, so non-macOS hosts get an empty stand-in that is never run.
        let target = std::env::var("TARGET").unwrap_or_default();
        let ext = if target.contains("windows") { ".exe" } else { "" };
        let stub = std::path::PathBuf::from(format!("binaries/photos-bridge-{target}{ext}"));
        if !stub.exists() {
            std::fs::create_dir_all("binaries").expect("create binaries/");
            std::fs::write(&stub, b"").expect("write photos-bridge stand-in");
        }
        println!("cargo:warning=Not macOS: the Swift photos-bridge was not built (tests only).");
    }

    tauri_build::build()
}
