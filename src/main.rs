//! # RustJay Template
//!
//! A high-performance video processing template for RustJay VJ applications.
//!
//! ## Features
//! - Single video input with hot-swappable sources (Webcam, NDI, Syphon)
//! - BGRA format throughout for native macOS performance
//! - HSB color manipulation (Hue, Saturation, Brightness)
//! - Real-time audio analysis with FFT and beat detection
//! - NDI and Syphon output
//! - Dual-window architecture (Control + Fullscreen Output)
//! - ImGui-based control interface
//!
//! ## Architecture
//! The application uses a dual-window architecture:
//! - **Control Window**: ImGui-based interface for adjusting settings
//! - **Output Window**: Fullscreen-capable display with hidden cursor
//!
//! ## Keyboard Shortcuts
//! - `Shift+F`: Toggle fullscreen on output window
//! - `Escape`: Exit application

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use env_logger;
use log::info;
use std::sync::{Arc, Mutex};

mod app;
mod audio;
mod config;
mod core;
mod engine;
mod gui;
mod input;
mod midi;
mod ndi_runtime;
mod osc;
mod output;
mod presets;
mod web;

use core::SharedState;

/// Application entry point
fn main() -> anyhow::Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_module("wgpu_hal::metal", log::LevelFilter::Warn)
        .filter_module("naga", log::LevelFilter::Warn)
        .filter_module("wgpu_core", log::LevelFilter::Warn)
        .filter_module("winit", log::LevelFilter::Warn)
        .filter_module("tracing::span", log::LevelFilter::Warn)
        .init();

    info!("Starting RustJay Template v{}", env!("CARGO_PKG_VERSION"));

    // Initialize NDI runtime path on Windows (no-op on other platforms)
    #[cfg(feature = "ndi")]
    if let Err(e) = ndi_runtime::init() {
        log::warn!("[NDI] Runtime initialization failed: {}", e);
        log::warn!("[NDI] NDI features may not work. Install NDI Tools from https://ndi.tv/tools/");
    }

    // Create shared state
    let shared_state = Arc::new(Mutex::new(SharedState::new()));

    // Run the application
    app::run_app(shared_state)?;

    Ok(())
}
