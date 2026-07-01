fn main() {
    // Track embedded agent binaries so Cargo recompiles when they change.
    // include_bytes! only tracks the Rust source file, not the included file.
    println!("cargo:rerun-if-changed=binaries/remote-agent-host-x86_64");
    println!("cargo:rerun-if-changed=binaries/remote-agent-host-aarch64");
    // Stale stub copies at apps/frontend/binaries/ — remove if present
    let old = std::path::Path::new("../binaries");
    if old.exists() {
        eprintln!("build.rs: WARNING stale binaries dir at {:?}, removing", old);
        let _ = std::fs::remove_dir_all(old);
    }

    // Embed dist/manifest.json so startup can compare embedded version
    // against cached version and pick the newer one.
    // build.rs runs with cwd at the crate root (apps/frontend/src-tauri/),
    // so ../../../ reaches the repo root where dist/ lives.
    let manifest_src = std::path::Path::new("../../../dist/manifest.json");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let manifest_dst = std::path::PathBuf::from(&out_dir).join("embedded_manifest.json");
    println!("cargo:rerun-if-changed=../../../dist/manifest.json");

    if manifest_src.exists() {
        std::fs::copy(manifest_src, &manifest_dst).expect("copy manifest to OUT_DIR");
        eprintln!("build.rs: embedded manifest from {}", manifest_src.display());
    } else {
        // Dev build without release.sh — use a sentinel version so cache always wins.
        let default = r#"{"version":"0.0.0.dev","files":{}}"#;
        std::fs::write(&manifest_dst, default).expect("write default manifest");
        eprintln!("build.rs: no dist/manifest.json, using dev sentinel");
    }

    tauri_build::build()
}
