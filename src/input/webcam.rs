//! # Webcam Input
//!
//! Local camera capture using nokhwa.

use nokhwa::Camera;
use nokhwa::pixel_format::YuyvFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType, Resolution};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

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
    capture_thread: Option<JoinHandle<()>>,
    stop_signal: Option<Sender<()>>,
}

impl WebcamCapture {
    /// Create a new webcam capture configuration
    pub fn new(device_index: usize, width: u32, height: u32, fps: u32) -> anyhow::Result<Self> {
        let _ = (width, height, fps); // Unused for now
        Ok(Self {
            device_index,
            capture_thread: None,
            stop_signal: None,
        })
    }

    /// Start capturing frames on a dedicated thread
    pub fn start(&mut self) -> anyhow::Result<Receiver<WebcamFrame>> {
        if self.capture_thread.is_some() {
            return Err(anyhow::anyhow!("Webcam already started"));
        }

        let (frame_tx, frame_rx) = mpsc::channel::<WebcamFrame>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let device_index = self.device_index;

        let thread_handle = thread::spawn(move || {
            let index = CameraIndex::Index(device_index as u32);
            // Request YUYV format - most webcams support this natively
            let format = RequestedFormat::new::<YuyvFormat>(
                RequestedFormatType::AbsoluteHighestResolution
            );

            let mut camera = match Camera::new(index, format) {
                Ok(cam) => cam,
                Err(e) => {
                    log::error!("[Webcam] Failed to open camera {}: {:?}", device_index, e);
                    return;
                }
            };

            if let Err(e) = camera.open_stream() {
                log::error!("[Webcam] Failed to open stream: {:?}", e);
                return;
            }

            let actual_resolution = camera.resolution();
            let actual_width = actual_resolution.width() as u32;
            let actual_height = actual_resolution.height() as u32;

            log::info!("[Webcam] Camera {} opened at {}x{}", 
                device_index, actual_width, actual_height);

            // Capture loop
            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                match camera.frame() {
                    Ok(frame) => {
                        let buffer = frame.buffer();
                        
                        // Convert YUY2 to BGRA
                        let yuyv_data = buffer.to_vec();
                        let mut bgra_data = Vec::with_capacity((actual_width * actual_height * 4) as usize);
                        
                        // YUY2 is 2 bytes per pixel, arranged as: Y0 U Y1 V
                        // Each pair of pixels shares U and V
                        for chunk in yuyv_data.chunks_exact(4) {
                            let y0 = chunk[0] as f32;
                            let u = chunk[1] as f32 - 128.0;
                            let y1 = chunk[2] as f32;
                            let v = chunk[3] as f32 - 128.0;
                            
                            // Convert first pixel (Y0, U, V)
                            let r0 = (y0 + 1.402 * v).clamp(0.0, 255.0) as u8;
                            let g0 = (y0 - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u8;
                            let b0 = (y0 + 1.772 * u).clamp(0.0, 255.0) as u8;
                            
                            // Convert second pixel (Y1, U, V)
                            let r1 = (y1 + 1.402 * v).clamp(0.0, 255.0) as u8;
                            let g1 = (y1 - 0.344136 * u - 0.714136 * v).clamp(0.0, 255.0) as u8;
                            let b1 = (y1 + 1.772 * u).clamp(0.0, 255.0) as u8;
                            
                            // Output as BGRA for first pixel
                            bgra_data.push(b0);
                            bgra_data.push(g0);
                            bgra_data.push(r0);
                            bgra_data.push(255); // Alpha
                            
                            // Output as BGRA for second pixel
                            bgra_data.push(b1);
                            bgra_data.push(g1);
                            bgra_data.push(r1);
                            bgra_data.push(255); // Alpha
                        }

                        let webcam_frame = WebcamFrame {
                            width: actual_width,
                            height: actual_height,
                            data: bgra_data,
                            timestamp: Instant::now(),
                        };

                        if frame_tx.send(webcam_frame).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        log::warn!("[Webcam] Frame capture error: {:?}", e);
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }

            let _ = camera.stop_stream();
            log::info!("[Webcam] Camera {} stopped", device_index);
        });

        self.capture_thread = Some(thread_handle);
        self.stop_signal = Some(stop_tx);

        Ok(frame_rx)
    }

    /// Stop capturing
    pub fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(stop_tx) = self.stop_signal.take() {
            let _ = stop_tx.send(());
        }

        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }

        Ok(())
    }
}

impl Drop for WebcamCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// List available webcam devices
pub fn list_cameras() -> Vec<String> {
    let mut cameras = Vec::new();

    for i in 0..4 {
        let index = CameraIndex::Index(i as u32);
        let format = RequestedFormat::new::<YuyvFormat>(
            RequestedFormatType::AbsoluteHighestResolution
        );

        match Camera::new(index, format) {
            Ok(cam) => {
                let name = cam.info().human_name();
                cameras.push(name);
            }
            Err(_) => {}
        }
    }

    cameras
}
