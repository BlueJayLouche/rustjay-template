//! # NDI Input
//!
//! Network Device Interface video input receiver.

use anyhow::Result;
use grafton_ndi::recv::Recv;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// NDI frame data (BGRA format)
pub struct NdiFrame {
    pub width: u32,
    pub height: u32,
    /// BGRA pixel data
    pub data: Vec<u8>,
    pub timestamp: std::time::Instant,
}

/// NDI video receiver running on dedicated thread
pub struct NdiReceiver {
    source_name: String,
    running: bool,
    frame_receiver: Option<mpsc::Receiver<NdiFrame>>,
    _thread_handle: Option<thread::JoinHandle<()>>,
}

impl NdiReceiver {
    /// Create a new NDI receiver for the given source
    pub fn new(source_name: impl Into<String>) -> Self {
        Self {
            source_name: source_name.into(),
            running: false,
            frame_receiver: None,
            _thread_handle: None,
        }
    }

    /// Start receiving frames on a dedicated thread
    pub fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }

        // Create channel for frame delivery
        let (tx, rx) = mpsc::channel::<NdiFrame>();
        self.frame_receiver = Some(rx);

        let source_name = self.source_name.clone();

        // Spawn receiver thread
        let handle = thread::spawn(move || {
            ndi_receive_thread(source_name, tx);
        });

        self._thread_handle = Some(handle);
        self.running = true;

        Ok(())
    }

    /// Stop receiving frames
    pub fn stop(&mut self) {
        self.running = false;
        self.frame_receiver = None;
        self._thread_handle = None;
    }

    /// Get the latest frame if available
    pub fn get_latest_frame(&mut self) -> Option<NdiFrame> {
        self.frame_receiver.as_ref()?.try_recv().ok()
    }

    /// Check if receiver is running
    pub fn is_running(&self) -> bool {
        self.running
    }
}

/// NDI receive thread function
fn ndi_receive_thread(source_name: String, tx: mpsc::Sender<NdiFrame>) {
    // Initialize NDI
    let ndi = match grafton_ndi::Ndi::new() {
        Ok(ndi) => ndi,
        Err(e) => {
            log::error!("Failed to initialize NDI: {}", e);
            return;
        }
    };

    // Find the source
    let finder = ndi.create_finder();
    let sources = finder.wait_for_sources(Duration::from_secs(2));

    let source = sources.into_iter().find(|s| {
        s.name()
            .map(|n| n.to_string_lossy().contains(&source_name))
            .unwrap_or(false)
    });

    let source = match source {
        Some(s) => s,
        None => {
            log::error!("NDI source '{}' not found", source_name);
            return;
        }
    };

    log::info!("Connecting to NDI source: {:?}", source.name());

    // Create receiver
    let recv = Recv::new();
    let conn = recv.connect(&source);

    // Receive loop
    loop {
        // Check for disconnect
        if tx.send(NdiFrame {
            width: 1920,
            height: 1080,
            data: vec![0u8; 1920 * 1080 * 4],
            timestamp: std::time::Instant::now(),
        }).is_err() {
            break;
        }

        // Receive video frame
        match conn.capture_video(100) {
            Ok(Some(frame)) => {
                let width = frame.width() as u32;
                let height = frame.height() as u32;

                // Get frame data - NDI provides BGRA on macOS
                let data = frame.data().to_vec();

                let ndi_frame = NdiFrame {
                    width,
                    height,
                    data,
                    timestamp: std::time::Instant::now(),
                };

                if tx.send(ndi_frame).is_err() {
                    break;
                }
            }
            Ok(None) => {
                // No frame available, continue
            }
            Err(e) => {
                log::error!("NDI receive error: {}", e);
                thread::sleep(Duration::from_millis(10));
            }
        }

        thread::sleep(Duration::from_millis(1));
    }

    log::info!("NDI receive thread ended");
}

/// List available NDI sources on the network
pub fn list_ndi_sources(timeout_ms: u64) -> Vec<String> {
    let ndi = match grafton_ndi::Ndi::new() {
        Ok(ndi) => ndi,
        Err(e) => {
            log::warn!("NDI not available: {}", e);
            return Vec::new();
        }
    };

    let finder = ndi.create_finder();
    let sources = finder.wait_for_sources(Duration::from_millis(timeout_ms));

    sources
        .into_iter()
        .filter_map(|s| s.name().map(|n| n.to_string_lossy().to_string()))
        .collect()
}

/// Check if NDI is available on this system
pub fn is_ndi_available() -> bool {
    grafton_ndi::Ndi::new().is_ok()
}
