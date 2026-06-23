//! Live test for Probe and Install APIs — exercising the auto-install code path
//! that the echo/cat tests skip.  This is the gap: the Windows GUI calls
//! SpawnSession { tool: Claude/Copilot } which triggers ensure_tool_installed()
//! → ensure_nodejs() → curl/wget download.
//!
//! Run: SSH_HOST=... SSH_USER=... SSH_PASS=... cargo test -p remote-ai-ide --test live_probe_test -- --nocapture

use std::env;
use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::ToolKind;
use remote_ai_ide_lib::transport::Transport;
use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};

fn env_or_skip(k: &str) -> String {
    env::var(k).unwrap_or_else(|_| panic!("SKIP: {} not set", k))
}

/// Test ProbeRequest — verifies the agent can report tool availability.
/// This is a non-destructive test that doesn't trigger any install.
#[test]
fn test_probe_tools() {
    let host = env_or_skip("SSH_HOST");
    let port: u16 = env::var("SSH_PORT").unwrap_or_else(|_| "22".into()).parse().unwrap();
    let user = env_or_skip("SSH_USER");
    let pass = env_or_skip("SSH_PASS");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let session = ssh::connect(&SshConnectionParams {
            host, port, user: user.clone(),
            auth: AuthMethod::Password(pass),
        }).await.expect("SSH connect");
        eprintln!("✓ connected");

        let info = remote_ai_ide_lib::bootstrap::detector::detect(&session).await.expect("detect");
        eprintln!("✓ detect: arch={} agent={:?}", info.arch, info.agent_version);

        // Force upload — we need the updated agent binary
        let bin = remote_ai_ide_lib::bootstrap::uploader::get_embedded(&info.arch)
            .expect("no binary for arch");
        eprintln!("uploading {} KB (force)...", bin.data.len() / 1024);
        remote_ai_ide_lib::bootstrap::uploader::upload_agent(&session, &bin, &info.home_dir)
            .await.expect("upload");
        eprintln!("✓ uploaded");

        let transport = remote_ai_ide_lib::bootstrap::uploader::start_agent(&session, &info.home_dir)
            .await.expect("start agent");
        eprintln!("✓ agent started");

        // Drain Hello
        loop {
            match transport.recv().await.expect("recv") {
                Some(ProtocolMessage::Hello { .. }) => { eprintln!("✓ hello"); break; }
                None => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
                _ => {}
            }
        }

        // ── Test Probe API ──────────────────────────────
        for tool in &[ToolKind::Claude, ToolKind::Copilot, ToolKind::Custom("echo".into())] {
            transport.send(ProtocolMessage::ProbeRequest { tool: tool.clone() }).await
                .expect("send probe");
            eprintln!("  probe sent: {:?}", tool);

            // Wait for ProbeResponse
            loop {
                match transport.recv().await.expect("recv") {
                    Some(ProtocolMessage::ProbeResponse { tool: t, installed, version, path, .. }) => {
                        eprintln!("  ✓ probe: {:?} installed={} version={:?} path={:?}",
                            t, installed, version, path);
                        break;
                    }
                    Some(other) => { eprintln!("  other: {}", other.kind()); }
                    None => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                }
            }
        }

        // ── Test Install API (may fail on this server due to disk space) ──
        // We test Claude install — it will trigger ensure_nodejs → curl download
        transport.send(ProtocolMessage::InstallRequest {
            tool: ToolKind::Claude,
            version: None,
        }).await.expect("send install");
        eprintln!("  install sent: Claude");

        // Wait for InstallComplete (success or failure, both are valid test outcomes)
        loop {
            match transport.recv().await.expect("recv") {
                Some(ProtocolMessage::InstallComplete { tool, success, version, error }) => {
                    if success {
                        eprintln!("  ✓ install SUCCESS: {:?} v{:?}", tool, version);
                    } else {
                        // This is expected on a server with full /tmp
                        // But the error should now include the real reason (disk full etc.)
                        eprintln!("  ⚠ install FAILED (expected on constrained server): {:?}", error);
                        // Verify the error message has the "Last download error:" prefix
                        if let Some(ref e) = error {
                            assert!(
                                e.contains("Last download error:")
                                || e.contains("Last package manager error:")
                                || e.contains("No install strategies"),
                                "Error should surface the real failure reason, got: {}",
                                e
                            );
                            eprintln!("  ✓ error includes actual failure reason ✓");
                        }
                    }
                    break;
                }
                Some(other) => { eprintln!("  other: {}", other.kind()); }
                None => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }

        eprintln!("\n✓ All probe/install API tests passed");
    });
}

/// Test remote file listing — uses the SSH session directly.
#[test]
fn test_list_files() {
    let host = env_or_skip("SSH_HOST");
    let port: u16 = env::var("SSH_PORT").unwrap_or_else(|_| "22".into()).parse().unwrap();
    let user = env_or_skip("SSH_USER");
    let pass = env_or_skip("SSH_PASS");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};

        let session = ssh::connect(&SshConnectionParams {
            host, port, user: user.clone(),
            auth: AuthMethod::Password(pass),
        }).await.expect("SSH connect");
        eprintln!("✓ connected");

        // Test listing home directory
        let home = format!("/home/{}", user);
        let cmd = format!(
            r#"cd "{}" 2>/dev/null || exit 1; for f in * .*; do [ "$f" = "." ] && continue; [ "$f" = ".." ] && continue; [ -e "$f" ] || continue; if [ -d "$f" ]; then echo "d|$f"; else echo "f|$f"; fi; done"#,
            home
        );
        let raw = ssh::exec_remote(&session, &cmd).await.expect("ls");
        eprintln!("  listing for {}:", home);
        let mut dirs = 0;
        let mut files = 0;
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            if line.starts_with("d|") {
                eprintln!("    [DIR]  {}", &line[2..]);
                dirs += 1;
            } else if line.starts_with("f|") {
                eprintln!("    [FILE] {}", &line[2..]);
                files += 1;
            }
        }
        eprintln!("  total: {} dirs, {} files", dirs, files);
        assert!(dirs > 0 || files > 0, "Home directory should have some entries");

        // Test listing /
        let cmd_root = r#"cd / 2>/dev/null; for f in * .*; do [ "$f" = "." ] && continue; [ "$f" = ".." ] && continue; [ -e "$f" ] || continue; if [ -d "$f" ]; then echo "d|$f"; else echo "f|$f"; fi; done"#;
        let raw_root = ssh::exec_remote(&session, &cmd_root).await.expect("ls /");
        let root_entries: Vec<_> = raw_root.lines().filter(|l| !l.trim().is_empty()).collect();
        eprintln!("  /: {} entries", root_entries.len());
        assert!(root_entries.len() > 3, "Root should have more than 3 entries");

        eprintln!("✓ File listing works");
    });
}
