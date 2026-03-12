//! # Webcam Input
//!
//! Local camera capture using nokhwa.

use anyhow::Result;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Webcam frame data (converted to BGRA)
pub struct WebcamFrame {
    pub width: u32,
    pub height: u32,
    /// BGRA pixel data
    pub data: Vec<u8>,
    pub timestamp: Instant,
}

/// Webcam capture running on dedicated thread
pub struct WebcamCapture {
    device_index: usize,
    width: u32,
    height: u32,
    fps: u32,
    running: bool,
}

impl WebcamCapture {
    /// Create a new webcam capture configuration
    pub fn new(device_index: usize, width: u32, height: u32, fps: u32) -> Result<Self> {
        Ok(Self {
            device_index,
            width,
            height,
            fps,
            running: false,
        })
    }

    /// Start capturing frames on a dedicated thread
    pub fn start(&mut self) -> Result<mpsc::Receiver<WebcamFrame>> {
        let (tx, rx) = mpsc::channel::<WebcamFrame>();

        let device_index = self.device_index;
        let target_width = self.width;
        let target_height = self.height;
        let target_fps = self.fps;

        thread::spawn(move || {
            webcam_capture_thread(device_index, target_width, target_height, target_fps, tx);
        });

        self.running = true;
        Ok(rx)
    }

    /// Stop capturing
    pub fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
}

/// Webcam capture thread
fn webcam_capture_thread(
    device_index: usize,
    target_width: u32,
    target_height: u32,
    target_fps: u32,
    tx: mpsc::Sender<WebcamFrame>,
) {
    // Open camera
    let index = CameraIndex::Index(device_index as u32);
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
        CameraFormat::new(
            Resolution::new(target_width, target_height),
            FrameFormat::MJPEG,
            target_fps,
        ),
    ));

    let mut camera = match Camera::new(index, format) {
        Ok(cam) => cam,
        Err(e) => {
            log::error!("Failed to open camera {}: {}", device_index, e);
            return;
        }
    };

    if let Err(e) = camera.open_stream() {
        log::error!("Failed to open camera stream: {}", e);
        return;
    }

    let actual_format = camera.camera_format();
    log::info!(
        "Camera opened at {}x{} @ {}fps",
        actual_format.resolution().width(),
        actual_format.resolution().height(),
        actual_format.frame_rate()
    );

    let width = actual_format.resolution().width();
    let height = actual_format.resolution().height();

    // Capture loop
    loop {
        match camera.frame() {
            Ok(buffer) => {
                // Convert RGB to BGRA
                let rgb_data = buffer.buffer();
                let mut bgra_data = Vec::with_capacity((width * height * 4) as usize);

                for chunk in rgb_data.chunks_exact(3) {
                    bgra_data.push(chunk[2]); // B
                    bgra_data.push(chunk[1]); // G
                    bgra_data.push(chunk[0]); // R
                    bgra_data.push(255);      // A
                }

                let frame = WebcamFrame {
                    width,
                    height,
                    data: bgra_data,
                    timestamp: Instant::now(),
                };

                if tx.send(frame).is_err() {
                    break;
                }
            }
            Err(e) => {
                log::warn!("Camera frame error: {}", e);
            }
        }

        // Frame rate limiting
        thread::sleep(Duration::from_millis(1000 / target_fps as u64));
    }

    let _ = camera.stop_stream();
    log::info!("Webcam capture thread ended");
}

/// List available camera devices
pub fn list_cameras() -> Vec<String> {
    use nokhwa::utils::query;

    match query(
        nokhwa::utils::ApiBackend::Auto,
    ) {
        Ok(cameras) => cameras
            .into_iter()
            .enumerate()
            .map(|(i, info)| {
                format!(
                    "{}: {} ({})",
                    i,
                    info.human_name(),
                    info.description()
                )
            })
            .collect(),
        Err(e) => {
            log::warn!("Failed to query cameras: {}", e);
            Vec::new()
        }
    }
}
