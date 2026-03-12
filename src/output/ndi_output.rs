//! # NDI Output
//!
//! Network Device Interface video output sender.

use grafton_ndi::send::Send;
use std::thread;

/// NDI video sender running on dedicated thread
pub struct NdiOutputSender {
    _name: String,
    width: u32,
    height: u32,
    include_alpha: bool,
}

impl NdiOutputSender {
    /// Create a new NDI output sender
    pub fn new(name: &str, width: u32, height: u32, include_alpha: bool) -> anyhow::Result<Self> {
        // Initialize NDI
        let _ndi = grafton_ndi::Ndi::new()?;

        Ok(Self {
            _name: name.to_string(),
            width,
            height,
            include_alpha,
        })
    }

    /// Send a video frame
    pub fn send_frame(&mut self, _data: &[u8]) {
        // TODO: Implement frame sending
        // This requires creating a send instance and submitting frames
    }
}
