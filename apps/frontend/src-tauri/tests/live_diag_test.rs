//! Diagnostic test — check disk space and inodes on remote.
//! Run: SSH_HOST=... SSH_USER=... SSH_PASS=... cargo test -p remote-ai-ide --test live_diag_test -- --nocapture

use std::env;
use remote_ai_ide_lib::connection::ssh::{self, SshConnectionParams, AuthMethod};

fn env_or_skip(k: &str) -> String {
    env::var(k).unwrap_or_else(|_| panic!("SKIP: {} not set", k))
}

#[test]
fn test_disk_diag() {
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

        // Check inodes
        let inodes = ssh::exec_remote(&session, "df -i / /tmp ~ 2>/dev/null").await.unwrap();
        eprintln!("--- inodes ---\n{}", inodes);

        // Check disk
        let disk = ssh::exec_remote(&session, "df -h / /tmp ~ 2>/dev/null").await.unwrap();
        eprintln!("--- disk ---\n{}", disk);

        // Check ulimit
        let ulimit = ssh::exec_remote(&session, "ulimit -n").await.unwrap();
        eprintln!("--- ulimit -n (max fds) ---\n{}", ulimit);

        // Check Claude/Node writable dirs
        let writable = ssh::exec_remote(&session, r#"
            echo "HOME=$HOME"; echo "TMPDIR=${TMPDIR:-/tmp}";
            for d in ~ ~/.cache ~/.npm ~/.npm-global ~/.claude /tmp; do
                [ -d "$d" ] && echo "  $(df -h $d | tail -1 | awk '{print $4" free "$5" used on "$NF}')  -> $d" || echo "  NOT FOUND: $d"
            done
            echo "---"
            # Try writing a small file to each location
            for d in ~ ~/.cache /tmp; do
                f="$d/_space_test_$$"
                dd if=/dev/zero of="$f" bs=1024 count=1 2>/dev/null && rm -f "$f" && echo "OK: $d" || echo "FAIL: $d"
            done
        "#).await.unwrap();
        eprintln!("--- writable dirs ---\n{}", writable);

        // Check if there's a per-user fs quota
        let quota = ssh::exec_remote(&session, "quota -v 2>/dev/null || echo 'no quota'").await.unwrap();
        eprintln!("--- quota ---\n{}", quota);
    });
}
