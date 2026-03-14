//! # Persistent Configuration
//!
//! Handles saving and loading application settings to disk.

use crate::core::{HsbParams, ResolutionState, SharedState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// MIDI mapping entry for persistence
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MidiMappingConfig {
    /// Controller number (CC)
    pub cc: u8,
    /// Channel (0-15)
    pub channel: u8,
    /// Parameter path (e.g., "color/hue_shift")
    pub param_path: String,
    /// Min value for output range
    pub min_value: f32,
    /// Max value for output range
    pub max_value: f32,
}

/// OSC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscConfig {
    /// UDP port to listen on
    pub port: u16,
    /// Whether OSC is enabled
    pub enabled: bool,
    /// Base address prefix (e.g., "/rustjay")
    pub base_address: String,
}

impl Default for OscConfig {
    fn default() -> Self {
        Self {
            port: 9000,
            enabled: false,
            base_address: "/rustjay".to_string(),
        }
    }
}

/// Persistent application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Window resolution
    pub output_width: u32,
    pub output_height: u32,
    
    /// Internal processing resolution
    pub internal_width: u32,
    pub internal_height: u32,
    
    /// Color parameters
    pub hsb_params: HsbParams,
    pub color_enabled: bool,
    
    /// Audio settings
    pub audio_enabled: bool,
    pub audio_amplitude: f32,
    pub audio_smoothing: f32,
    pub audio_normalize: bool,
    pub audio_pink_noise: bool,
    pub audio_device: Option<String>,
    
    /// NDI output settings
    pub ndi_stream_name: String,
    pub ndi_include_alpha: bool,
    
    /// Syphon output settings (macOS)
    #[cfg(target_os = "macos")]
    pub syphon_server_name: String,
    
    /// MIDI settings
    pub midi_enabled: bool,
    pub midi_device: Option<String>,
    pub midi_mappings: Vec<MidiMappingConfig>,
    
    /// OSC settings
    pub osc: OscConfig,
    
    /// UI settings
    pub ui_scale: f32,
    pub show_preview: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            output_width: 1920,
            output_height: 1080,
            internal_width: 1920,
            internal_height: 1080,
            hsb_params: HsbParams::default(),
            color_enabled: true,
            audio_enabled: true,
            audio_amplitude: 1.0,
            audio_smoothing: 0.5,
            audio_normalize: true,
            audio_pink_noise: false,
            audio_device: None,
            ndi_stream_name: "RustJay Output".to_string(),
            ndi_include_alpha: false,
            #[cfg(target_os = "macos")]
            syphon_server_name: "RustJay".to_string(),
            midi_enabled: false,
            midi_device: None,
            midi_mappings: Vec::new(),
            osc: OscConfig::default(),
            ui_scale: 1.0,
            show_preview: true,
        }
    }
}

impl AppSettings {
    /// Load settings from default config file
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            log::info!("No config file found at {:?}, using defaults", path);
            return Ok(Self::default());
        }
        
        let content = std::fs::read_to_string(&path)?;
        let settings: AppSettings = serde_json::from_str(&content)?;
        log::info!("Loaded settings from {:?}", path);
        Ok(settings)
    }
    
    /// Save settings to default config file
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path()?;
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        log::info!("Saved settings to {:?}", path);
        Ok(())
    }
    
    /// Get the default config file path
    pub fn config_path() -> anyhow::Result<PathBuf> {
        let dirs = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(dirs.join("rustjay").join("settings.json"))
    }
    
    /// Get presets directory path
    pub fn presets_dir() -> anyhow::Result<PathBuf> {
        let dirs = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(dirs.join("rustjay").join("presets"))
    }
    
    /// Apply settings to shared state
    pub fn apply_to_state(&self, state: &mut SharedState) {
        state.output_width = self.output_width;
        state.output_height = self.output_height;
        state.resolution.internal_width = self.internal_width;
        state.resolution.internal_height = self.internal_height;
        state.hsb_params = self.hsb_params;
        state.color_enabled = self.color_enabled;
        state.audio.enabled = self.audio_enabled;
        state.audio.amplitude = self.audio_amplitude;
        state.audio.smoothing = self.audio_smoothing;
        state.audio.normalize = self.audio_normalize;
        state.audio.pink_noise_shaping = self.audio_pink_noise;
        state.audio.selected_device = self.audio_device.clone();
        state.ndi_output.stream_name = self.ndi_stream_name.clone();
        state.ndi_output.include_alpha = self.ndi_include_alpha;
        #[cfg(target_os = "macos")]
        {
            state.syphon_output.server_name = self.syphon_server_name.clone();
        }
        state.ui_scale = self.ui_scale;
        state.show_preview = self.show_preview;
    }
    
    /// Extract settings from shared state
    pub fn from_state(state: &SharedState) -> Self {
        Self {
            output_width: state.output_width,
            output_height: state.output_height,
            internal_width: state.resolution.internal_width,
            internal_height: state.resolution.internal_height,
            hsb_params: state.hsb_params,
            color_enabled: state.color_enabled,
            audio_enabled: state.audio.enabled,
            audio_amplitude: state.audio.amplitude,
            audio_smoothing: state.audio.smoothing,
            audio_normalize: state.audio.normalize,
            audio_pink_noise: state.audio.pink_noise_shaping,
            audio_device: state.audio.selected_device.clone(),
            ndi_stream_name: state.ndi_output.stream_name.clone(),
            ndi_include_alpha: state.ndi_output.include_alpha,
            #[cfg(target_os = "macos")]
            syphon_server_name: state.syphon_output.server_name.clone(),
            midi_enabled: false, // Will be set separately
            midi_device: None,
            midi_mappings: Vec::new(),
            osc: OscConfig::default(),
            ui_scale: state.ui_scale,
            show_preview: state.show_preview,
        }
    }
}

/// Global config manager
pub struct ConfigManager {
    pub settings: AppSettings,
}

impl ConfigManager {
    pub fn new() -> Self {
        let settings = AppSettings::load().unwrap_or_default();
        Self { settings }
    }
    
    pub fn save(&self) -> anyhow::Result<()> {
        self.settings.save()
    }
}