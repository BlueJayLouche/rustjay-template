//! # Syphon Output (macOS)
//!
//! GPU texture sharing output for macOS using Syphon framework.

use anyhow::Result;
use std::sync::Arc;

/// Syphon output server
pub struct SyphonOutput {
    server_name: String,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    server: Option<syphon_wgpu::SyphonWgpuOutput>,
    width: u32,
    height: u32,
}

impl SyphonOutput {
    /// Create a new Syphon output
    pub fn new(
        server_name: &str,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Result<Self> {
        Ok(Self {
            server_name: server_name.to_string(),
            device,
            queue,
            server: None,
            width: 1920,
            height: 1080,
        })
    }

    /// Initialize the Syphon server with dimensions
    pub fn initialize(&mut self, width: u32, height: u32) -> Result<()> {
        self.width = width;
        self.height = height;

        // Create the Syphon server using syphon-wgpu
        let server = syphon_wgpu::SyphonWgpuOutput::new(
            &self.server_name,
            &self.device,
            &self.queue,
            width,
            height,
        )?;

        self.server = Some(server);
        log::info!("Syphon server initialized: {} ({}x{})", self.server_name, width, height);

        Ok(())
    }

    /// Submit a frame to Syphon (zero-copy)
    pub fn submit_frame(
        &mut self,
        texture: &wgpu::Texture,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        if let Some(ref mut server) = self.server {
            // Publish the texture to Syphon
            server.publish_frame(texture, queue)?;
        }
        Ok(())
    }

    /// Shutdown the Syphon server
    pub fn shutdown(&mut self) {
        self.server = None;
        log::info!("Syphon server shutdown: {}", self.server_name);
    }
}
