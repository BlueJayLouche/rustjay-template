//! # Input Module
//!
//! Handles video input sources:
//! - Webcam capture (via nokhwa)
//! - NDI input (Network Device Interface)
//! - Syphon input (macOS GPU texture sharing)
//!
//! All inputs are converted to BGRA format for native macOS compatibility.

use anyhow::Result;
use std::sync::mpsc;

pub mod ndi;
pub use ndi::{list_ndi_sources, NdiReceiver, NdiFrame};

#[cfg(feature = "webcam")]
pub mod webcam;
#[cfg(feature = "webcam")]
pub use webcam::{list_cameras, WebcamCapture, WebcamFrame};

#[cfg(target_os = "macos")]
pub mod syphon_input;
#[cfg(target_os = "macos")]
pub use syphon_input::{SyphonInputReceiver, SyphonServerInfo};

use crate::core::InputType;

/// Frame data from any input source
pub struct InputFrame {
    pub width: u32,
    pub height: u32,
    /// BGRA pixel data
    pub data: Vec<u8>,
    pub timestamp: std::time::Instant,
}

/// Manages a single video input source with hot-swappable backends
pub struct InputManager {
    /// Current input type
    input_type: InputType,
    /// Whether input is active
    active: bool,
    /// Has new frame available
    has_new_frame: bool,
    /// Current resolution
    resolution: (u32, u32),

    // Input backends
    #[cfg(feature = "webcam")]
    webcam: Option<WebcamCapture>,
    #[cfg(not(feature = "webcam"))]
    webcam: Option<()>,
    frame_receiver: Option<mpsc::Receiver<WebcamFrame>>,
    ndi_receiver: Option<NdiReceiver>,

    // Syphon (macOS only)
    #[cfg(target_os = "macos")]
    syphon_receiver: Option<SyphonInputReceiver>,
    #[cfg(target_os = "macos")]
    syphon_texture: Option<wgpu::Texture>,
    #[cfg(target_os = "macos")]
    syphon_device: Option<std::sync::Arc<wgpu::Device>>,
    #[cfg(target_os = "macos")]
    syphon_queue: Option<std::sync::Arc<wgpu::Queue>>,

    // Current frame data (CPU path)
    current_frame: Option<Vec<u8>>,

    // Device lists
    webcam_devices: Vec<String>,
    ndi_sources: Vec<String>,
    #[cfg(target_os = "macos")]
    syphon_servers: Vec<String>,
}

impl InputManager {
    /// Create a new input manager
    pub fn new() -> Self {
        Self {
            input_type: InputType::None,
            active: false,
            has_new_frame: false,
            resolution: (1920, 1080),
            #[cfg(feature = "webcam")]
            webcam: None,
            #[cfg(not(feature = "webcam"))]
            webcam: None,
            frame_receiver: None,
            ndi_receiver: None,
            #[cfg(target_os = "macos")]
            syphon_receiver: None,
            #[cfg(target_os = "macos")]
            syphon_texture: None,
            #[cfg(target_os = "macos")]
            syphon_device: None,
            #[cfg(target_os = "macos")]
            syphon_queue: None,
            current_frame: None,
            webcam_devices: Vec::new(),
            ndi_sources: Vec::new(),
            #[cfg(target_os = "macos")]
            syphon_servers: Vec::new(),
        }
    }

