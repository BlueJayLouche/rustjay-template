//! # Shared State
//!
//! Thread-safe state shared between the GUI, renderer, and input/output threads.

use serde::{Deserialize, Serialize};

/// Type of video input source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputType {
    None,
    Webcam,
    Ndi,
    #[cfg(target_os = "macos")]
    Syphon,
}

impl Default for InputType {
    fn default() -> Self {
        InputType::None
    }
}

impl InputType {
    /// Get display name for UI
    pub fn name(&self) -> &'static str {
        match self {
            InputType::None => "None",
            InputType::Webcam => "Webcam",
            InputType::Ndi => "NDI",
            #[cfg(target_os = "macos")]
            InputType::Syphon => "Syphon",
        }
    }
}

/// Current state of the video input
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Type of active input
    pub input_type: InputType,
    /// Source name (NDI source, webcam device name, Syphon server)
    pub source_name: String,
    /// Whether input is active and receiving frames
    pub is_active: bool,
    /// Current resolution
    pub width: u32,
    pub height: u32,
    /// Frame rate (if known)
    pub fps: f32,
}

/// HSB color adjustment parameters
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HsbParams {
    /// Hue shift in degrees (-180 to 180)
    pub hue_shift: f32,
    /// Saturation multiplier (0 to 2, 1 = no change)
    pub saturation: f32,
    /// Brightness multiplier (0 to 2, 1 = no change)
    pub brightness: f32,
}

impl Default for HsbParams {
    fn default() -> Self {
        Self {
            hue_shift: 0.0,
            saturation: 1.0,
            brightness: 1.0,
        }
    }
}

impl HsbParams {
    /// Reset to default values
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Audio analysis state
#[derive(Debug, Clone, Default)]
pub struct AudioState {
    /// 8-band FFT values (normalized 0-1)
    pub fft: [f32; 8],
    /// Overall volume/energy (0-1)
    pub volume: f32,
    /// Beat detected this frame
    pub beat: bool,
    /// Estimated BPM
    pub bpm: f32,
    /// Beat phase (0-1)
    pub beat_phase: f32,
    /// Audio processing enabled
    pub enabled: bool,
    /// Amplitude multiplier
    pub amplitude: f32,
    /// Smoothing factor (0-1)
    pub smoothing: f32,
    /// Selected audio input device name
    pub selected_device: Option<String>,
    /// List of available audio devices
    pub available_devices: Vec<String>,
    /// Normalize FFT bands to maximum value
    pub normalize: bool,
    /// Apply +3dB per octave pink noise compensation
    pub pink_noise_shaping: bool,
}

/// NDI output state
#[derive(Debug, Clone, Default)]
pub struct NdiOutputState {
    /// Output stream name
    pub stream_name: String,
    /// Whether output is active
    pub is_active: bool,
    /// Include alpha channel
    pub include_alpha: bool,
}

/// Syphon output state (macOS only)
#[derive(Debug, Clone, Default)]
pub struct SyphonOutputState {
    /// Server name displayed to clients
    pub server_name: String,
    /// Whether output is enabled
    pub enabled: bool,
}

/// Commands for input changes
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
    },
    StopInput,
    RefreshDevices,
}

/// Commands for output control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputCommand {
    None,
    StartNdi,
    StopNdi,
    #[cfg(target_os = "macos")]
    StartSyphon,
    #[cfg(target_os = "macos")]
    StopSyphon,
}

/// Commands for audio control
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCommand {
    None,
    RefreshDevices,
    SelectDevice(String),
    Start,
    Stop,
}

/// Shared state accessible from multiple threads
#[derive(Debug)]
pub struct SharedState {
    // Output window settings
    pub output_fullscreen: bool,
    pub output_width: u32,
    pub output_height: u32,

    // Input state
    pub input: InputState,
    pub input_command: InputCommand,

    // Color adjustment
    pub hsb_params: HsbParams,
    pub color_enabled: bool,

    // Audio analysis
    pub audio: AudioState,
    pub audio_command: AudioCommand,

    // NDI Output
    pub ndi_output: NdiOutputState,
    pub output_command: OutputCommand,

    // Syphon Output (macOS)
    #[cfg(target_os = "macos")]
    pub syphon_output: SyphonOutputState,

    // UI state
    pub show_preview: bool,
    pub ui_scale: f32,

    // Current GUI tab
    pub current_tab: GuiTab,
}

/// GUI tab selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuiTab {
    #[default]
    Input,
    Color,
    Audio,
    Output,
    Settings,
}

impl GuiTab {
    /// Get display name for UI
    pub fn name(&self) -> &'static str {
        match self {
            GuiTab::Input => "Input",
            GuiTab::Color => "Color",
            GuiTab::Audio => "Audio",
            GuiTab::Output => "Output",
            GuiTab::Settings => "Settings",
        }
    }
}

impl SharedState {
    /// Create new shared state with default values
    pub fn new() -> Self {
        Self {
            output_fullscreen: false,
            output_width: 1920,
            output_height: 1080,

            input: InputState::default(),
            input_command: InputCommand::None,

            hsb_params: HsbParams::default(),
            color_enabled: true,

            audio: AudioState {
                enabled: true,
                amplitude: 1.0,
                smoothing: 0.5,
                normalize: true,
                pink_noise_shaping: false,
                ..Default::default()
            },
            audio_command: AudioCommand::None,

            ndi_output: NdiOutputState {
                stream_name: "RustJay Output".to_string(),
                is_active: false,
                include_alpha: false,
            },
            output_command: OutputCommand::None,

            #[cfg(target_os = "macos")]
            syphon_output: SyphonOutputState {
                server_name: "RustJay".to_string(),
                enabled: false,
            },

            show_preview: true,
            ui_scale: 1.0,

            current_tab: GuiTab::Input,
        }
    }

    /// Toggle fullscreen state
    pub fn toggle_fullscreen(&mut self) {
        self.output_fullscreen = !self.output_fullscreen;
    }

    /// Set output resolution
    pub fn set_output_resolution(&mut self, width: u32, height: u32) {
        self.output_width = width;
        self.output_height = height;
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}
