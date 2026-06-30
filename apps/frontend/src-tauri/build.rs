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
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
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

    tauri_build::build()
}
