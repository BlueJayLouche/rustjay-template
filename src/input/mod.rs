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

/// Commands for changing the active input source
#[derive(Debug, Clone, PartialEq)]
pub enum InputCommand {
    None,
    StartWebcam {
        device_index: usize,
        width: u32,
        height: u32,
        fps: u32,
    },
    StartNdi {
        source_name: String,
    },
    #[cfg(target_os = "macos")]
    StartSyphon {
        server_name: String,
        server_uuid: String,
    },
    #[cfg(target_os = "windows")]
    StartSpout {
        sender_name: String,
    },
    #[cfg(target_os = "linux")]
    StartV4l2 {
        device_path: String,
    },
    StopInput,
    RefreshDevices,
}

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

#[cfg(target_os = "windows")]
pub mod spout_input;
#[cfg(target_os = "windows")]
pub use spout_input::{SpoutInputReceiver, SpoutSenderInfo};

// Note: V4L2 input on Linux is handled by nokhwa (input-native maps to V4L2).
// A separate v4l2_input module is only needed if nokhwa proves insufficient.

/// Placeholder type on non-Windows platforms — the real struct lives in spout_input.rs
#[cfg(not(target_os = "windows"))]
#[derive(Debug, Clone)]
pub struct SpoutSenderInfo {
    pub name: String,
}

/// Placeholder type on non-macOS platforms — the real struct lives in syphon_input.rs
#[cfg(not(target_os = "macos"))]
#[derive(Debug, Clone)]
pub struct SyphonServerInfo {
    pub name: String,
    pub app_name: String,
    pub uuid: String,
}

use crate::core::InputType;

/// Frame data from any input source
pub struct InputFrame {
    pub width: u32,
    pub height: u32,
    /// BGRA pixel data
    pub data: Vec<u8>,
    pub timestamp: std::time::Instant,
}

/// Results returned from the background discovery thread
struct DiscoveryResults {
    webcam: Vec<String>,
    ndi: Vec<String>,
    #[cfg(target_os = "macos")]
    syphon: Vec<SyphonServerInfo>,
    #[cfg(target_os = "windows")]
    spout: Vec<SpoutSenderInfo>,
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
    syphon_device: Option<std::sync::Arc<wgpu::Device>>,
    #[cfg(target_os = "macos")]
    syphon_queue: Option<std::sync::Arc<wgpu::Queue>>,

    // Spout (Windows only) — TODO: replace () with real type from spout crate
    #[cfg(target_os = "windows")]
    spout_receiver: Option<SpoutInputReceiver>,

    // Current frame data (CPU path)
    current_frame: Option<Vec<u8>>,

    // Device lists — None = not yet discovered, Some([]) = discovered but none found
    webcam_devices: Option<Vec<String>>,
    ndi_sources: Option<Vec<String>>,
    #[cfg(target_os = "macos")]
    syphon_servers: Option<Vec<SyphonServerInfo>>,
    #[cfg(target_os = "windows")]
    spout_senders: Option<Vec<SpoutSenderInfo>>,

    // Background discovery
    discovery_rx: Option<mpsc::Receiver<DiscoveryResults>>,
    is_discovering: bool,
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
            syphon_device: None,
            #[cfg(target_os = "macos")]
            syphon_queue: None,
            #[cfg(target_os = "windows")]
            spout_receiver: None,
            current_frame: None,
            webcam_devices: None,
            ndi_sources: None,
            #[cfg(target_os = "macos")]
            syphon_servers: None,
            #[cfg(target_os = "windows")]
            spout_senders: None,
            discovery_rx: None,
            is_discovering: false,
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

    /// Get cached list of webcam devices (empty until discovery completes)
    pub fn webcam_devices(&self) -> &[String] {
        self.webcam_devices.as_deref().unwrap_or(&[])
    }

    /// Get cached list of NDI sources (empty until discovery completes)
    pub fn ndi_sources(&self) -> &[String] {
        self.ndi_sources.as_deref().unwrap_or(&[])
    }

    /// Get cached list of Syphon servers (macOS only; empty until discovery completes)
    #[cfg(target_os = "macos")]
    pub fn syphon_servers(&self) -> &[SyphonServerInfo] {
        self.syphon_servers.as_deref().unwrap_or(&[])
    }

    #[cfg(not(target_os = "macos"))]
    pub fn syphon_servers(&self) -> &[SyphonServerInfo] {
        &[]
    }

    /// Whether background discovery is currently in progress
    pub fn is_discovering(&self) -> bool {
        self.is_discovering
    }

