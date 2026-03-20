//! # Spout Input (Windows)
//!
//! GPU texture sharing input via Spout2 (DirectX shared surfaces).
//! This is the Windows equivalent of Syphon input.
//!
//! ## Implementation TODO
//!
//! 1. Find and add a Spout2 Rust crate to Cargo.toml (see PORTING_GUIDE.md Step 1)
//! 2. Replace the stub bodies below with real Spout2 SDK calls
//! 3. `SpoutDiscovery::list_senders()` — enumerate active Spout senders
//! 4. `SpoutInputReceiver::connect()` — open a named sender
//! 5. `try_receive_texture()` — poll for a new shared texture each frame
//! 6. Expose the received texture as a `wgpu::Texture` (may require a copy from D3D11)
//!
//! Study `src/input/syphon_input.rs` for the pattern to follow.

#![cfg(target_os = "windows")]

/// Information about an available Spout sender
#[derive(Debug, Clone)]
pub struct SpoutSenderInfo {
    /// Sender name as registered with the Spout SDK
    pub name: String,
    /// Width of the shared texture
    pub width: u32,
    /// Height of the shared texture
    pub height: u32,
}

/// Discovers active Spout senders on this machine
pub struct SpoutDiscovery;

impl SpoutDiscovery {
    /// Return a list of all active Spout senders.
    ///
    /// TODO (Windows): call the Spout SDK to enumerate senders.
    pub fn list_senders() -> Vec<SpoutSenderInfo> {
        log::warn!("SpoutDiscovery::list_senders — not yet implemented");
        Vec::new()
    }
}

/// Receives frames from a Spout sender as a wgpu texture
pub struct SpoutInputReceiver {
    /// Name of the connected sender (None = disconnected)
    sender_name: Option<String>,
    /// Current resolution of the shared texture
    resolution: (u32, u32),
    // TODO (Windows): add inner: Option<spout::Receiver> (replace with real type)
}

impl SpoutInputReceiver {
    /// Create an unconnected receiver
    pub fn new() -> Self {
        Self {
            sender_name: None,
            resolution: (0, 0),
        }
    }

    /// Connect to the named sender.
    ///
    /// TODO (Windows): call `spout::Receiver::connect(sender_name)` or equivalent.
    pub fn connect(&mut self, sender_name: &str) -> anyhow::Result<()> {
        log::warn!("SpoutInputReceiver::connect({}) — not yet implemented", sender_name);
        self.sender_name = Some(sender_name.to_string());
        Ok(())
    }

    /// Disconnect from the current sender
    pub fn disconnect(&mut self) {
        self.sender_name = None;
        self.resolution = (0, 0);
    }

    /// Poll for a new frame from the sender.
    ///
    /// Returns `true` if a new texture is available (call `output_texture()` to use it).
    ///
    /// TODO (Windows): call the Spout SDK receive method; update `self.resolution`.
    pub fn try_receive_texture(&mut self) -> bool {
        // TODO (Windows): implement GPU texture receive
        false
    }

    /// Borrow the most recently received texture.
    ///
    /// Valid only after `try_receive_texture()` returns `true`.
    ///
    /// TODO (Windows): return a reference to the wgpu::Texture wrapping the shared D3D11 surface.
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        None
    }

    /// Current resolution of the shared texture
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }
}

impl Default for SpoutInputReceiver {
    fn default() -> Self {
        Self::new()
    }
}