    /// Initialize with wgpu device/queue (required for Syphon on macOS)
    pub fn initialize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        #[cfg(target_os = "macos")]
        {
            self.syphon_device = Some(std::sync::Arc::new(device.clone()));
            self.syphon_queue = Some(std::sync::Arc::new(queue.clone()));
        }
    }

    /// Get list of available webcam devices
    pub fn webcam_devices(&mut self) -> &[String] {
        #[cfg(feature = "webcam")]
        if self.webcam_devices.is_empty() {
            self.webcam_devices = list_cameras();
        }
        &self.webcam_devices
    }

    /// Get list of available NDI sources
    pub fn ndi_sources(&mut self) -> &[String] {
        if self.ndi_sources.is_empty() {
            self.ndi_sources = list_ndi_sources(2000);
        }
        &self.ndi_sources
    }

    /// Get list of available Syphon servers (macOS only)
    #[cfg(target_os = "macos")]
    pub fn syphon_servers(&mut self) -> &[String] {
        if self.syphon_servers.is_empty() {
            let discovery = syphon_input::SyphonDiscovery::new();
            let servers = discovery.discover_servers();
            self.syphon_servers = servers
                .into_iter()
                .map(|s| s.display_name().to_string())
                .collect();
        }
        &self.syphon_servers
    }

    #[cfg(not(target_os = "macos"))]
    pub fn syphon_servers(&self) -> &[String] {
        &[]
    }

    /// Refresh all device lists
    pub fn refresh_devices(&mut self) {
        self.webcam_devices.clear();
        self.ndi_sources.clear();
        #[cfg(target_os = "macos")]
        {
            self.syphon_servers.clear();
        }
        // Re-populate on next access
        let _ = self.webcam_devices();
        let _ = self.ndi_sources();
        #[cfg(target_os = "macos")]
        {
            let _ = self.syphon_servers();
        }
    }

    /// Start webcam capture
    #[cfg(feature = "webcam")]
    pub fn start_webcam(&mut self, device_index: usize, width: u32, height: u32, fps: u32) -> Result<()> {
        self.stop();

        let mut webcam = WebcamCapture::new(device_index, width, height, fps)?;
        let receiver = webcam.start()?;

        self.input_type = InputType::Webcam;
        self.resolution = (width, height);
        self.active = true;
        self.webcam = Some(webcam);
        self.frame_receiver = Some(receiver);

        log::info!("Started webcam {} at {}x{}@{}fps", device_index, width, height, fps);
        Ok(())
    }

    /// Start webcam (placeholder when disabled)
    #[cfg(not(feature = "webcam"))]
    pub fn start_webcam(&mut self, _device_index: usize, _width: u32, _height: u32, _fps: u32) -> Result<()> {
        Err(anyhow::anyhow!("Webcam support not compiled. Enable the 'webcam' feature."))
    }

    /// Start NDI input
    pub fn start_ndi(&mut self, source_name: impl Into<String>) -> Result<()> {
        self.stop();

        let source_name = source_name.into();
        let mut ndi = NdiReceiver::new(source_name.clone());
        ndi.start()?;

        self.input_type = InputType::Ndi;
        self.active = true;
        self.ndi_receiver = Some(ndi);

        log::info!("Started NDI input: {}", source_name);
        Ok(())
    }

    /// Start Syphon input (macOS only)
    #[cfg(target_os = "macos")]
    pub fn start_syphon(&mut self, server_name: impl Into<String>) -> Result<()> {
        let server_name = server_name.into();

        let device = self.syphon_device.clone();
        let queue = self.syphon_queue.clone();

        if let (Some(device), Some(queue)) = (device, queue) {
            self.stop();

            let mut receiver = SyphonInputReceiver::new();
            receiver.initialize(&device, &queue);
            receiver.connect(&server_name)?;

            self.input_type = InputType::Syphon;
            self.active = true;
            self.syphon_receiver = Some(receiver);

            log::info!("Started Syphon input: {}", server_name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("InputManager not initialized with wgpu device/queue"))
        }
    }

    /// Start Syphon (stub on non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn start_syphon(&mut self, _server_name: impl Into<String>) -> Result<()> {
        Err(anyhow::anyhow!("Syphon is only available on macOS"))
    }

    /// Stop current input
    pub fn stop(&mut self) {
        if !self.active {
            return;
        }

        log::info!("Stopping input source ({:?})", self.input_type);

        self.active = false;
        self.has_new_frame = false;

        // Stop webcam
        #[cfg(feature = "webcam")]
        if let Some(mut webcam) = self.webcam.take() {
            let _ = webcam.stop();
        }

        // Stop NDI
        if let Some(mut ndi) = self.ndi_receiver.take() {
            ndi.stop();
        }

        // Stop Syphon
        #[cfg(target_os = "macos")]
        {
            self.syphon_receiver = None;
            self.syphon_texture = None;
        }

        self.frame_receiver = None;
        self.current_frame = None;
        self.input_type = InputType::None;
    }

    /// Update - poll for new frames
    pub fn update(&mut self) {
        if !self.active {
            return;
        }

        // Handle webcam frames
        if let Some(ref receiver) = self.frame_receiver {
            let mut latest_frame: Option<WebcamFrame> = None;
            // Drain the channel (keep only latest)
            while let Ok(frame) = receiver.try_recv() {
                latest_frame = Some(frame);
            }
            if let Some(frame) = latest_frame {
                self.resolution = (frame.width, frame.height);
                self.current_frame = Some(frame.data);
                self.has_new_frame = true;
            }
        }

        // Handle NDI frames
        if let Some(ref mut ndi) = self.ndi_receiver {
            if let Some(frame) = ndi.get_latest_frame() {
                self.resolution = (frame.width, frame.height);
                self.current_frame = Some(frame.data);
                self.has_new_frame = true;
            }
        }

        // Handle Syphon frames (zero-copy texture path)
        #[cfg(target_os = "macos")]
        if let Some(ref mut syphon) = self.syphon_receiver {
            if let Some(texture) = syphon.try_receive_texture() {
                self.resolution = (texture.width(), texture.height());
                self.syphon_texture = Some(texture);
                self.has_new_frame = true;
            }
        }
    }

    /// Check if there's a new frame available
    pub fn has_frame(&self) -> bool {
        self.has_new_frame
    }

    /// Take the current frame data (CPU path)
    pub fn take_frame(&mut self) -> Option<Vec<u8>> {
        self.has_new_frame = false;
        self.current_frame.take()
    }

    /// Take the Syphon texture (zero-copy path, macOS only)
    #[cfg(target_os = "macos")]
    pub fn take_syphon_texture(&mut self) -> Option<wgpu::Texture> {
        self.has_new_frame = false;
        self.syphon_texture.take()
    }

    /// Stub for non-macOS platforms
    #[cfg(not(target_os = "macos"))]
    pub fn take_syphon_texture(&mut self) -> Option<std::convert::Infallible> {
        None
    }

    /// Get current resolution
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }

    /// Get current input type
    pub fn input_type(&self) -> InputType {
        self.input_type
    }

    /// Check if input is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

// Placeholder types when webcam is disabled
#[cfg(not(feature = "webcam"))]
pub struct WebcamFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub timestamp: std::time::Instant,
}

#[cfg(not(feature = "webcam"))]
pub fn list_cameras() -> Vec<String> {
    Vec::new()
}
