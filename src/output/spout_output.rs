//! # Spout Output (Windows)
//!
//! GPU texture sharing output via Spout2 (DirectX shared surfaces).
//! This is the Windows equivalent of Syphon output.
//!
//! ## Implementation TODO
//!
//! 1. Find and add a Spout2 Rust crate to Cargo.toml (see PORTING_GUIDE.md Step 1)
//! 2. Replace the stub bodies below with real Spout2 SDK calls
//! 3. `SpoutOutput::new()` — create a named sender and register it with the Spout SDK
//! 4. `submit_frame()` — share the wgpu texture each frame (may require D3D11 interop)
//!
//! Study `src/output/syphon_output.rs` for the pattern to follow.

#![cfg(target_os = "windows")]

use std::sync::Arc;

/// Spout sender — shares wgpu render output with other apps on the same machine
pub struct SpoutOutput {
    /// Name visible to receiving apps
    sender_name: String,
    // TODO (Windows): add inner: spout::Sender (replace with real type)
}

impl SpoutOutput {
    /// Create a new Spout sender with the given name.
    ///
    /// TODO (Windows): call `spout::Sender::new(name)` or equivalent and register
    /// it with the Spout SDK so other applications can discover this sender.
    pub fn new(
        name: &str,
        _device: Arc<wgpu::Device>,
        _queue: Arc<wgpu::Queue>,
    ) -> anyhow::Result<Self> {
        log::warn!("SpoutOutput::new({}) — not yet implemented", name);
        Ok(Self {
            sender_name: name.to_string(),
        })
    }

    /// Share a wgpu texture with all connected Spout receivers.
    ///
    /// TODO (Windows): copy or alias the wgpu texture into a D3D11 shared surface
    /// and call the Spout SDK send method to publish it.
    pub fn submit_frame(
        &mut self,
        _texture: &wgpu::Texture,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> anyhow::Result<()> {
        log::warn!("SpoutOutput::submit_frame — not yet implemented (sender: {})", self.sender_name);
        Ok(())
    }
}
