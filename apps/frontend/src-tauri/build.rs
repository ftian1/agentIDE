fn main() {
    // Embed dist/manifest.json so startup can compare embedded version
    // against cached version and pick the newer one.
    // Paths are relative to the crate root (apps/frontend/src-tauri/):
    //   ../dist        = apps/frontend/dist/  (Vite output, frontend assets)
    //   ../../../dist  = repo-root dist/      (manifest, frontend.tar.gz, agents)
    let manifest_src = std::path::Path::new("../../../dist/manifest.json");
    let frontend_tar = std::path::Path::new("../../../dist/frontend.tar.gz");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let manifest_dst = std::path::PathBuf::from(&out_dir).join("embedded_manifest.json");

    // ── Cache invalidation: track dist/ outputs that loader.exe depends on ──
    // When these files change (because --frontend-only or --agent-only was run
    // before --tauri-only), cargo re-runs build.rs and re-embeds the assets.
    // This is the mechanism the user described: "如果这些文件时间戳比当前重新build
    // 更新，则使用这些新的文件而不是用缓存" — and it only works if --frontend-only
    // runs first, leaving dist/ files in place for --tauri-only to detect.
    println!("cargo:rerun-if-changed=../../../dist/manifest.json");
    println!("cargo:rerun-if-changed=../../../dist/frontend.tar.gz");

    // Also track the Vite output directory so dev builds (without release.sh)
    // still pick up frontend changes.
    println!("cargo:rerun-if-changed=../dist/index.html");

    // ── Embed manifest ──
    if manifest_src.exists() {
        std::fs::copy(manifest_src, &manifest_dst).expect("copy manifest to OUT_DIR");
        eprintln!(
            "build.rs: embedded manifest {} ({} bytes)",
            manifest_src.display(),
            std::fs::metadata(manifest_src).map(|m| m.len()).unwrap_or(0),
        );
    } else {
        let default = r#"{"version":"0.0.0.dev","files":{}}"#;
        std::fs::write(&manifest_dst, default).expect("write default manifest");
        eprintln!("build.rs: no dist/manifest.json, using dev sentinel");
    }

    // ── Log what we're tracking for debugging ──
    eprintln!(
        "build.rs: frontend tar {} ({})",
        frontend_tar.display(),
        if frontend_tar.exists() { "found" } else { "missing" },
    );

    tauri_build::build()
}
