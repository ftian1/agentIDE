//! Tauri IPC commands for remote file listing.
//!
//! Reuses the SSH session stored in AppState to run `ls` on the
//! remote machine via `exec_remote`.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: String, // "file" | "directory"
}

/// List files in a directory on the remote machine.
#[tauri::command]
pub async fn list_files(
    state: State<'_, AppState>,
    connection_id: String,
    path: String,
) -> Result<Vec<FileEntry>, String> {
    // Look up the stored SSH session
    let session_ref = state
        .ssh_sessions
        .get(&connection_id)
        .ok_or_else(|| format!("Connection not found: {}", connection_id))?;
    let ssh_arc = session_ref.value().clone();

    // Normalize path
    let dir = if path == "/" {
        "/".to_string()
    } else {
        path.trim_end_matches('/').to_string()
    };

    // Inline shell: list all entries including dotfiles, mark d| or f|
    let cmd = format!(
        r#"cd "{}" 2>/dev/null || exit 1; for f in * .*; do [ "$f" = "." ] && continue; [ "$f" = ".." ] && continue; [ -e "$f" ] || continue; if [ -d "$f" ]; then echo "d|$f"; else echo "f|$f"; fi; done"#,
        dir
    );
    let raw = crate::connection::ssh::exec_remote(ssh_arc.as_ref(), &cmd)
        .await
        .map_err(|e| format!("ls failed: {e}"))?;

    let mut entries = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (kind, name) = if let Some(n) = line.strip_prefix("d|") {
            ("directory", n.to_string())
        } else if let Some(n) = line.strip_prefix("f|") {
            ("file", n.to_string())
        } else {
            continue;
        };
        entries.push(FileEntry {
            path: if dir == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", dir, name)
            },
            name,
            kind: kind.into(),
        });
    }

    // Directories first, then alphabetical
    entries.sort_by(|a, b| b.kind.cmp(&a.kind).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));

    Ok(entries)
}

/// Read a file's contents from the remote machine (base64 for binary safety).
#[tauri::command]
pub async fn read_file(
    state: State<'_, AppState>,
    connection_id: String,
    path: String,
) -> Result<String, String> {
    let session_ref = state
        .ssh_sessions
        .get(&connection_id)
        .ok_or_else(|| format!("Connection not found: {}", connection_id))?;
    let ssh_arc = session_ref.value().clone();

    // base64-encode on the remote so arbitrary bytes survive the text channel.
    let cmd = format!("base64 < {} 2>/dev/null", shell_quote(&path));
    let raw = crate::connection::ssh::exec_remote(ssh_arc.as_ref(), &cmd)
        .await
        .map_err(|e| format!("read failed: {e}"))?;

    let bytes = decode_base64(&raw).map_err(|e| format!("decode failed: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("file is not valid UTF-8: {e}"))
}

/// Write file contents back to the remote machine (atomic temp-file rename).
#[tauri::command]
pub async fn write_file(
    state: State<'_, AppState>,
    connection_id: String,
    path: String,
    content: String,
) -> Result<(), String> {
    let session_ref = state
        .ssh_sessions
        .get(&connection_id)
        .ok_or_else(|| format!("Connection not found: {}", connection_id))?;
    let ssh_arc = session_ref.value().clone();

    // Pipe base64 → decode → temp file → atomic rename, so a failed write never
    // truncates the original.
    let b64 = encode_base64(content.as_bytes());
    let quoted = shell_quote(&path);
    let tmp = format!("{}.rai_tmp", quoted);
    let cmd = format!(
        "printf '%s' {} | base64 -d > {} && mv {} {}",
        shell_quote(&b64),
        tmp,
        tmp,
        quoted
    );
    crate::connection::ssh::exec_remote(ssh_arc.as_ref(), &cmd)
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    Ok(())
}

/// Single-quote a string for safe POSIX shell interpolation.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn decode_base64(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let Some(v) = val(c) else { continue }; // skip whitespace/newlines
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        for sample in ["", "hello", "fn main() {}\n", "中文 + symbols !@#\n\t"] {
            let encoded = encode_base64(sample.as_bytes());
            let decoded = decode_base64(&encoded).unwrap();
            assert_eq!(decoded, sample.as_bytes(), "roundtrip failed for {sample:?}");
        }
    }

    #[test]
    fn base64_decode_skips_newlines() {
        let encoded = encode_base64(b"line content here");
        let wrapped = format!("{}\n{}\n", &encoded[..4], &encoded[4..]);
        assert_eq!(decode_base64(&wrapped).unwrap(), b"line content here");
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("a'b"), r"'a'\''b'");
        assert_eq!(shell_quote("/tmp/x"), "'/tmp/x'");
    }
}
