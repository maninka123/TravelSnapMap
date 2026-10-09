use std::process::Command;

fn main() {
    // Compile the Swift photos-bridge sidecar before Tauri validates `externalBin`.
    // The script is incremental, so this is instant when the Swift sources haven't changed.
    println!("cargo:rerun-if-changed=../native/photos-bridge/Sources");
    println!("cargo:rerun-if-changed=../native/photos-bridge/Info.plist");
    println!("cargo:rerun-if-changed=../scripts/build-bridge.sh");
    let status = Command::new("sh")
        .arg("../scripts/build-bridge.sh")
        .status()
        .expect("failed to run scripts/build-bridge.sh");
    if !status.success() {
        panic!("Building the Swift photos-bridge failed (see output above)");
    }

    tauri_build::build()
}