    /// Begin async device discovery in a background thread.
    ///
    /// Returns immediately; call [`poll_discovery`](Self::poll_discovery) each frame
    /// to check when results are ready. Calling while a discovery is already in
    /// progress is a no-op.
    pub fn begin_refresh_devices(&mut self) {
        if self.is_discovering {
            return;
        }

        self.webcam_devices = None;
        self.ndi_sources = None;
        #[cfg(target_os = "macos")]
        {
            self.syphon_servers = None;
        }
        #[cfg(target_os = "windows")]
        {
            self.spout_senders = None;
        }

        self.is_discovering = true;
        let (tx, rx) = mpsc::channel();
        self.discovery_rx = Some(rx);

        std::thread::spawn(move || {
            #[cfg(feature = "webcam")]
            let webcam = {
                log::info!("[InputManager] Discovering webcam devices...");
                let devices = list_cameras();
                log::info!("[InputManager] Found {} webcam device(s)", devices.len());
                for d in &devices {
                    log::info!("  - {}", d);
                }
                devices
            };
            #[cfg(not(feature = "webcam"))]
            let webcam: Vec<String> = Vec::new();

            log::info!("[InputManager] Discovering NDI sources...");
            let ndi = list_ndi_sources(2000);
            log::info!("[InputManager] Found {} NDI source(s)", ndi.len());

            #[cfg(target_os = "macos")]
            let syphon = {
                log::info!("[InputManager] Discovering Syphon servers...");
                let servers = syphon_input::SyphonDiscovery::new().discover_servers();
                log::info!("[InputManager] Found {} Syphon server(s)", servers.len());
                servers
            };

            // TODO (Windows): implement Spout sender discovery
            #[cfg(target_os = "windows")]
            let spout = {
                log::info!("[InputManager] Discovering Spout senders...");
                let senders = spout_input::SpoutDiscovery::list_senders();
                log::info!("[InputManager] Found {} Spout sender(s)", senders.len());
                senders
            };

            let _ = tx.send(DiscoveryResults {
                webcam,
                ndi,
                #[cfg(target_os = "macos")]
                syphon,
                #[cfg(target_os = "windows")]
                spout,
            });
        });
    }

    /// Poll for background discovery completion.
    ///
    /// Returns `true` exactly once when discovery finishes — the caller should
    /// update any caches (e.g. GUI device lists) at that point.
    pub fn poll_discovery(&mut self) -> bool {
        if !self.is_discovering {
            return false;
        }
        let result = self.discovery_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = result {
            self.webcam_devices = Some(result.webcam);
            self.ndi_sources = Some(result.ndi);
            #[cfg(target_os = "macos")]
            {
                self.syphon_servers = Some(result.syphon);
            }
            #[cfg(target_os = "windows")]
            {
                self.spout_senders = Some(result.spout);
            }
            self.is_discovering = false;
            self.discovery_rx = None;
            true
        } else {
            false
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
    pub fn start_syphon(&mut self, server_name: impl Into<String>, server_uuid: impl Into<String>) -> Result<()> {
        let server_name = server_name.into();
        let server_uuid = server_uuid.into();

        let device = self.syphon_device.clone();
        let queue = self.syphon_queue.clone();

        if let (Some(device), Some(queue)) = (device, queue) {
            self.stop();

            let mut receiver = SyphonInputReceiver::new();
            receiver.initialize(&device, &queue);
            receiver.connect_by_uuid(&server_uuid, &server_name)?;

            self.input_type = InputType::Syphon;
            self.active = true;
            self.syphon_receiver = Some(receiver);

            log::info!("Started Syphon input: {} (uuid={})", server_name, server_uuid);
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

    /// Start Spout input (Windows only)
    /// TODO (Windows): implement this using the SpoutInputReceiver in spout_input.rs
    #[cfg(target_os = "windows")]
    pub fn start_spout(&mut self, sender_name: impl Into<String>) -> Result<()> {
        let sender_name = sender_name.into();
        self.stop();
        let mut receiver = SpoutInputReceiver::new();
        receiver.connect(&sender_name)?;
        self.input_type = crate::core::InputType::Spout;
        self.active = true;
        self.spout_receiver = Some(receiver);
        log::info!("Started Spout input: {}", sender_name);
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    pub fn start_spout(&mut self, _sender_name: impl Into<String>) -> Result<()> {
        Err(anyhow::anyhow!("Spout is only available on Windows"))
    }

    /// Get cached list of Spout senders (Windows only)
    #[cfg(target_os = "windows")]
    pub fn spout_senders(&self) -> &[SpoutSenderInfo] {
        self.spout_senders.as_deref().unwrap_or(&[])
    }

    #[cfg(not(target_os = "windows"))]
    pub fn spout_senders(&self) -> &[SpoutSenderInfo] {
        &[]
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
        }

        // Stop Spout
        #[cfg(target_os = "windows")]
        {
            self.spout_receiver = None;
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
        if let (Some(ref mut syphon), Some(ref device), Some(ref queue)) =
            (self.syphon_receiver.as_mut(), self.syphon_device.as_ref(), self.syphon_queue.as_ref())
        {
            if syphon.try_receive_texture(device, queue) {
                self.resolution = syphon.resolution();
                self.has_new_frame = true;
            }
        }

        // Handle Spout frames (CPU path on Windows — bytes → current_frame → InputTexture)
        #[cfg(target_os = "windows")]
        if let Some(ref mut spout) = self.spout_receiver {
            if spout.try_receive_texture() {
                self.resolution = spout.resolution();
                self.has_new_frame = true;
                // Move pixel bytes into current_frame so take_frame() / InputTexture::update() works
                if let Some(pixels) = spout.take_pixels() {
                    self.current_frame = Some(pixels);
                }
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

    /// Borrow the Syphon output texture (macOS only).
    ///
    /// Valid after [`update`](Self::update) sets [`has_frame`](Self::has_frame).
    /// Call [`clear_syphon_frame`](Self::clear_syphon_frame) after consuming it.
    #[cfg(target_os = "macos")]
    pub fn syphon_output_texture(&self) -> Option<&wgpu::Texture> {
        self.syphon_receiver.as_ref().and_then(|r| r.output_texture())
    }

    /// Reset the new-frame flag for the Syphon path.
    #[cfg(target_os = "macos")]
    pub fn clear_syphon_frame(&mut self) {
        self.has_new_frame = false;
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

    /// Returns true if the NDI source was lost (not found or too many errors)
    pub fn is_ndi_source_lost(&self) -> bool {
        self.ndi_receiver.as_ref().map(|r| r.is_source_lost()).unwrap_or(false)
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
