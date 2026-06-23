//! Remote performance monitor.
//!
//! Periodically samples the remote host's CPU, memory, and disk I/O over the
//! existing SSH session and emits `perf:stats` Tauri events for the status bar.
//!
//! Sampling is a single `cat` of three /proc files per tick, so the cost on the
//! remote is negligible. CPU% and disk-I/O rate are computed from deltas
//! between consecutive ticks.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::connection::ssh::{self, SshSession};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(2500);

/// Perf snapshot emitted to the frontend as `perf:stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfStats {
    pub connection_id: String,
    /// Total CPU utilization 0..100 (across all cores).
    pub cpu_percent: f32,
    /// Used memory in MiB and total memory in MiB.
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    /// Qualitative disk I/O level: "Idle" | "Normal" | "Busy".
    pub disk_io: String,
    /// Aggregate sectors read+written per second across all disks.
    pub disk_sectors_per_sec: u64,
}

#[derive(Clone, Copy)]
struct CpuSample {
    idle: u64,
    total: u64,
}

#[derive(Clone, Copy)]
struct DiskSample {
    sectors: u64,
}

/// Spawn the perf monitor loop for a connection. Exits when the SSH session is
/// removed from `AppState` (i.e. on disconnect) or a sample fails repeatedly.
pub async fn run_perf_monitor(connection_id: String, app_handle: AppHandle) {
    let mut prev_cpu: Option<CpuSample> = None;
    let mut prev_disk: Option<DiskSample> = None;
    let mut consecutive_errors = 0u32;

    loop {
        tokio::time::sleep(SAMPLE_INTERVAL).await;

        // Fetch the current SSH session; if it's gone, the connection closed.
        let session = {
            let state = app_handle.state::<crate::AppState>();
            state.ssh_sessions.get(&connection_id).map(|r| r.value().clone())
        };
        let Some(session) = session else {
            tracing::info!(%connection_id, "Perf monitor: session gone, exiting");
            break;
        };

        match sample(&session).await {
            Ok(raw) => {
                consecutive_errors = 0;
                let (cpu_percent, disk_rate) =
                    derive_rates(&raw, &mut prev_cpu, &mut prev_disk);

                let stats = PerfStats {
                    connection_id: connection_id.clone(),
                    cpu_percent,
                    mem_used_mb: raw.mem_used_kb / 1024,
                    mem_total_mb: raw.mem_total_kb / 1024,
                    disk_io: classify_disk(disk_rate),
                    disk_sectors_per_sec: disk_rate,
                };
                let _ = app_handle.emit("perf:stats", stats);
            }
            Err(e) => {
                consecutive_errors += 1;
                tracing::debug!(%connection_id, error = %e, consecutive_errors, "Perf sample failed");
                if consecutive_errors >= 4 {
                    tracing::info!(%connection_id, "Perf monitor: too many errors, exiting");
                    break;
                }
            }
        }
    }
}

struct RawSample {
    cpu_idle: u64,
    cpu_total: u64,
    mem_total_kb: u64,
    mem_used_kb: u64,
    disk_sectors: u64,
}

/// One round-trip reading all three /proc files.
async fn sample(session: &SshSession) -> anyhow::Result<RawSample> {
    // Markers delimit the three sections in a single exec.
    let cmd = "echo __CPU__; head -n1 /proc/stat; \
               echo __MEM__; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo; \
               echo __DISK__; cat /proc/diskstats";
    let out = ssh::exec_remote(session, cmd).await?;
    parse_sample(&out)
}

fn parse_sample(out: &str) -> anyhow::Result<RawSample> {
    let mut section = "";
    let mut cpu_idle = 0u64;
    let mut cpu_total = 0u64;
    let mut mem_total_kb = 0u64;
    let mut mem_avail_kb = 0u64;
    let mut disk_sectors = 0u64;

    for line in out.lines() {
        match line.trim() {
            "__CPU__" => { section = "cpu"; continue; }
            "__MEM__" => { section = "mem"; continue; }
            "__DISK__" => { section = "disk"; continue; }
            _ => {}
        }
        match section {
            "cpu" if line.starts_with("cpu ") => {
                // cpu  user nice system idle iowait irq softirq steal ...
                let vals: Vec<u64> = line
                    .split_whitespace()
                    .skip(1)
                    .filter_map(|s| s.parse().ok())
                    .collect();
                cpu_total = vals.iter().sum();
                // idle = idle + iowait (indices 3 and 4)
                cpu_idle = vals.get(3).copied().unwrap_or(0) + vals.get(4).copied().unwrap_or(0);
            }
            "mem" => {
                if let Some(v) = line.strip_prefix("MemTotal:") {
                    mem_total_kb = parse_kb(v);
                } else if let Some(v) = line.strip_prefix("MemAvailable:") {
                    mem_avail_kb = parse_kb(v);
                }
            }
            "disk" => {
                // Fields: major minor name reads rdMerged sectorsRead msRead
                //         writes wrMerged sectorsWritten ...
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() >= 10 {
                    let name = f[2];
                    // Skip partitions (e.g. sda1) and loop/ram devices; aggregate
                    // only whole disks to avoid double-counting.
                    if is_whole_disk(name) {
                        let sectors_read: u64 = f[5].parse().unwrap_or(0);
                        let sectors_written: u64 = f[9].parse().unwrap_or(0);
                        disk_sectors += sectors_read + sectors_written;
                    }
                }
            }
            _ => {}
        }
    }

    anyhow::ensure!(cpu_total > 0, "failed to parse /proc/stat");
    anyhow::ensure!(mem_total_kb > 0, "failed to parse /proc/meminfo");

    Ok(RawSample {
        cpu_idle,
        cpu_total,
        mem_total_kb,
        mem_used_kb: mem_total_kb.saturating_sub(mem_avail_kb),
        disk_sectors,
    })
}

