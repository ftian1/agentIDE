//! Session types and metadata for the Remote Agent Host.

use chrono::{DateTime, Utc};
use shared_protocol::types::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};

/// Flow control watermarks (bytes of unacknowledged output).
pub const DEFAULT_WATERMARK_HIGH: usize = 65536; // 64 KiB
pub const DEFAULT_WATERMARK_LOW: usize = 16384;  // 16 KiB

/// Signal sent to the PTY loop for write/resize/shutdown operations.
#[derive(Debug)]
#[allow(dead_code)]
pub enum PtyOp {
    Write(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Shutdown,
}

/// A managed AI agent CLI session.
#[allow(dead_code)]
pub struct Session {
    pub id: String,
    pub tool: ToolKind,
    pub tool_args: Vec<String>,
    pub pid: u32,
    pub state: tokio::sync::RwLock<SessionState>,
    pub created_at: DateTime<Utc>,

    // ── Flow Control ───────────────────
    pub seq_counter: AtomicU64,
    pub last_acked_seq: AtomicU64,
    pub bytes_pending: AtomicUsize,
    pub watermark_high: usize,
    pub watermark_low: usize,
    pub paused: AtomicBool,

    // ── Metrics ────────────────────────
    pub turn_count: AtomicU64,
    pub total_input_tokens: AtomicU64,
    pub total_output_tokens: AtomicU64,
    pub total_cost_usd: tokio::sync::RwLock<f64>,

    // ── PTY Channel ────────────────────
    /// Sender for write/resize/shutdown ops to the PTY loop.
    pub pty_op_tx: tokio::sync::mpsc::UnboundedSender<PtyOp>,

    // ── Metadata ───────────────────────
    pub metadata: SessionMetadata,
    pub terminal_cols: tokio::sync::RwLock<u16>,
    pub terminal_rows: tokio::sync::RwLock<u16>,
}

impl Session {
    pub fn new(
        id: String,
        tool: ToolKind,
        tool_args: Vec<String>,
        pid: u32,
        metadata: SessionMetadata,
        terminal_cols: u16,
        terminal_rows: u16,
        pty_op_tx: tokio::sync::mpsc::UnboundedSender<PtyOp>,
    ) -> Self {
        Self {
            id,
            tool,
            tool_args,
            pid,
            state: tokio::sync::RwLock::new(SessionState::Running),
            created_at: Utc::now(),
            seq_counter: AtomicU64::new(0),
            last_acked_seq: AtomicU64::new(0),
            bytes_pending: AtomicUsize::new(0),
            watermark_high: DEFAULT_WATERMARK_HIGH,
            watermark_low: DEFAULT_WATERMARK_LOW,
            paused: AtomicBool::new(false),
            turn_count: AtomicU64::new(0),
            total_input_tokens: AtomicU64::new(0),
            total_output_tokens: AtomicU64::new(0),
            total_cost_usd: tokio::sync::RwLock::new(0.0),
            pty_op_tx,
            metadata,
            terminal_cols: tokio::sync::RwLock::new(terminal_cols),
            terminal_rows: tokio::sync::RwLock::new(terminal_rows),
        }
    }

    pub fn next_seq(&self) -> u64 {
        self.seq_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    pub fn ack(&self, seq: u64) {
        let prev = self.last_acked_seq.load(std::sync::atomic::Ordering::SeqCst);
        if seq > prev {
            self.last_acked_seq.store(seq, std::sync::atomic::Ordering::SeqCst);
        }
    }

    pub fn record_ack(&self, bytes: u64) {
        let pending = self.bytes_pending.fetch_sub(bytes as usize, std::sync::atomic::Ordering::SeqCst);
        // If pending dropped below low watermark, auto-resume
        if pending.saturating_sub(bytes as usize) <= self.watermark_low {
            self.paused.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    pub fn record_sent(&self, bytes: usize) {
        let pending = self.bytes_pending.fetch_add(bytes, std::sync::atomic::Ordering::SeqCst);
        if pending + bytes > self.watermark_high {
            self.paused.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
