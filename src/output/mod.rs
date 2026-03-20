//! # Output Module
//!
//! Video output to other applications via:
//! - NDI (cross-platform network)
//! - Syphon (macOS GPU texture sharing)
//! - Spout (Windows GPU texture sharing) - TODO
//! - v4l2loopback (Linux virtual camera) - TODO

use std::sync::Arc;

/// Commands for output stream control
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputCommand {
    None,
    StartNdi,
    StopNdi,
    #[cfg(target_os = "macos")]
    StartSyphon,
    #[cfg(target_os = "macos")]
    StopSyphon,
    #[cfg(target_os = "windows")]
    StartSpout { sender_name: String },
    #[cfg(target_os = "windows")]
    StopSpout,
    #[cfg(target_os = "linux")]
    StartV4l2 { device_path: String },
    #[cfg(target_os = "linux")]
    StopV4l2,
    ResizeOutput,
}

pub mod ndi_output;
#[cfg(target_os = "macos")]
pub mod syphon_output;
#[cfg(target_os = "windows")]
pub mod spout_output;
#[cfg(target_os = "linux")]
pub mod v4l2_output;

use ndi_output::NdiOutputSender;

/// Manages all video outputs
pub struct OutputManager {
    /// NDI network output
    ndi_output: Option<NdiOutputSender>,

    /// Syphon output (macOS)
    #[cfg(target_os = "macos")]
    syphon_output: Option<syphon_output::SyphonOutput>,

    /// Spout output (Windows) — TODO: replace () with real type from spout crate
    #[cfg(target_os = "windows")]
    spout_output: Option<spout_output::SpoutOutput>,

    /// V4L2 loopback output (Linux)
    #[cfg(target_os = "linux")]
    v4l2_output: Option<v4l2_output::V4l2LoopbackOutput>,

    frame_count: u64,
}

impl OutputManager {
    /// Create a new output manager
    pub fn new() -> Self {
        Self {
            ndi_output: None,
            #[cfg(target_os = "macos")]
            syphon_output: None,
            #[cfg(target_os = "windows")]
            spout_output: None,
            #[cfg(target_os = "linux")]
            v4l2_output: None,
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

    /// Start Spout output (Windows only)
    /// TODO (Windows): implement this using SpoutOutput in spout_output.rs
    #[cfg(target_os = "windows")]
    pub fn start_spout(
        &mut self,
        sender_name: &str,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> anyhow::Result<()> {
        let spout = spout_output::SpoutOutput::new(sender_name, device, queue)?;
        self.spout_output = Some(spout);
        log::info!("Spout output started: {}", sender_name);
        Ok(())
    }

    /// Stop Spout output (Windows only)
    #[cfg(target_os = "windows")]
    pub fn stop_spout(&mut self) {
        if self.spout_output.take().is_some() {
            log::info!("Spout output stopped");
        }
    }

    /// Check if Spout is active (Windows only)
    #[cfg(target_os = "windows")]
    pub fn is_spout_active(&self) -> bool {
        self.spout_output.is_some()
    }

    #[cfg(not(target_os = "windows"))]
    pub fn is_spout_active(&self) -> bool {
        false
    }

    /// Start V4L2 loopback output (Linux only)
    /// TODO (Linux): implement this using V4l2LoopbackOutput in v4l2_output.rs
    #[cfg(target_os = "linux")]
    pub fn start_v4l2(&mut self, device_path: &str, width: u32, height: u32) -> anyhow::Result<()> {
        let output = v4l2_output::V4l2LoopbackOutput::new(device_path, width, height)?;
        self.v4l2_output = Some(output);
        log::info!("V4L2 output started on {}", device_path);
        Ok(())
    }

    /// Stop V4L2 loopback output (Linux only)
    #[cfg(target_os = "linux")]
    pub fn stop_v4l2(&mut self) {
        if self.v4l2_output.take().is_some() {
            log::info!("V4L2 output stopped");
        }
    }

    /// Check if V4L2 is active (Linux only)
    #[cfg(target_os = "linux")]
    pub fn is_v4l2_active(&self) -> bool {
        self.v4l2_output.is_some()
    }

    #[cfg(not(target_os = "linux"))]
    pub fn is_v4l2_active(&self) -> bool {
        false
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

        // Spout output (zero-copy on Windows)
        #[cfg(target_os = "windows")]
        if let Some(ref mut spout) = self.spout_output {
            if let Err(e) = spout.submit_frame(texture, device, queue) {
                log::error!("Spout output error: {}", e);
            }
        }

        // V4L2 loopback output (CPU path on Linux)
        #[cfg(target_os = "linux")]
        if self.v4l2_output.is_some() {
            if let Some(data) = self.read_texture_bgra(texture, device, queue) {
                if let Some(ref mut v4l2) = self.v4l2_output {
                    if let Err(e) = v4l2.send_frame(&data) {
                        log::error!("V4L2 output error: {}", e);
                    }
                }
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
        #[cfg(target_os = "windows")]
        self.stop_spout();
        #[cfg(target_os = "linux")]
        self.stop_v4l2();
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