/// Update prev samples and return (cpu_percent, disk_sectors_per_sec).
fn derive_rates(
    raw: &RawSample,
    prev_cpu: &mut Option<CpuSample>,
    prev_disk: &mut Option<DiskSample>,
) -> (f32, u64) {
    let cur_cpu = CpuSample { idle: raw.cpu_idle, total: raw.cpu_total };
    let cpu_percent = match *prev_cpu {
        Some(p) => {
            let dt = cur_cpu.total.saturating_sub(p.total);
            let di = cur_cpu.idle.saturating_sub(p.idle);
            if dt == 0 { 0.0 } else { (1.0 - (di as f32 / dt as f32)) * 100.0 }
        }
        None => 0.0,
    };
    *prev_cpu = Some(cur_cpu);

    let cur_disk = DiskSample { sectors: raw.disk_sectors };
    let disk_rate = match *prev_disk {
        Some(p) => {
            let ds = cur_disk.sectors.saturating_sub(p.sectors);
            // sectors over the sample interval → per second
            (ds as f64 / SAMPLE_INTERVAL.as_secs_f64()) as u64
        }
        None => 0,
    };
    *prev_disk = Some(cur_disk);

    (cpu_percent.clamp(0.0, 100.0), disk_rate)
}

/// Map a disk-sector rate to a qualitative level. A standard 512-byte sector
/// means ~2000 sectors/s ≈ 1 MB/s. Thresholds are deliberately coarse — the
/// status bar shows a word, not a number.
fn classify_disk(sectors_per_sec: u64) -> String {
    match sectors_per_sec {
        0..=200 => "Idle".to_string(),
        201..=40_000 => "Normal".to_string(),  // up to ~20 MB/s
        _ => "Busy".to_string(),
    }
}

fn parse_kb(s: &str) -> u64 {
    s.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// Whole disks only: sd*, vd*, nvme*n*, xvd*, hd* — excluding trailing
/// partition numbers (sda1) and virtual devices (loop, ram, dm-).
fn is_whole_disk(name: &str) -> bool {
    let is_disk_prefix = name.starts_with("sd")
        || name.starts_with("vd")
        || name.starts_with("xvd")
        || name.starts_with("hd")
        || name.starts_with("nvme");
    if !is_disk_prefix {
        return false;
    }
    if name.starts_with("nvme") {
        // nvme0n1 = disk; nvme0n1p1 = partition. Partitions contain a 'p'
        // followed by digits after the namespace.
        return !name.contains('p');
    }
    // sda = disk, sda1 = partition → reject if it ends with a digit.
    !name.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combined_sample() {
        let out = "__CPU__\n\
            cpu  100 0 50 1000 20 0 5 0 0 0\n\
            __MEM__\n\
            MemTotal:       8000000 kB\n\
            MemAvailable:   2000000 kB\n\
            __DISK__\n\
            8 0 sda 10 0 500 5 20 0 300 8 0 1 1\n\
            8 1 sda1 5 0 100 2 10 0 50 3 0 1 1\n";
        let s = parse_sample(out).unwrap();
        // total = 100+0+50+1000+20+0+5 = 1175; idle = idle(1000)+iowait(20) = 1020
        assert_eq!(s.cpu_total, 1175);
        assert_eq!(s.cpu_idle, 1020);
        assert_eq!(s.mem_total_kb, 8000000);
        assert_eq!(s.mem_used_kb, 6000000);
        // only sda counted (sda1 is a partition): 500 read + 300 written = 800
        assert_eq!(s.disk_sectors, 800);
    }

    #[test]
    fn cpu_percent_from_deltas() {
        let mut pc = None;
        let mut pd = None;
        let s1 = RawSample { cpu_idle: 1000, cpu_total: 2000, mem_total_kb: 1, mem_used_kb: 0, disk_sectors: 0 };
        let (p1, _) = derive_rates(&s1, &mut pc, &mut pd);
        assert_eq!(p1, 0.0, "first sample has no baseline");
        // next: idle +100, total +200 → 50% idle delta → 50% busy
        let s2 = RawSample { cpu_idle: 1100, cpu_total: 2200, mem_total_kb: 1, mem_used_kb: 0, disk_sectors: 0 };
        let (p2, _) = derive_rates(&s2, &mut pc, &mut pd);
        assert!((p2 - 50.0).abs() < 0.01, "expected ~50%, got {p2}");
    }

    #[test]
    fn whole_disk_detection() {
        assert!(is_whole_disk("sda"));
        assert!(is_whole_disk("vda"));
        assert!(is_whole_disk("nvme0n1"));
        assert!(!is_whole_disk("sda1"));
        assert!(!is_whole_disk("nvme0n1p1"));
        assert!(!is_whole_disk("loop0"));
        assert!(!is_whole_disk("dm-0"));
    }

    #[test]
    fn disk_classification() {
        assert_eq!(classify_disk(0), "Idle");
        assert_eq!(classify_disk(100), "Idle");
        assert_eq!(classify_disk(5000), "Normal");
        assert_eq!(classify_disk(100_000), "Busy");
    }
}
