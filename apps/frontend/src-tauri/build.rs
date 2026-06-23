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
    tauri_build::build()
}
