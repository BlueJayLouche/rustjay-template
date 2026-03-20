//! # V4L2 Loopback Output (Linux)
//!
//! Writes frames to a `/dev/videoN` loopback device created by the `v4l2loopback`
//! kernel module.  Other applications (OBS, ffplay, browsers) can read from the
//! virtual camera as if it were a real webcam.
//!
//! ## Implementation TODO
//!
//! 1. Uncomment `v4l = "0.14"` in Cargo.toml (see PORTING_GUIDE.md Step 2)
//! 2. Load the kernel module and create a virtual device first:
//!    ```bash
//!    sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="RustJay Output" exclusive_caps=1
//!    ```
//! 3. Replace the stub body of `new()` with real device configuration
//! 4. Replace `send_frame()` with the actual write; consider BGRA→BGR3 or BGRA→YUYV
//!    conversion if consuming apps don't understand BGRA.
//!
//! See PORTING_GUIDE.md §Linux — V4L2 for the complete step-by-step guide.

#![cfg(target_os = "linux")]

/// Writes BGRA frames to a V4L2 loopback device
pub struct V4l2LoopbackOutput {
    /// Path to the loopback device (e.g. "/dev/video10")
    device_path: String,
    width: u32,
    height: u32,
    // TODO (Linux): add device: v4l::Device (uncomment v4l dep in Cargo.toml first)
}

impl V4l2LoopbackOutput {
    /// Open the loopback device and configure its format.
    ///
    /// TODO (Linux): create a `v4l::Device`, set the pixel format (BGRA / BGR3 / YUYV),
    /// width, and height via `v4l::video::Output::set_format()`.
    pub fn new(device_path: &str, width: u32, height: u32) -> anyhow::Result<Self> {
        log::warn!(
            "V4l2LoopbackOutput::new({}) — not yet implemented ({}x{})",
            device_path,
            width,
            height
        );
        Ok(Self {
            device_path: device_path.to_string(),
            width,
            height,
        })
    }

    /// Write one frame of BGRA pixel data to the loopback device.
    ///
    /// The `bgra_data` slice must be exactly `width * height * 4` bytes.
    ///
    /// TODO (Linux): write the data (or a converted version) to the V4L2 device.
    /// Most consuming apps expect `BGR3` (24-bit BGR) or `YUYV` — convert as needed.
    pub fn send_frame(&mut self, _bgra_data: &[u8]) -> anyhow::Result<()> {
        log::warn!("V4l2LoopbackOutput::send_frame — not yet implemented ({})", self.device_path);
        Ok(())
    }

    /// Current output resolution
    pub fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
