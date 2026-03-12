//! # NDI Output
//!
//! Network Device Interface video output sender.
//! Stub implementation - TODO: implement with correct grafton-ndi API

/// NDI output sender (stub)
pub struct NdiOutputSender;

impl NdiOutputSender {
    /// Create a new NDI output sender (stub)
    pub fn new(_name: &str, _width: u32, _height: u32, _include_alpha: bool) -> anyhow::Result<Self> {
        log::warn!("NDI output not yet implemented");
        Ok(Self)
    }

    /// Send a video frame (stub)
    pub fn send_frame(&self, _data: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }
}
