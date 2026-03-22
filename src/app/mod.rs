//! # Application Handler
//!
//! Dual-window application handler implementing winit's ApplicationHandler.

use crate::audio::{AudioAnalyzer, list_audio_devices};
use crate::config::{AppSettings, ConfigManager};
use crate::core::{AudioCommand, InputCommand, InputType, OutputCommand, SharedState, MidiCommand, OscCommand, PresetCommand, WebCommand};
use crate::engine::WgpuEngine;
use crate::gui::{ControlGui, ImGuiRenderer};
use crate::input::InputManager;
use crate::midi::{MidiManager, list_midi_devices};
use crate::osc::OscServer;
use crate::output::{OutputManager};
use crate::presets::{PresetBank, default_presets_dir};
use crate::web::{WebServer, WebConfig, WebCommand as WebServerCommand};

use anyhow::Result;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

/// Run the application
pub fn run_app(shared_state: Arc<std::sync::Mutex<SharedState>>) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(shared_state);
    event_loop.run_app(&mut app)?;

    Ok(())
}

/// Main application state
struct App {
    shared_state: Arc<std::sync::Mutex<SharedState>>,

    // Shared wgpu resources
    wgpu_instance: Option<wgpu::Instance>,
    wgpu_adapter: Option<wgpu::Adapter>,
    wgpu_device: Option<Arc<wgpu::Device>>,
    wgpu_queue: Option<Arc<wgpu::Queue>>,

    // Output window (fullscreen capable)
    output_window: Option<Arc<Window>>,
    output_engine: Option<WgpuEngine>,

    // Control window (ImGui)
    control_window: Option<Arc<Window>>,
    control_gui: Option<ControlGui>,
    imgui_renderer: Option<ImGuiRenderer>,

    // Input manager
    input_manager: Option<InputManager>,

    // Audio analyzer
    audio_analyzer: Option<AudioAnalyzer>,

    // MIDI manager
    midi_manager: Option<MidiManager>,

    // OSC server
    osc_server: Option<OscServer>,

    // Preset bank
    preset_bank: Option<PresetBank>,

    // Web server
    web_server: Option<WebServer>,
    web_command_tx: Option<tokio::sync::mpsc::Sender<WebServerCommand>>,

    // Config manager
    config_manager: ConfigManager,

    // Modifier state
    shift_pressed: bool,

    // Output window visibility — when occluded we skip the screen blit
    // but keep the shader pipeline and output sinks running at full speed.
    output_occluded: bool,

    // Frame timing for accurate delta-time
    last_frame_time: std::time::Instant,
    frame_delta_time: f32,
}

impl App {
    fn new(shared_state: Arc<std::sync::Mutex<SharedState>>) -> Self {
        // Load settings and apply to state
        let config_manager = ConfigManager::new();
        if let Ok(mut state) = shared_state.lock() {
            config_manager.settings.apply_to_state(&mut state);
            log::info!("Applied saved settings to state");
        }

        // Initialize audio analyzer
        let mut analyzer = AudioAnalyzer::new();
        if let Err(e) = analyzer.start() {
            log::warn!("Failed to start audio analyzer: {}", e);
        } else {
            log::info!("Audio analyzer started");
        }

        // Initialize MIDI manager
        let midi_manager = {
            let midi_state = std::sync::Arc::new(std::sync::Mutex::new(crate::midi::MidiState::default()));
            match MidiManager::new(midi_state) {
                Ok(mut manager) => {
                    manager.refresh_devices();
                    log::info!("MIDI manager initialized");
                    Some(manager)
                }
                Err(e) => {
                    log::warn!("Failed to initialize MIDI manager: {}", e);
                    None
                }
            }
        };

        // Initialize OSC server
        let osc_server = {
            let mut server = OscServer::new(9000, "/rustjay");
            if let Ok(mut state) = server.state().lock() {
                state.register_default_parameters();
            }
            log::info!("OSC server initialized");
            Some(server)
        };

        // Initialize preset bank
        let preset_bank = match default_presets_dir() {
            Ok(presets_dir) => {
                log::info!("Preset bank initialized");
                Some(PresetBank::new(presets_dir))
            }
            Err(e) => {
                log::warn!("Failed to initialize preset bank: {}", e);
                None
            }
        };

        // Initialize web server
        let web_port = shared_state.lock().unwrap_or_else(|e| e.into_inner()).web_port;
        let (web_server, web_command_tx) = {
            let config = WebConfig {
                port: web_port,
                app_name: "rustjay".to_string(),
                enabled: false,
            };
            let (mut server, cmd_tx) = WebServer::new(config);
            server.register_default_parameters();
            log::info!("Web server initialized on port {}", web_port);
            (Some(server), Some(cmd_tx))
        };

        Self {
            shared_state,
            wgpu_instance: None,
            wgpu_adapter: None,
            wgpu_device: None,
            wgpu_queue: None,
            output_window: None,
            output_engine: None,
            control_window: None,
            control_gui: None,
            imgui_renderer: None,
            input_manager: Some(InputManager::new()),
            audio_analyzer: Some(analyzer),
            midi_manager,
            osc_server,
            preset_bank,
            web_server,
            web_command_tx,
            config_manager,
            shift_pressed: false,
            output_occluded: false,
            last_frame_time: std::time::Instant::now(),
            frame_delta_time: 1.0 / 60.0,
        }
    }

    /// Toggle fullscreen on output window
    fn toggle_fullscreen(&mut self) {
        if let Some(ref output_window) = self.output_window {
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
            state.toggle_fullscreen();

            let fullscreen_mode = if state.output_fullscreen {
                Some(winit::window::Fullscreen::Borderless(None))
            } else {
                None
            };

            output_window.set_fullscreen(fullscreen_mode);
            output_window.set_cursor_visible(false);
            log::info!("Fullscreen: {}", state.output_fullscreen);
        }
    }

    /// Trigger tap tempo from keyboard shortcut
    fn trigger_tap_tempo(&mut self) {
        if let Some(ref mut gui) = self.control_gui {
            gui.handle_tap_tempo();
            log::info!("Tap tempo triggered via keyboard");
        }
    }

    /// Save settings on exit
    fn save_settings(&mut self) {
        // Update settings from current state
        if let Ok(state) = self.shared_state.lock() {
            self.config_manager.settings = AppSettings::from_state(&state);
        }

        // Save to disk
        if let Err(e) = self.config_manager.save() {
            log::error!("Failed to save settings: {}", e);
        } else {
            log::info!("Settings saved");
        }
    }
}

mod commands;
mod update;
mod events;
