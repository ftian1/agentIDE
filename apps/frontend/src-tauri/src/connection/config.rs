//! SSH configuration — parses ~/.ssh/config for connection details.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A parsed SSH host entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshHostConfig {
    pub alias: String,
    pub hostname: String,
    pub port: u16,
    pub user: String,
    pub identity_file: Option<String>,
}

/// Parse ~/.ssh/config and return a list of Host entries.
///
/// This is a simple line-by-line parser that handles the most common
/// directives: Host, Hostname, Port, User, IdentityFile.
pub fn parse_ssh_config() -> Vec<SshHostConfig> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let config_path = home.join(".ssh").join("config");

    if !config_path.exists() {
        return vec![];
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut hosts: Vec<SshHostConfig> = Vec::new();
    let mut current: Option<SshHostConfig> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Split on whitespace, handling quoted values
        let parts = shell_words::split(trimmed).unwrap_or_else(|_| {
            trimmed.split_whitespace().map(String::from).collect()
        });

        if parts.is_empty() {
            continue;
        }

        let directive = parts[0].to_lowercase();
        let value = parts.get(1).map(|s| s.as_str()).unwrap_or("");

        match directive.as_str() {
            "host" => {
                // Save previous host
                if let Some(host) = current.take() {
                    hosts.push(host);
                }
                // Start new host entry
                let alias = parts.get(1).map(|s| s.clone()).unwrap_or_default();
                current = Some(SshHostConfig {
                    alias,
                    hostname: String::new(),
                    port: 22,
                    user: String::new(),
                    identity_file: None,
                });
            }
            "hostname" => {
                if let Some(ref mut host) = current {
                    host.hostname = value.to_string();
                }
            }
            "port" => {
                if let Some(ref mut host) = current {
                    host.port = value.parse().unwrap_or(22);
                }
            }
            "user" => {
                if let Some(ref mut host) = current {
                    host.user = value.to_string();
                }
            }
            "identityfile" => {
                if let Some(ref mut host) = current {
                    host.identity_file = Some(shellexpand(value));
                }
            }
            _ => {}
        }
    }

    if let Some(host) = current {
        hosts.push(host);
    }

    hosts
}

/// Resolve an SSH host alias against ~/.ssh/config.
/// Returns the resolved (hostname, port, user, identity_file) or falls back to the alias.
pub fn resolve_host(alias: &str) -> (String, u16, String, Option<String>) {
    let configs = parse_ssh_config();
    for cfg in &configs {
        // SSH config Host can contain glob patterns
        if host_matches(alias, &cfg.alias) {
            let user = if cfg.user.is_empty() {
                whoami::username()
            } else {
                cfg.user.clone()
            };
            return (
                if cfg.hostname.is_empty() { alias.to_string() } else { cfg.hostname.clone() },
                cfg.port,
                user,
                cfg.identity_file.clone(),
            );
        }
    }
    // Fallback: use the alias as hostname
    (alias.to_string(), 22, whoami::username(), None)
}

/// Simple glob matching for SSH config Host patterns.
fn host_matches(host: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.contains('*') || pattern.contains('?') {
        simple_glob_match(host, pattern)
    } else {
        host.eq_ignore_ascii_case(pattern)
    }
}

/// Very simple glob matching without regex dependency.
fn simple_glob_match(text: &str, pattern: &str) -> bool {
    let pattern_bytes = pattern.as_bytes();
    let text_bytes = text.as_bytes();
    let mut pi = 0;
    let mut ti = 0;
    let mut star_idx: isize = -1;
    let mut match_idx: isize = -1;

    while ti < text_bytes.len() {
        if pi < pattern_bytes.len() && pattern_bytes[pi] == b'*' {
            star_idx = pi as isize;
            match_idx = ti as isize;
            pi += 1;
        } else if pi < pattern_bytes.len() &&
            (pattern_bytes[pi] == b'?' || pattern_bytes[pi].eq_ignore_ascii_case(&text_bytes[ti]))
        {
            pi += 1;
            ti += 1;
        } else if star_idx >= 0 {
            pi = (star_idx + 1) as usize;
            match_idx += 1;
            ti = match_idx as usize;
        } else {
            return false;
        }
    }

    while pi < pattern_bytes.len() && pattern_bytes[pi] == b'*' {
        pi += 1;
    }

    pi == pattern_bytes.len()
}

/// Expand tilde and environment variables in a path.
fn shellexpand(s: &str) -> String {
    let expanded = s.replace("~", &dirs::home_dir().unwrap_or_default().to_string_lossy());
    expanded
}

/// Get the current username.
mod whoami {
    pub fn username() -> String {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "root".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_matches_exact() {
        assert!(host_matches("dev.example.com", "dev.example.com"));
        assert!(!host_matches("dev.example.com", "prod.example.com"));
    }

    #[test]
    fn test_host_matches_wildcard() {
        assert!(host_matches("dev.example.com", "*.example.com"));
        assert!(host_matches("anything", "*"));
    }
}
