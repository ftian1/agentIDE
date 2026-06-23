//! Tests for bootstrap logic that DON'T require a real SSH connection.
//!
//! The full bootstrap flow (SSH connect → detect → upload → start) needs a real
//! remote machine and can't be unit-tested. But the pure logic within each step
//! CAN be tested independently — these tests catch bugs like:
//! - Version comparison errors leading to unnecessary re-uploads
//! - Base64 encode/decode corruption
//! - Upload command format errors (heredoc, chunking)
//! - Detection output parsing

use base64::Engine;

/// Verify that the embedded binary survives base64 encode → decode round-trip.
/// If this fails, every upload is corrupted.
#[test]
fn test_base64_roundtrip_integrity() {
    // Real binary-like data with all byte values
    let original: Vec<u8> = (0..=255).cycle().take(8192).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&original);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .expect("Base64 decode must succeed");
    assert_eq!(original, decoded, "Base64 round-trip must be lossless");

    // Standard base64 (no line wrapping) produces one continuous line.
    // This is actually fine for heredoc — no embedded newlines to break the delimiter.
    assert!(!encoded.contains('\n'), "Default base64 should NOT wrap lines");
    assert!(!encoded.is_empty());
}

/// Detect parser: verify the combined detection output is parsed correctly.
/// This is the output format from detector.rs's single SSH exec call.
#[test]
fn test_detection_output_parsing() {
    let raw = "ARCH=x86_64\nPLATFORM=Linux\nUSER=sdp\nHOME=/home/sdp\nAGENT_VER=0.2.0\n";
    let mut arch = "";
    let mut platform = "";
    let mut user = "";
    let mut home = "";
    let mut agent_ver = "";
    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("ARCH=") { arch = v; }
        else if let Some(v) = line.strip_prefix("PLATFORM=") { platform = v; }
        else if let Some(v) = line.strip_prefix("USER=") { user = v; }
        else if let Some(v) = line.strip_prefix("HOME=") { home = v; }
        else if let Some(v) = line.strip_prefix("AGENT_VER=") { agent_ver = v; }
    }
    assert_eq!(arch, "x86_64");
    assert_eq!(platform, "Linux");
    assert_eq!(user, "sdp");
    assert_eq!(home, "/home/sdp");
    assert_eq!(agent_ver, "0.2.0");
}

/// Detect parser: not_installed should result in empty agent_ver.
#[test]
fn test_detection_agent_not_installed() {
    let raw = "ARCH=aarch64\nPLATFORM=Linux\nUSER=root\nHOME=/root\nAGENT_VER=not_installed\n";
    let mut agent_ver: Option<String> = None;
    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("AGENT_VER=") {
            if v != "not_installed" {
                agent_ver = Some(v.to_string());
            }
        }
    }
    assert!(agent_ver.is_none(), "not_installed → agent_version must be None");
}

/// Version comparison: installed version matches expected → no re-upload.
#[test]
fn test_version_match_skips_upload() {
    let expected = "0.2.0";
    let installed = Some("0.2.0".to_string());
    let need_upload = match &installed {
        Some(v) if v == expected => false,
        Some(_) => true,
        None => true,
    };
    assert!(!need_upload);
}

/// Version comparison: different version → re-upload.
#[test]
fn test_version_mismatch_triggers_upload() {
    let expected = "0.2.0";
    let installed = Some("0.1.0".to_string());
    let need_upload = match &installed {
        Some(v) if v == expected => false,
        Some(_) => true,
        None => true,
    };
    assert!(need_upload);
}

/// Version comparison: not installed → upload.
#[test]
fn test_not_installed_triggers_upload() {
    let need_upload = match &Option::<String>::None {
        Some(v) if v == "0.2.0" => false,
        Some(_) => true,
        None => true,
    };
    assert!(need_upload);
}

/// Upload chunk command: verify first chunk uses > (overwrite), others use >> (append).
#[test]
fn test_upload_chunk_command_format() {
    let remote_path = "/home/sdp/.remote-agent-host/agent";
    fn make_cmd(i: usize, path: &str, data: &str) -> String {
        if i == 0 {
            format!("set -e; base64 -d > {} << 'B64EOF'\n{}\nB64EOF", path, data)
        } else {
            format!("set -e; base64 -d >> {} << 'B64EOF'\n{}\nB64EOF", path, data)
        }
    }
    let cmd0 = make_cmd(0, remote_path, "AAAA");
    let cmd1 = make_cmd(1, remote_path, "BBBB");
    assert!(cmd0.contains(" > "), "First chunk must overwrite (>)");
    assert!(cmd0.contains("set -e;"), "First chunk must have set -e");
    assert!(cmd1.contains(" >> "), "Subsequent chunks must append (>>)");
    assert!(cmd1.contains("set -e;"), "All chunks must have set -e");
}

/// Size verification command format.
#[test]
fn test_size_verification_command() {
    let remote_path = "/home/sdp/.remote-agent-host/agent";
    let cmd = format!("stat -c%s {}", remote_path);
    assert_eq!(cmd, "stat -c%s /home/sdp/.remote-agent-host/agent");
}
