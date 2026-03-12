//! # Syphon Input (macOS)
//!
//! GPU texture sharing input for macOS using Syphon framework.

use std::sync::Arc;
use std::time::Instant;

/// Information about a Syphon server
pub use syphon_core::ServerInfo as SyphonServerInfo;

/// Syphon input receiver using syphon-wgpu integration
pub struct SyphonInputReceiver {
    #[cfg(target_os = "macos")]
    client: Option<syphon_wgpu::SyphonWgpuInput>,
    server_name: Option<String>,
    resolution: (u32, u32),
    device: Option<Arc<wgpu::Device>>,
    queue: Option<Arc<wgpu::Queue>>,
}

impl SyphonInputReceiver {
    /// Create a new Syphon input receiver
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            client: None,
            server_name: None,
            resolution: (1920, 1080),
            device: None,
            queue: None,
        }
    }

    /// Check if Syphon is available
    pub fn is_available() -> bool {
        syphon_core::is_available()
    }

    /// Initialize with wgpu device and queue
    pub fn initialize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.device = Some(Arc::new(device.clone()));
        self.queue = Some(Arc::new(queue.clone()));
    }

    /// Connect to a Syphon server by name
    pub fn connect(&mut self, server_name: impl Into<String>) -> anyhow::Result<()> {
        let server_name = server_name.into();

        if self.is_connected() {
            self.disconnect();
        }

        log::info!("[Syphon Input] Connecting to: {}", server_name);

        #[cfg(target_os = "macos")]
        {
            let (device, queue) = self.device.as_ref()
                .and_then(|d| self.queue.as_ref().map(|q| (d, q)))
                .ok_or_else(|| anyhow::anyhow!("SyphonInputReceiver not initialized"))?;

            let mut client = syphon_wgpu::SyphonWgpuInput::new(device, queue);
            client.connect(&server_name)
                .map_err(|e| anyhow::anyhow!("Failed to connect: {:?}", e))?;

            self.client = Some(client);
        }

        self.server_name = Some(server_name);
        Ok(())
    }

    /// Try to receive a texture frame (zero-copy)
    pub fn try_receive_texture(&mut self) -> Option<wgpu::Texture> {
        #[cfg(target_os = "macos")]
        {
            let client = self.client.as_mut()?;
            let device = self.device.as_ref()?;
            let queue = self.queue.as_ref()?;

            if let Some(texture) = client.receive_texture(device, queue) {
                self.resolution = (texture.width(), texture.height());
                return Some(texture);
            }
        }

        None
    }

    /// Disconnect from the server
    pub fn disconnect(&mut self) {
        #[cfg(target_os = "macos")]
        {
            self.client = None;
        }
        self.server_name = None;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.client.as_ref().map_or(false, |c| c.is_connected())
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Get current resolution
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }
}

impl Default for SyphonInputReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SyphonInputReceiver {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Syphon server discovery
pub struct SyphonDiscovery;

impl SyphonDiscovery {
    /// Create a new discovery instance
    pub fn new() -> Self {
        Self
    }

    /// Discover available Syphon servers
    pub fn discover_servers(&self) -> Vec<SyphonServerInfo> {
        syphon_core::SyphonServerDirectory::servers()
    }
}

impl Default for SyphonDiscovery {
    fn default() -> Self {
        Self::new()
    }
}
