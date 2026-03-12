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

/// Manages all video outputs
pub struct OutputManager {
    /// NDI network output
    ndi_output: Option<ndi_output::NdiOutputSender>,

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
        let sender = ndi_output::NdiOutputSender::new(name, width, height, include_alpha)?;
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
        let mut syphon = syphon_output::SyphonOutput::new(server_name, device, queue)?;
        syphon.initialize(1920, 1080)?;
        self.syphon_output = Some(syphon);
        log::info!("Syphon output started: {}", server_name);
        Ok(())
    }

    /// Stop Syphon output (macOS only)
    #[cfg(target_os = "macos")]
    pub fn stop_syphon(&mut self) {
        if let Some(mut syphon) = self.syphon_output.take() {
            syphon.shutdown();
            log::info!("Syphon output stopped");
        }
    }

    /// Submit frame to all active outputs
    pub fn submit_frame(&mut self, texture: &wgpu::Texture, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.frame_count += 1;

        // NDI output
        if let Some(ref mut ndi) = self.ndi_output {
            // TODO: Implement NDI frame submission
            // This requires GPU readback which is complex
        }

        // Syphon output (zero-copy on macOS)
        #[cfg(target_os = "macos")]
        if let Some(ref mut syphon) = self.syphon_output {
            if let Err(e) = syphon.submit_frame(texture, device, queue) {
                log::error!("Syphon output error: {}", e);
            }
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
