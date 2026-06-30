fn main() {
    // Track embedded agent binaries so Cargo recompiles when they change.
    println!("cargo:rerun-if-changed=binaries/remote-agent-host-x86_64");
    println!("cargo:rerun-if-changed=binaries/remote-agent-host-aarch64");
    // Stale stub copies at apps/frontend/binaries/ — remove if present
    let old = std::path::Path::new("../binaries");
    if old.exists() {
        eprintln!("build.rs: WARNING stale binaries dir at {:?}, removing", old);
        let _ = std::fs::remove_dir_all(old);
    }

    // Generate build info (timestamp + git hash) as a Rust source file.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("build_info.rs");
    // Use `date` command for UTC timestamp (fast, no extra crate)
    let timestamp = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();
    std::fs::write(
        &dest,
        format!(
            "pub const BUILD_TIMESTAMP: &str = \"{timestamp}\";\npub const GIT_HASH: &str = \"{git_hash}\";\n"
        ),
    )
    .expect("write build_info.rs");

    tauri_build::build();

    // After tauri_build runs Vite, the output is at ../dist (per tauri.conf.json
    // frontendDist). Additionally, the release script populates ../../../../dist/
    // with all OTA-managed files before the Tauri build. We pack the entire dist/
    // directory into a tarball so the binary can extract ALL components to cache
    // on every startup (frontend.tar.gz, agent binaries, pricing.json, manifest).
    let repo_dist = std::path::Path::new("../../../../dist");
    let tarball_path = std::path::Path::new(&out_dir).join("embedded_dist.tar.gz");
    if repo_dist.exists() && repo_dist.join("frontend.tar.gz").exists() {
        let status = std::process::Command::new("tar")
            .args(["-czf", &tarball_path.to_string_lossy(), "-C", &repo_dist.to_string_lossy(), "."])
            .status()
            .ok();
        if let Some(s) = status {
            if s.success() {
                let size = std::fs::metadata(&tarball_path).map(|m| m.len()).unwrap_or(0);
                println!("cargo:warning=embedded dist tarball: {} bytes from {:?}", size, repo_dist);
            }
        }
    } else {
        // Fallback: pack just the Vite output so compilation doesn't fail
        let vite_dist = std::path::Path::new("../dist");
        if vite_dist.exists() {
            let _ = std::process::Command::new("tar")
                .args(["-czf", &tarball_path.to_string_lossy(), "-C", &vite_dist.to_string_lossy(), "."])
                .status();
        } else {
            std::fs::write(&tarball_path, &[]).ok();
        }
    }
}
