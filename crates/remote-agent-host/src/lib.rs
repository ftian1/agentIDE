//! Remote Agent Host — runs on Linux, manages PTY sessions for AI agent CLIs.
//!
//! # Architecture
//! - [`server`] — Main event loop: recv msg → dispatch handler → send response.
//! - [`session`] — Session struct, registry, and manager.
//! - [`pty`] — PTY worker and flow control.
//! - [`transport`] — Transport layer (stdio, future websocket).

pub mod server;
pub mod session;
pub mod pty;
pub mod installer;
pub mod transport;
pub mod agent_parse;
