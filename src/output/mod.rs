//! # Output Module
//!
//! Video output to other applications via:
//! - NDI (cross-platform network)
//! - Syphon (macOS GPU texture sharing)
//! - Spout (Windows GPU texture sharing) - TODO
//! - v4l2loopback (Linux virtual camera) - TODO

use std::sync::Arc;

pub mod ndi_output;
#[cfg(target_os = "macos")]
pub mod syphon_output;

use ndi_output::NdiOutputSender;

/// Manages all video outputs
pub struct OutputManager {
    /// NDI network output
    ndi_output: Option<NdiOutputSender>,

    /// Syphon output (macOS)
    #[cfg(target_os = "macos")]
    syphon_output: Option<syphon_output::SyphonOutput>,

    frame_count: u64,
}

impl OutputManager {
    /// Create a new output manager
    pub fn new() -> Self {
        Self {
            ndi_output: None,
            #[cfg(target_os = "macos")]
            syphon_output: None,
            frame_count: 0,
        }
    }

    /// Start NDI output
    pub fn start_ndi(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        include_alpha: bool,
    ) -> anyhow::Result<()> {
        let sender = NdiOutputSender::new(name, width, height, include_alpha)?;
        self.ndi_output = Some(sender);
        log::info!("NDI output started: {} ({}x{})", name, width, height);
        Ok(())
    }

    /// Stop NDI output
    pub fn stop_ndi(&mut self) {
        if self.ndi_output.take().is_some() {
            log::info!("NDI output stopped");
        }
    }

    /// Start Syphon output (macOS only)
    #[cfg(target_os = "macos")]
    pub fn start_syphon(
        &mut self,
        server_name: &str,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> anyhow::Result<()> {
        let syphon = syphon_output::SyphonOutput::new(server_name, device, queue)?;
        self.syphon_output = Some(syphon);
        log::info!("Syphon output started: {}", server_name);
        Ok(())
    }

    /// Stop Syphon output (macOS only)
    #[cfg(target_os = "macos")]
    pub fn stop_syphon(&mut self) {
        if self.syphon_output.take().is_some() {
            log::info!("Syphon output stopped");
        }
    }

    /// Submit frame to all active outputs
    pub fn submit_frame(&mut self, texture: &wgpu::Texture, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.frame_count += 1;

        // NDI output - requires GPU readback
        if self.ndi_output.is_some() {
            if let Some(data) = self.read_texture_bgra(texture, device, queue) {
                if let Some(ref sender) = self.ndi_output {
                    sender.submit_frame(&data, texture.width(), texture.height());
                }
            }
        }

        // Syphon output (zero-copy on macOS)
        #[cfg(target_os = "macos")]
        if let Some(ref mut syphon) = self.syphon_output {
            if let Err(e) = syphon.submit_frame(texture, device, queue) {
                log::error!("Syphon output error: {}", e);
            }
        }
    }
    
    /// Read texture data back to CPU as BGRA
    fn read_texture_bgra(&self, texture: &wgpu::Texture, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Vec<u8>> {
        let width = texture.width();
        let height = texture.height();
        let bytes_per_row = width * 4;
        let buffer_size = (bytes_per_row * height) as u64;
        
        // Create staging buffer
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("NDI Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        
        // Copy texture to buffer
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("NDI Readback Encoder"),
        });
        
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        
        queue.submit(std::iter::once(encoder.finish()));
        
        // Map and read data
        let buffer_slice = staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result.is_ok());
        });
        
        // Poll until mapped
        device.poll(wgpu::PollType::Wait).ok();
        
        // Check if mapping succeeded and read data
        if rx.recv().ok()? {
            let data = buffer_slice.get_mapped_range();
            let bytes = data.to_vec();
            drop(data);
            staging_buffer.unmap();
            Some(bytes)
        } else {
            None
        }
    }

    /// Check if NDI is active
    pub fn is_ndi_active(&self) -> bool {
        self.ndi_output.is_some()
    }

    /// Check if Syphon is active (macOS only)
    #[cfg(target_os = "macos")]
    pub fn is_syphon_active(&self) -> bool {
        self.syphon_output.is_some()
    }

    #[cfg(not(target_os = "macos"))]
    pub fn is_syphon_active(&self) -> bool {
        false
    }

    /// Shutdown all outputs
    pub fn shutdown(&mut self) {
        self.stop_ndi();
        #[cfg(target_os = "macos")]
        self.stop_syphon();
    }
}

impl Default for OutputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OutputManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
