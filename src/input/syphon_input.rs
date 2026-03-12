//! # Syphon Input (macOS)
//!
//! GPU texture sharing input for macOS using Syphon framework.
//! Provides zero-copy texture sharing from applications like Resolume, MadMapper, etc.

use anyhow::Result;
use std::sync::Arc;

/// Information about a Syphon server
#[derive(Debug, Clone)]
pub struct SyphonServerInfo {
    pub name: String,
    pub app_name: String,
}

impl SyphonServerInfo {
    /// Get display name (prefer server name, fall back to app name)
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.app_name
        } else {
            &self.name
        }
    }
}

/// Syphon input receiver using syphon-wgpu integration
pub struct SyphonInputReceiver {
    inner: Option<syphon_wgpu::SyphonWgpuInput>,
    device: Option<Arc<wgpu::Device>>,
    queue: Option<Arc<wgpu::Queue>>,
    connected: bool,
}

impl SyphonInputReceiver {
    /// Create a new Syphon input receiver
    pub fn new() -> Self {
        Self {
            inner: None,
            device: None,
            queue: None,
            connected: false,
        }
    }

    /// Initialize with wgpu device and queue
    pub fn initialize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.device = Some(Arc::new(device.clone()));
        self.queue = Some(Arc::new(queue.clone()));
    }

    /// Connect to a Syphon server by name
    pub fn connect(&mut self, server_name: &str) -> Result<()> {
        let device = self.device.as_ref()
            .ok_or_else(|| anyhow::anyhow!("SyphonInputReceiver not initialized"))?;
        let queue = self.queue.as_ref()
            .ok_or_else(|| anyhow::anyhow!("SyphonInputReceiver not initialized"))?;

        // Create the syphon-wgpu input
        let input = syphon_wgpu::SyphonWgpuInput::new(
            &**device,
            &**queue,
            server_name,
            syphon_wgpu::InputFormat::Bgra8,
        )?;

        self.inner = Some(input);
        self.connected = true;

        Ok(())
    }

    /// Try to receive a texture frame (zero-copy)
    /// Returns None if no new frame is available
    pub fn try_receive_texture(&mut self) -> Option<wgpu::Texture> {
        if !self.connected {
            return None;
        }

        if let Some(ref mut inner) = self.inner {
            // Try to get a frame from syphon-wgpu
            inner.try_receive_frame().ok().flatten()
        } else {
            None
        }
    }

    /// Disconnect from the server
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.inner = None;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

impl Default for SyphonInputReceiver {
    fn default() -> Self {
        Self::new()
    }
}

/// Syphon server discovery
pub struct SyphonDiscovery {
    inner: syphon_core::SyphonDirectory,
}

impl SyphonDiscovery {
    /// Create a new discovery instance
    pub fn new() -> Self {
        Self {
            inner: syphon_core::SyphonDirectory::new(),
        }
    }

    /// Discover available Syphon servers
    pub fn discover_servers(&self) -> Vec<SyphonServerInfo> {
        let servers = self.inner.servers();
        
        servers
            .into_iter()
            .map(|server| {
                let name = server.name().map(|s| s.to_string()).unwrap_or_default();
                let app_name = server.app_name().map(|s| s.to_string()).unwrap_or_default();
                
                SyphonServerInfo { name, app_name }
            })
            .collect()
    }
}

impl Default for SyphonDiscovery {
    fn default() -> Self {
        Self::new()
    }
}
