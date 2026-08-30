fn main() {
    // The repository lives on ExFAT, where macOS can create `._*` metadata
    // files. Tauri's default recursive capabilities glob tries to parse those
    // files as JSON. Keep the real capability manifest explicit so AppleDouble
    // files never enter the build input set.
    println!("cargo:rerun-if-changed=capabilities/default.json");
    tauri_build::try_build(
        tauri_build::Attributes::new().capabilities_path_pattern("./capabilities/default.json"),
    )
    .expect("failed to run Tauri build script");
}
