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
    inner: syphon_wgpu::SyphonWgpuInput,
    server_name: Option<String>,
    resolution: (u32, u32),
}

impl SyphonInputReceiver {
    /// Create a new Syphon input receiver
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        {
            // Create a dummy device/queue - will be properly initialized later
            // We need to create these for the API, but they're not actually used
            // until receive_texture is called
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("Failed to create adapter for Syphon");
            let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("Failed to create device for Syphon");
            
            Self {
                inner: syphon_wgpu::SyphonWgpuInput::new(&device, &queue),
                server_name: None,
                resolution: (1920, 1080),
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            Self {
                server_name: None,
                resolution: (1920, 1080),
            }
        }
    }

    /// Check if Syphon is available
    pub fn is_available() -> bool {
        syphon_core::is_available()
    }

    /// Initialize with wgpu device and queue (required before connect)
    pub fn initialize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        #[cfg(target_os = "macos")]
        {
            self.inner = syphon_wgpu::SyphonWgpuInput::new(device, queue);
        }
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
            self.inner.connect(&server_name)
                .map_err(|e| anyhow::anyhow!("Failed to connect: {:?}", e))?;
        }

        self.server_name = Some(server_name);
        Ok(())
    }

    /// Try to receive a texture frame (zero-copy)
    /// 
    /// NOTE: The new API requires device/queue to be passed here
    pub fn try_receive_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<wgpu::Texture> {
        #[cfg(target_os = "macos")]
        {
            if let Some(texture) = self.inner.receive_texture(device, queue) {
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
            self.inner.disconnect();
        }
        self.server_name = None;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.inner.is_connected()
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
