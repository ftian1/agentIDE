//! Remote Bootstrap — deploys the Remote Agent Host to Linux servers.
//! Requires the `ssh` feature.

#[cfg(feature = "ssh")]
pub mod detector;
#[cfg(feature = "ssh")]
pub mod uploader;
#[cfg(feature = "ssh")]
pub mod installer;
