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
    
    // Track if initial device refresh has been done
    initial_refresh_done: bool,
}

impl App {
    fn new(shared_state: Arc<std::sync::Mutex<SharedState>>) -> Self {
        // Load settings and apply to state
        let config_manager = ConfigManager::new();
        
        // Apply loaded settings to shared state
        if let Ok(mut state) = shared_state.lock() {
            config_manager.settings.apply_to_state(&mut state);
            log::info!("Applied saved settings to state");
        }
        
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
            input_manager: None,
            audio_analyzer: None,
            midi_manager: None,
            osc_server: None,
            preset_bank: None,
            web_server: None,
            web_command_tx: None,
            config_manager,
            shift_pressed: false,
            initial_refresh_done: false,
        }
    }

    /// Toggle fullscreen on output window
    fn toggle_fullscreen(&mut self) {
        if let Some(ref output_window) = self.output_window {
            let mut state = self.shared_state.lock().unwrap();
            state.toggle_fullscreen();

            let fullscreen_mode = if state.output_fullscreen {
                Some(winit::window::Fullscreen::Borderless(None))
            } else {
                None
            };

            output_window.set_fullscreen(fullscreen_mode);
            output_window.set_cursor_visible(!state.output_fullscreen);
            log::info!("Fullscreen: {}", state.output_fullscreen);
        }
    }

    /// Process input commands
    fn process_input_commands(&mut self) {
        let command = {
            let mut state = self.shared_state.lock().unwrap();
            std::mem::replace(&mut state.input_command, InputCommand::None)
        };

        match command {
            InputCommand::StartWebcam {
                device_index,
                width,
                height,
                fps,
            } => {
                log::info!("Starting webcam: device={}", device_index);
                if let Some(ref mut manager) = self.input_manager {
                    match manager.start_webcam(device_index, width, height, fps) {
                        Ok(_) => {
                            let mut state = self.shared_state.lock().unwrap();
                            state.input.is_active = true;
                            state.input.input_type = crate::core::InputType::Webcam;
                            state.input.source_name = format!("Webcam {}", device_index);
                        }
                        Err(e) => log::error!("Failed to start webcam: {:?}", e),
                    }
                }
            }
            InputCommand::StartNdi { source_name } => {
                log::info!("Starting NDI: {}", source_name);
                if let Some(ref mut manager) = self.input_manager {
                    match manager.start_ndi(&source_name) {
                        Ok(_) => {
                            let mut state = self.shared_state.lock().unwrap();
                            state.input.is_active = true;
                            state.input.input_type = crate::core::InputType::Ndi;
                            state.input.source_name = source_name;
                        }
                        Err(e) => log::error!("Failed to start NDI: {:?}", e),
                    }
                }
            }
            #[cfg(target_os = "macos")]
            InputCommand::StartSyphon { server_name } => {
                log::info!("Starting Syphon: {}", server_name);
                if let Some(ref mut manager) = self.input_manager {
                    match manager.start_syphon(&server_name) {
                        Ok(_) => {
                            let mut state = self.shared_state.lock().unwrap();
                            state.input.is_active = true;
                            state.input.input_type = crate::core::InputType::Syphon;
                            state.input.source_name = server_name;
                        }
                        Err(e) => log::error!("Failed to start Syphon: {:?}", e),
                    }
                }
            }
            InputCommand::StopInput => {
                if let Some(ref mut manager) = self.input_manager {
                    manager.stop();
                    let mut state = self.shared_state.lock().unwrap();
                    state.input.is_active = false;
                    state.input.source_name.clear();
                }
            }
            InputCommand::RefreshDevices => {
                if let Some(ref mut manager) = self.input_manager {
                    manager.refresh_devices();
                    // Update GUI device lists
                    if let Some(ref mut gui) = self.control_gui {
                        gui.refresh_devices(manager);
                    }
                }
            }
            _ => {}
        }
    }

    /// Process output commands
    fn process_output_commands(&mut self) {
        let command = {
            let mut state = self.shared_state.lock().unwrap();
            std::mem::replace(&mut state.output_command, OutputCommand::None)
        };

        match command {
            OutputCommand::StartNdi => {
                if let Some(ref mut engine) = self.output_engine {
                    let (name, include_alpha) = {
                        let state = self.shared_state.lock().unwrap();
                        (state.ndi_output.stream_name.clone(), state.ndi_output.include_alpha)
                    };
                    if let Err(e) = engine.start_ndi_output(&name, include_alpha) {
                        log::error!("Failed to start NDI output: {:?}", e);
                    } else {
                        let mut state = self.shared_state.lock().unwrap();
                        state.ndi_output.is_active = true;
                    }
                }
            }
            OutputCommand::StopNdi => {
                if let Some(ref mut engine) = self.output_engine {
                    engine.stop_ndi_output();
                }
                let mut state = self.shared_state.lock().unwrap();
                state.ndi_output.is_active = false;
            }
            #[cfg(target_os = "macos")]
            OutputCommand::StartSyphon => {
                if let Some(ref mut engine) = self.output_engine {
                    let name = {
                        let state = self.shared_state.lock().unwrap();
                        state.syphon_output.server_name.clone()
                    };
                    if let Err(e) = engine.start_syphon_output(&name) {
                        log::error!("Failed to start Syphon output: {:?}", e);
                    } else {
                        let mut state = self.shared_state.lock().unwrap();
                        state.syphon_output.enabled = true;
                    }
                }
            }
            #[cfg(target_os = "macos")]
            OutputCommand::StopSyphon => {
                if let Some(ref mut engine) = self.output_engine {
                    engine.stop_syphon_output();
                }
                let mut state = self.shared_state.lock().unwrap();
                state.syphon_output.enabled = false;
            }
            OutputCommand::ResizeOutput => {
                // Resize output window if needed
                if let (Some(ref output_window), Some(ref mut engine)) = 
                    (self.output_window.as_ref(), self.output_engine.as_mut()) 
                {
                    let (new_width, new_height) = {
                        let state = self.shared_state.lock().unwrap();
                        (state.output_width, state.output_height)
                    };
                    
                    // Resize the wgpu surface
                    engine.resize(new_width, new_height);
                    
                    // Request window resize
                    let _ = output_window.request_inner_size(winit::dpi::LogicalSize::new(new_width, new_height));
                    
                    log::info!("Output resized to {}x{}", new_width, new_height);
                }
            }
            _ => {}
        }
    }

    /// Process audio commands
    fn process_audio_commands(&mut self) {
        let command = {
            let mut state = self.shared_state.lock().unwrap();
            std::mem::replace(&mut state.audio_command, AudioCommand::None)
        };

        match command {
            AudioCommand::RefreshDevices => {
                let devices = list_audio_devices();
                log::info!("[Audio] Refreshed devices: {} found", devices.len());
                let mut state = self.shared_state.lock().unwrap();
                state.audio.available_devices = devices;
            }
            AudioCommand::SelectDevice(device_name) => {
                log::info!("[Audio] Selecting device: {}", device_name);
                let mut state = self.shared_state.lock().unwrap();
                state.audio.selected_device = Some(device_name.clone());
                
                // Restart audio with new device if already running
                if let Some(ref mut analyzer) = self.audio_analyzer {
                    analyzer.stop();
                    if let Err(e) = analyzer.start_with_device(Some(&device_name)) {
                        log::error!("Failed to start audio with device '{}': {}", device_name, e);
                    } else {
                        log::info!("[Audio] Started with device: {}", device_name);
                    }
                }
            }
            AudioCommand::Start => {
                if let Some(ref mut analyzer) = self.audio_analyzer {
                    let device = {
                        let state = self.shared_state.lock().unwrap();
                        state.audio.selected_device.clone()
                    };
                    if let Err(e) = analyzer.start_with_device(device.as_deref()) {
                        log::error!("Failed to start audio: {}", e);
                    } else {
                        log::info!("[Audio] Analysis started");
                    }
                }
            }
            AudioCommand::Stop => {
                if let Some(ref mut analyzer) = self.audio_analyzer {
                    analyzer.stop();
                    log::info!("[Audio] Analysis stopped");
                }
            }
            _ => {}
        }
    }

    /// Update input and upload frames to GPU
    fn update_input(&mut self) {
        if let Some(ref mut manager) = self.input_manager {
            manager.update();

            // Handle Syphon texture (zero-copy path)
            #[cfg(target_os = "macos")]
            if manager.input_type() == InputType::Syphon {
                if let Some(texture) = manager.take_syphon_texture() {
                    let width = texture.width();
                    let height = texture.height();

                    if let Some(ref mut engine) = self.output_engine {
                        engine.input_texture.update_from_texture(&texture);
                    }

                    let mut state = self.shared_state.lock().unwrap();
                    state.input.width = width;
                    state.input.height = height;
                }
            } else {
                // CPU fallback path
                if let Some(frame_data) = manager.take_frame() {
                    let (width, height) = manager.resolution();

                    if let Some(ref mut engine) = self.output_engine {
                        engine.input_texture.update(&frame_data, width, height);
                    }

                    let mut state = self.shared_state.lock().unwrap();
                    state.input.width = width;
                    state.input.height = height;
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                if let Some(frame_data) = manager.take_frame() {
                    let (width, height) = manager.resolution();

                    if let Some(ref mut engine) = self.output_engine {
                        engine.input_texture.update(&frame_data, width, height);
                    }

                    let mut state = self.shared_state.lock().unwrap();
                    state.input.width = width;
                    state.input.height = height;
                }
            }
        }
    }

    /// Trigger tap tempo from keyboard shortcut
    fn trigger_tap_tempo(&mut self) {
        if let Some(ref mut gui) = self.control_gui {
            gui.handle_tap_tempo();
            log::info!("Tap tempo triggered via keyboard");
        }
    }

    /// Update audio analysis
    fn update_audio(&mut self) {
        // Sync settings from shared state TO analyzer
        if let Some(ref analyzer) = self.audio_analyzer {
            let (amplitude, smoothing, normalize, pink_noise) = {
                let state = self.shared_state.lock().unwrap();
                (state.audio.amplitude, state.audio.smoothing, state.audio.normalize, state.audio.pink_noise_shaping)
            };
            
            analyzer.set_amplitude(amplitude);
            analyzer.set_smoothing(smoothing);
            analyzer.set_normalize(normalize);
            analyzer.set_pink_noise_shaping(pink_noise);
        }
        
        // Read analysis results FROM analyzer TO shared state
        if let Some(ref analyzer) = self.audio_analyzer {
            let fft = analyzer.get_fft();
            let volume = analyzer.get_volume();
            let beat = analyzer.is_beat();
            let phase = analyzer.get_beat_phase();

            let mut state = self.shared_state.lock().unwrap();
            if state.audio.enabled {
                state.audio.fft = fft;
                state.audio.volume = volume;
                state.audio.beat = beat;
                state.audio.beat_phase = phase;
                
                // Process audio routing (updates internal smoothed values)
                // Actual application of modulation happens in render step
                if state.audio_routing.enabled {
                    let delta_time = 1.0 / 60.0;
                    state.audio_routing.matrix.process(&fft, delta_time);
                }
            }
        }
    }

    /// Update LFO phases (modulation applied in final composite step)
    fn update_lfo(&mut self) {
        // Get current BPM and beat phase from audio state
        let (bpm, beat_phase) = {
            let state = self.shared_state.lock().unwrap();
            (state.audio.bpm, state.audio.beat_phase)
        };
        
        // Assume 60fps for delta time
        let delta_time = 1.0 / 60.0;
        
        let mut state = self.shared_state.lock().unwrap();
        
        // Update LFO phases only - don't write to hsb_params here
        state.lfo.bank.update(bpm, delta_time, beat_phase);
    }

    /// Update preview textures for GUI
    fn update_preview_textures(&mut self) {
        if let (Some(ref mut renderer), Some(ref gui)) =
            (self.imgui_renderer.as_mut(), self.control_gui.as_ref())
        {
            // Update input preview
            if let Some(input_tex) = self.output_engine.as_ref().and_then(|e| e.input_texture.texture.as_ref()) {
                if let Some(preview_id) = gui.input_preview_texture_id {
                    let mut encoder = renderer.device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Preview Update Encoder"),
                    });
                    renderer.update_preview_texture(preview_id, &input_tex.texture, &mut encoder);
                    renderer.queue().submit(std::iter::once(encoder.finish()));
                }
            }

            // Update output preview
            if let Some(output_tex) = self.output_engine.as_ref().map(|e| &e.render_target) {
                if let Some(preview_id) = gui.output_preview_texture_id {
                    let mut encoder = renderer.device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Preview Update Encoder"),
                    });
                    renderer.update_preview_texture(preview_id, &output_tex.texture, &mut encoder);
                    renderer.queue().submit(std::iter::once(encoder.finish()));
                }
            }
        }
    }

    /// Process MIDI commands
    fn process_midi_commands(&mut self) {
        let command = {
            let mut state = self.shared_state.lock().unwrap();
            std::mem::replace(&mut state.midi_command, MidiCommand::None)
        };

        match command {
            MidiCommand::RefreshDevices => {
                if let Some(ref mut manager) = self.midi_manager {
                    let devices = manager.refresh_devices();
                    log::info!("MIDI devices refreshed: {} found", devices.len());
                }
            }
            MidiCommand::SelectDevice(device_name) => {
                if let Some(ref mut manager) = self.midi_manager {
                    if let Err(e) = manager.connect(&device_name) {
                        log::error!("Failed to connect to MIDI device '{}': {}", device_name, e);
                    }
                }
            }
            MidiCommand::StartLearn { param_path, param_name } => {
                if let Some(ref mut manager) = self.midi_manager {
                    manager.start_learn(&param_path, &param_name);
                }
            }
            MidiCommand::CancelLearn => {
                if let Some(ref mut manager) = self.midi_manager {
                    manager.cancel_learn();
                }
            }
            MidiCommand::ClearMappings => {
                if let Some(ref mut manager) = self.midi_manager {
                    if let Ok(mut state) = manager.state().lock() {
                        state.mappings.clear();
                        log::info!("MIDI mappings cleared");
                    }
                }
            }
            _ => {}
        }
    }

    /// Process OSC commands
    fn process_osc_commands(&mut self) {
        let command = {
            let mut state = self.shared_state.lock().unwrap();
            std::mem::replace(&mut state.osc_command, OscCommand::None)
        };

        match command {
            OscCommand::Start => {
                if let Some(ref mut server) = self.osc_server {
                    if let Err(e) = server.start() {
                        log::error!("Failed to start OSC server: {}", e);
                    } else {
                        // Update shared state
                        if let Ok(mut state) = self.shared_state.lock() {
                            state.osc_enabled = true;
                        }
                        log::info!("OSC server started");
                    }
                }
            }
            OscCommand::Stop => {
                if let Some(ref mut server) = self.osc_server {
                    server.stop();
                    // Update shared state
                    if let Ok(mut state) = self.shared_state.lock() {
                        state.osc_enabled = false;
                    }
                    log::info!("OSC server stopped");
                }
            }
            OscCommand::SetPort(port) => {
                if let Some(ref mut server) = self.osc_server {
                    // Stop if running
                    server.stop();
                    // Create new server with new port
                    let mut new_server = OscServer::new(port, "/rustjay");
                    if let Ok(mut state) = new_server.state().lock() {
                        state.register_default_parameters();
                    }
                    *server = new_server;
                    // Update shared state
                    if let Ok(mut state) = self.shared_state.lock() {
                        state.osc_port = port;
                        state.osc_enabled = false; // Reset to stopped
                    }
                    log::info!("OSC server port changed to {}", port);
                }
            }
            OscCommand::RefreshAddresses => {
                if let Some(ref mut server) = self.osc_server {
                    if let Ok(mut state) = server.state().lock() {
                        state.register_default_parameters();
                    }
                }
            }
            _ => {}
        }
    }

    /// Process preset commands
    fn process_preset_commands(&mut self) {
        let command = {
            let mut state = self.shared_state.lock().unwrap();
            std::mem::replace(&mut state.preset_command, PresetCommand::None)
        };

        match command {
            PresetCommand::Save { name } => {
                if let Some(ref mut bank) = self.preset_bank {
                    let state = self.shared_state.lock().unwrap();
                    let preset = crate::presets::Preset::from_state(&name, &state);
                    match bank.add_preset(preset) {
                        Ok(index) => log::info!("Saved preset '{}' at index {}", name, index),
                        Err(e) => log::error!("Failed to save preset: {}", e),
                    }
                }
            }
            PresetCommand::Load(index) => {
                if let Some(ref mut bank) = self.preset_bank {
                    let mut state = self.shared_state.lock().unwrap();
                    if let Err(e) = bank.apply_preset(index, &mut state) {
                        log::error!("Failed to load preset: {}", e);
                    }
                }
            }
            PresetCommand::Delete(index) => {
                if let Some(ref mut bank) = self.preset_bank {
                    if let Err(e) = bank.delete_preset(index) {
                        log::error!("Failed to delete preset: {}", e);
                    }
                }
            }
            PresetCommand::ApplySlot(slot) => {
                if let Some(ref mut bank) = self.preset_bank {
                    let mut state = self.shared_state.lock().unwrap();
                    if let Err(e) = bank.apply_slot(slot, &mut state) {
                        log::warn!("Failed to apply preset slot {}: {}", slot, e);
                    }
                }
            }
            PresetCommand::Refresh => {
                if let Some(ref mut bank) = self.preset_bank {
                    if let Err(e) = bank.refresh() {
                        log::error!("Failed to refresh presets: {}", e);
                    }
                }
            }
            _ => {}
        }
    }

    /// Process web server commands
    fn process_web_commands(&mut self) {
        let command = {
            let mut state = self.shared_state.lock().unwrap();
            std::mem::replace(&mut state.web_command, WebCommand::None)
        };

        match command {
            WebCommand::Start => {
                if let Some(ref mut server) = self.web_server {
                    if let Err(e) = server.start() {
                        log::error!("Failed to start web server: {}", e);
                    } else {
                        if let Ok(mut state) = self.shared_state.lock() {
                            state.web_enabled = true;
                        }
                        log::info!("Web server started at {}", server.get_url());
                    }
                }
            }
            WebCommand::Stop => {
                if let Some(ref mut server) = self.web_server {
                    server.stop();
                    if let Ok(mut state) = self.shared_state.lock() {
                        state.web_enabled = false;
                    }
                    log::info!("Web server stopped");
                }
            }
            WebCommand::SetPort(port) => {
                if let Some(ref mut server) = self.web_server {
                    server.stop();
                    // Create new server with new port
                    let config = WebConfig {
                        port,
                        app_name: "rustjay".to_string(),
                        enabled: false,
                    };
                    let (new_server, cmd_tx) = WebServer::new(config);
                    *server = new_server;
                    self.web_command_tx = Some(cmd_tx);
                    if let Ok(mut state) = self.shared_state.lock() {
                        state.web_port = port;
                        state.web_enabled = false;
                    }
                    log::info!("Web server port changed to {}", port);
                }
            }
            _ => {}
        }
        
        // Process commands from web clients
        if let Some(ref mut server) = self.web_server {
            while let Ok(cmd) = server.command_rx.try_recv() {
                match cmd {
                    WebServerCommand::Set { id, value } => {
                        // Apply the parameter change
                        if let Ok(mut state) = self.shared_state.lock() {
                            match id.as_str() {
                                "color/hue_shift" => {
                                    state.hsb_params.hue_shift = value.clamp(-180.0, 180.0);
                                    let (h, s, b) = (state.hsb_params.hue_shift, state.hsb_params.saturation, state.hsb_params.brightness);
                                    state.audio_routing.update_base_values(h, s, b);
                                }
                                "color/saturation" => {
                                    state.hsb_params.saturation = value.clamp(0.0, 2.0);
                                    let (h, s, b) = (state.hsb_params.hue_shift, state.hsb_params.saturation, state.hsb_params.brightness);
                                    state.audio_routing.update_base_values(h, s, b);
                                }
                                "color/brightness" => {
                                    state.hsb_params.brightness = value.clamp(0.0, 2.0);
                                    let (h, s, b) = (state.hsb_params.hue_shift, state.hsb_params.saturation, state.hsb_params.brightness);
                                    state.audio_routing.update_base_values(h, s, b);
                                }
                                "color/enabled" => state.color_enabled = value > 0.5,
                                "audio/amplitude" => state.audio.amplitude = value.clamp(0.0, 5.0),
                                "audio/smoothing" => state.audio.smoothing = value.clamp(0.0, 1.0),
                                "audio/enabled" => state.audio.enabled = value > 0.5,
                                "audio/normalize" => state.audio.normalize = value > 0.5,
                                "audio/pink_noise" => state.audio.pink_noise_shaping = value > 0.5,
                                "output/fullscreen" => {
                                    if value > 0.5 && !state.output_fullscreen {
                                        state.output_fullscreen = true;
                                    } else if value <= 0.5 && state.output_fullscreen {
                                        state.output_fullscreen = false;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    /// Update web server with current state
    fn update_web(&mut self) {
        if let Some(ref mut server) = self.web_server {
            if !server.is_running() {
                return;
            }
            
            // Sync current parameter values to web server
            if let Ok(state) = self.shared_state.lock() {
                server.update_parameter("color/hue_shift", state.hsb_params.hue_shift);
                server.update_parameter("color/saturation", state.hsb_params.saturation);
                server.update_parameter("color/brightness", state.hsb_params.brightness);
                server.update_parameter("color/enabled", if state.color_enabled { 1.0 } else { 0.0 });
                server.update_parameter("audio/amplitude", state.audio.amplitude);
                server.update_parameter("audio/smoothing", state.audio.smoothing);
                server.update_parameter("audio/enabled", if state.audio.enabled { 1.0 } else { 0.0 });
                server.update_parameter("audio/normalize", if state.audio.normalize { 1.0 } else { 0.0 });
                server.update_parameter("audio/pink_noise", if state.audio.pink_noise_shaping { 1.0 } else { 0.0 });
                server.update_parameter("output/fullscreen", if state.output_fullscreen { 1.0 } else { 0.0 });
            }
        }
    }

    /// Update MIDI - apply mapped values to state (only when changed)
    fn update_midi(&mut self) {
        if let Some(ref manager) = self.midi_manager {
            // Collect only dirty values
            let mut dirty_values: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
            
            {
                let midi_state_arc = manager.state();
                let mut midi_state = midi_state_arc.lock().unwrap();
                
                for mapping in &mut midi_state.mappings {
                    if mapping.is_dirty() {
                        let value = mapping.get_scaled_value();
                        dirty_values.insert(mapping.param_path.clone(), value);
                    }
                }
            }
            
            // Now apply to shared state only if there are dirty values
            if !dirty_values.is_empty() {
                if let Ok(mut shared) = self.shared_state.lock() {
                    if let Some(&v) = dirty_values.get("color/hue_shift") {
                        shared.hsb_params.hue_shift = v.clamp(-180.0, 180.0);
                    }
                    if let Some(&v) = dirty_values.get("color/saturation") {
                        shared.hsb_params.saturation = v.clamp(0.0, 2.0);
                    }
                    if let Some(&v) = dirty_values.get("color/brightness") {
                        shared.hsb_params.brightness = v.clamp(0.0, 2.0);
                    }
                    if let Some(&v) = dirty_values.get("audio/amplitude") {
                        shared.audio.amplitude = v.clamp(0.0, 5.0);
                    }
                    if let Some(&v) = dirty_values.get("audio/smoothing") {
                        shared.audio.smoothing = v.clamp(0.0, 1.0);
                    }
                }
            }
        }
    }

    /// Update OSC - apply received values to state (only when changed)
    fn update_osc(&mut self) {
        if let Some(ref server) = self.osc_server {
            // Collect only dirty values
            let (hue_shift, saturation, brightness, color_enabled, amplitude, smoothing) = {
                if let Ok(mut osc_state) = server.state().lock() {
                    (
                        osc_state.get_value_if_dirty("/color/hue_shift"),
                        osc_state.get_value_if_dirty("/color/saturation"),
                        osc_state.get_value_if_dirty("/color/brightness"),
                        osc_state.get_value_if_dirty("/color/enabled"),
                        osc_state.get_value_if_dirty("/audio/amplitude"),
                        osc_state.get_value_if_dirty("/audio/smoothing"),
                    )
                } else {
                    (None, None, None, None, None, None)
                }
            };
            
            // Apply to shared state only if there are changes
            if hue_shift.is_some() || saturation.is_some() || brightness.is_some() || 
               color_enabled.is_some() || amplitude.is_some() || smoothing.is_some() {
                if let Ok(mut shared) = self.shared_state.lock() {
                    if let Some(v) = hue_shift {
                        shared.hsb_params.hue_shift = v.clamp(-180.0, 180.0);
                    }
                    if let Some(v) = saturation {
                        shared.hsb_params.saturation = v.clamp(0.0, 2.0);
                    }
                    if let Some(v) = brightness {
                        shared.hsb_params.brightness = v.clamp(0.0, 2.0);
                    }
                    if let Some(v) = color_enabled {
                        shared.color_enabled = v > 0.5;
                    }
                    if let Some(v) = amplitude {
                        shared.audio.amplitude = v.clamp(0.0, 5.0);
                    }
                    if let Some(v) = smoothing {
                        shared.audio.smoothing = v.clamp(0.0, 1.0);
                    }
                }
            }
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

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create wgpu instance
        if self.wgpu_instance.is_none() {
            self.wgpu_instance = Some(wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            }));
        }
        let instance = self.wgpu_instance.as_ref().unwrap();

        // Create output window
        if self.output_window.is_none() {
            let (output_width, output_height, fullscreen) = {
                let state = self.shared_state.lock().unwrap();
                (state.output_width, state.output_height, state.output_fullscreen)
            };

            let window_attrs = WindowAttributes::default()
                .with_title("RustJay Output")
                .with_inner_size(winit::dpi::LogicalSize::new(output_width, output_height))
                .with_resizable(true)
                .with_decorations(true);

            let window = Arc::new(event_loop.create_window(window_attrs).unwrap());

            // Set fullscreen if needed
            if fullscreen {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
            window.set_cursor_visible(!fullscreen);

            self.output_window = Some(Arc::clone(&window));

            // Initialize output engine
            let shared_state = Arc::clone(&self.shared_state);

            match pollster::block_on(WgpuEngine::new(instance, window, shared_state)) {
                Ok(engine) => {
                    log::info!("Output engine initialized");
                    self.wgpu_adapter = Some(engine.adapter.clone());
                    self.wgpu_device = Some(Arc::clone(&engine.device));
                    self.wgpu_queue = Some(Arc::clone(&engine.queue));
                    self.output_engine = Some(engine);
                }
                Err(err) => {
                    log::error!("Failed to create output engine: {}", err);
                    event_loop.exit();
                    return;
                }
            }
        }

        // Create control window
        if self.control_window.is_none() {
            if let Some(ref engine) = self.output_engine {
                let device = Arc::clone(&engine.device);
                let queue = Arc::clone(&engine.queue);

                let window_attrs = WindowAttributes::default()
                    .with_title("RustJay Template - Control")
                    .with_inner_size(winit::dpi::LogicalSize::new(1200, 800))
                    .with_resizable(true)
                    .with_decorations(true);

                let window = Arc::new(event_loop.create_window(window_attrs).unwrap());
                self.control_window = Some(Arc::clone(&window));

                let adapter = self.wgpu_adapter.as_ref().unwrap();

                // Initialize ImGui renderer with correct scale factor
                let scale_factor = window.scale_factor();
                match pollster::block_on(ImGuiRenderer::new(
                    instance,
                    adapter,
                    device,
                    queue,
                    window,
                    scale_factor,
                )) {
                    Ok(mut renderer) => {
                        match ControlGui::new(Arc::clone(&self.shared_state)) {
                            Ok(mut gui) => {
                                // Create preview textures
                                let input_preview_id = renderer.create_preview_texture(1920, 1080);
                                let output_preview_id = renderer.create_preview_texture(1920, 1080);

                                gui.set_input_preview_texture(input_preview_id);
                                gui.set_output_preview_texture(output_preview_id);

                                log::info!("Created preview textures");

                                // Initial device refresh
                                if let Some(ref mut manager) = self.input_manager {
                                    gui.refresh_devices(manager);
                                }

                                self.control_gui = Some(gui);
                                self.imgui_renderer = Some(renderer);
                            }
                            Err(err) => {
                                log::error!("Failed to create control GUI: {}", err);
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("Failed to create ImGui renderer: {}", err);
                    }
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Handle output window events
        if let Some(ref output_window) = self.output_window {
            if window_id == output_window.id() {
                match event {
                    WindowEvent::CloseRequested => {
                        self.save_settings();
                        event_loop.exit();
                    }
                    WindowEvent::CursorEntered { .. } => {
                        let state = self.shared_state.lock().unwrap();
                        output_window.set_cursor_visible(!state.output_fullscreen);
                    }
                    WindowEvent::CursorLeft { .. } => {
                        output_window.set_cursor_visible(true);
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        // Track shift key
                        if let winit::keyboard::Key::Named(winit::keyboard::NamedKey::Shift) = &event.logical_key {
                            self.shift_pressed = event.state == winit::event::ElementState::Pressed;
                        }

                        if event.state == winit::event::ElementState::Pressed {
                            match &event.logical_key {
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) => {
                                    self.save_settings();
                                    event_loop.exit();
                                }
                                winit::keyboard::Key::Character(ch) => {
                                    let key = ch.to_lowercase();
                                    if self.shift_pressed && key == "f" {
                                        self.toggle_fullscreen();
                                    }
                                    if self.shift_pressed && key == "t" {
                                        self.trigger_tap_tempo();
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F1) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(1, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F2) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(2, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F3) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(3, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F4) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(4, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F5) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(5, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F6) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(6, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F7) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(7, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F8) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap();
                                            let _ = bank.apply_slot(8, &mut state);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    WindowEvent::Resized(size) => {
                        if let Some(ref mut engine) = self.output_engine {
                            engine.resize(size.width, size.height);
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if let Some(ref mut engine) = self.output_engine {
                            engine.render();
                            self.update_preview_textures();
                        }
                    }
                    _ => {}
                }
                return;
            }
        }

        // Handle control window events
        if let Some(ref control_window) = self.control_window {
            if window_id == control_window.id() {
                if let Some(ref mut renderer) = self.imgui_renderer {
                    let winit_event = winit::event::Event::WindowEvent { window_id, event: event.clone() };
                    renderer.handle_event(&winit_event);
                }

                match event {
                    WindowEvent::CloseRequested => {
                        self.save_settings();
                        event_loop.exit();
                    }
                    WindowEvent::Resized(size) => {
                        if let Some(ref mut renderer) = self.imgui_renderer {
                            renderer.resize(size.width, size.height);
                        }
                    }
                    WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                        if let Some(ref mut renderer) = self.imgui_renderer {
                            renderer.set_scale_factor(scale_factor);
                            // Update display size with new scale
                            let window_size = control_window.inner_size();
                            let logical_width = window_size.width as f32 / scale_factor as f32;
                            let logical_height = window_size.height as f32 / scale_factor as f32;
                            renderer.set_display_size(logical_width, logical_height);
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if let (Some(ref mut renderer), Some(ref mut gui)) =
                            (self.imgui_renderer.as_mut(), self.control_gui.as_mut())
                        {
                            let scale_factor = control_window.scale_factor();
                            let window_size = control_window.inner_size();
                            let logical_width = window_size.width as f32 / scale_factor as f32;
                            let logical_height = window_size.height as f32 / scale_factor as f32;
                            renderer.set_display_size(logical_width, logical_height);

                            if let Err(err) = renderer.render_frame(|ui| gui.build_ui(ui)) {
                                log::error!("ImGui render error: {}", err);
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Initialize input manager
        if self.input_manager.is_none() {
            let mut manager = InputManager::new();

            if let (Some(ref device), Some(ref queue)) = (&self.wgpu_device, &self.wgpu_queue) {
                manager.initialize(device, queue);
                log::info!("InputManager initialized");
            }

            self.input_manager = Some(manager);
        }
        
        // Do initial device refresh once both input manager and GUI are ready
        if !self.initial_refresh_done {
            if let (Some(ref mut manager), Some(ref mut gui)) = 
                (self.input_manager.as_mut(), self.control_gui.as_mut()) 
            {
                log::info!("Doing initial device refresh");
                gui.refresh_devices(manager);
                self.initial_refresh_done = true;
            }
        }

        // Initialize audio analyzer
        if self.audio_analyzer.is_none() {
            let mut analyzer = AudioAnalyzer::new();
            if let Err(e) = analyzer.start() {
                log::warn!("Failed to start audio analyzer: {}", e);
            } else {
                log::info!("Audio analyzer started");
            }
            self.audio_analyzer = Some(analyzer);
        }

        // Initialize MIDI manager
        if self.midi_manager.is_none() {
            let midi_state = std::sync::Arc::new(std::sync::Mutex::new(crate::midi::MidiState::default()));
            match MidiManager::new(midi_state) {
                Ok(mut manager) => {
                    // Refresh device list
                    manager.refresh_devices();
                    self.midi_manager = Some(manager);
                    log::info!("MIDI manager initialized");
                }
                Err(e) => {
                    log::warn!("Failed to initialize MIDI manager: {}", e);
                }
            }
        }

        // Initialize OSC server
        if self.osc_server.is_none() {
            let mut server = OscServer::new(9000, "/rustjay");
            // Register default parameters
            if let Ok(mut state) = server.state().lock() {
                state.register_default_parameters();
            }
            self.osc_server = Some(server);
            log::info!("OSC server initialized");
        }

        // Initialize preset bank
        if self.preset_bank.is_none() {
            match default_presets_dir() {
                Ok(presets_dir) => {
                    let bank = PresetBank::new(presets_dir);
                    self.preset_bank = Some(bank);
                    log::info!("Preset bank initialized");
                }
                Err(e) => {
                    log::warn!("Failed to initialize preset bank: {}", e);
                }
            }
        }

        // Initialize web server
        if self.web_server.is_none() {
            let port = {
                let state = self.shared_state.lock().unwrap();
                state.web_port
            };
            let config = WebConfig {
                port,
                app_name: "rustjay".to_string(),
                enabled: false,
            };
            let (mut server, cmd_tx) = WebServer::new(config);
            server.register_default_parameters();
            self.web_server = Some(server);
            self.web_command_tx = Some(cmd_tx);
            log::info!("Web server initialized on port {}", port);
        }

        // Process commands
        self.process_input_commands();
        self.process_output_commands();
        self.process_audio_commands();
        self.process_midi_commands();
        self.process_osc_commands();
        self.process_preset_commands();
        self.process_web_commands();

        // Update systems
        self.update_input();
        self.update_audio();
        self.update_lfo();
        self.update_midi();
        self.update_osc();
        self.update_web();
        
        // Check for settings save request
        let should_save = {
            let mut state = self.shared_state.lock().unwrap();
            if state.save_settings_requested {
                state.save_settings_requested = false;
                true
            } else {
                false
            }
        };
        if should_save {
            self.save_settings();
        }
        
        // Request redraws
        if let Some(ref window) = self.output_window {
            window.request_redraw();
        }
        if let Some(ref window) = self.control_window {
            window.request_redraw();
        }
    }
}
