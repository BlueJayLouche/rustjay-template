use super::App;
use crate::audio::list_audio_devices;
use crate::core::{AudioCommand, InputCommand, OutputCommand, MidiCommand, OscCommand, PresetCommand, SharedState, WebCommand};
use crate::osc::OscServer;
use crate::web::{WebServer, WebConfig, WebCommand as WebServerCommand};

/// Acquire a mutex lock, recovering from poisoning.
/// Uses a free function (not a method) so it only borrows the specific field.
fn lock(state: &std::sync::Mutex<SharedState>) -> std::sync::MutexGuard<SharedState> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

impl App {
    /// Dispatch all pending subsystem commands. Call once per frame.
    pub(super) fn dispatch_commands(&mut self) {
        self.process_input_commands();
        self.process_output_commands();
        self.process_audio_commands();
        self.process_midi_commands();
        self.process_osc_commands();
        self.process_preset_commands();
        self.process_web_commands();
    }

    fn process_input_commands(&mut self) {
        let command = std::mem::replace(&mut lock(&self.shared_state).input_command, InputCommand::None);

        match command {
            InputCommand::StartWebcam { device_index, width, height, fps } => {
                log::info!("Starting webcam: device={}", device_index);
                if let Some(ref mut manager) = self.input_manager {
                    match manager.start_webcam(device_index, width, height, fps) {
                        Ok(_) => {
                            let mut state = lock(&self.shared_state);
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
                            let mut state = lock(&self.shared_state);
                            state.input.is_active = true;
                            state.input.input_type = crate::core::InputType::Ndi;
                            state.input.source_name = source_name;
                        }
                        Err(e) => log::error!("Failed to start NDI: {:?}", e),
                    }
                }
            }
            #[cfg(target_os = "macos")]
            InputCommand::StartSyphon { server_name, server_uuid } => {
                log::info!("Starting Syphon: {} (uuid={})", server_name, server_uuid);
                if let Some(ref mut manager) = self.input_manager {
                    match manager.start_syphon(&server_name, &server_uuid) {
                        Ok(_) => {
                            let mut state = lock(&self.shared_state);
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
                    let mut state = lock(&self.shared_state);
                    state.input.is_active = false;
                    state.input.source_name.clear();
                }
            }
            InputCommand::RefreshDevices => {
                if let Some(ref mut manager) = self.input_manager {
                    manager.begin_refresh_devices();
                    lock(&self.shared_state).input_discovering = true;
                }
            }
            _ => {}
        }
    }

    fn process_output_commands(&mut self) {
        let command = std::mem::replace(&mut lock(&self.shared_state).output_command, OutputCommand::None);

        match command {
            OutputCommand::StartNdi => {
                if let Some(ref mut engine) = self.output_engine {
                    let (name, include_alpha) = {
                        let state = lock(&self.shared_state);
                        (state.ndi_output.stream_name.clone(), state.ndi_output.include_alpha)
                    };
                    if let Err(e) = engine.start_ndi_output(&name, include_alpha) {
                        log::error!("Failed to start NDI output: {:?}", e);
                    } else {
                        lock(&self.shared_state).ndi_output.is_active = true;
                    }
                }
            }
            OutputCommand::StopNdi => {
                if let Some(ref mut engine) = self.output_engine {
                    engine.stop_ndi_output();
                }
                lock(&self.shared_state).ndi_output.is_active = false;
            }
            #[cfg(target_os = "macos")]
            OutputCommand::StartSyphon => {
                if let Some(ref mut engine) = self.output_engine {
                    let name = lock(&self.shared_state).syphon_output.server_name.clone();
                    if let Err(e) = engine.start_syphon_output(&name) {
                        log::error!("Failed to start Syphon output: {:?}", e);
                    } else {
                        lock(&self.shared_state).syphon_output.enabled = true;
                    }
                }
            }
            #[cfg(target_os = "macos")]
            OutputCommand::StopSyphon => {
                if let Some(ref mut engine) = self.output_engine {
                    engine.stop_syphon_output();
                }
                lock(&self.shared_state).syphon_output.enabled = false;
            }
            OutputCommand::ResizeOutput => {
                if let (Some(ref output_window), Some(ref mut engine)) =
                    (self.output_window.as_ref(), self.output_engine.as_mut())
                {
                    let (new_width, new_height) = {
                        let state = lock(&self.shared_state);
                        (state.output_width, state.output_height)
                    };
                    engine.resize(new_width, new_height);
                    let _ = output_window.request_inner_size(
                        winit::dpi::LogicalSize::new(new_width, new_height),
                    );
                    log::info!("Output resized to {}x{}", new_width, new_height);
                }
            }
            _ => {}
        }
    }

    fn process_audio_commands(&mut self) {
        let command = std::mem::replace(&mut lock(&self.shared_state).audio_command, AudioCommand::None);

        match command {
            AudioCommand::RefreshDevices => {
                let devices = list_audio_devices();
                log::info!("[Audio] Refreshed devices: {} found", devices.len());
                lock(&self.shared_state).audio.available_devices = devices;
            }
            AudioCommand::SelectDevice(device_name) => {
                log::info!("[Audio] Selecting device: {}", device_name);
                lock(&self.shared_state).audio.selected_device = Some(device_name.clone());
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
                    let device = lock(&self.shared_state).audio.selected_device.clone();
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

    fn process_midi_commands(&mut self) {
        let command = std::mem::replace(&mut lock(&self.shared_state).midi_command, MidiCommand::None);

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

    fn process_osc_commands(&mut self) {
        let command = std::mem::replace(&mut lock(&self.shared_state).osc_command, OscCommand::None);

        match command {
            OscCommand::Start => {
                if let Some(ref mut server) = self.osc_server {
                    if let Err(e) = server.start() {
                        log::error!("Failed to start OSC server: {}", e);
                    } else {
                        lock(&self.shared_state).osc_enabled = true;
                        log::info!("OSC server started");
                    }
                }
            }
            OscCommand::Stop => {
                if let Some(ref mut server) = self.osc_server {
                    server.stop();
                    lock(&self.shared_state).osc_enabled = false;
                    log::info!("OSC server stopped");
                }
            }
            OscCommand::SetPort(port) => {
                if let Some(ref mut server) = self.osc_server {
                    server.stop();
                    let mut new_server = OscServer::new(port, "/rustjay");
                    if let Ok(mut state) = new_server.state().lock() {
                        state.register_default_parameters();
                    }
                    *server = new_server;
                    let mut state = lock(&self.shared_state);
                    state.osc_port = port;
                    state.osc_enabled = false;
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

    fn process_preset_commands(&mut self) {
        let command = std::mem::replace(&mut lock(&self.shared_state).preset_command, PresetCommand::None);

        match command {
            PresetCommand::Save { name } => {
                if let Some(ref mut bank) = self.preset_bank {
                    let preset = {
                        let state = lock(&self.shared_state);
                        crate::presets::Preset::from_state(&name, &state)
                    };
                    match bank.add_preset(preset) {
                        Ok(index) => log::info!("Saved preset '{}' at index {}", name, index),
                        Err(e) => log::error!("Failed to save preset: {}", e),
                    }
                }
            }
            PresetCommand::Load(index) => {
                if let Some(ref mut bank) = self.preset_bank {
                    let mut state = lock(&self.shared_state);
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
                    let mut state = lock(&self.shared_state);
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

    fn process_web_commands(&mut self) {
        let command = std::mem::replace(&mut lock(&self.shared_state).web_command, WebCommand::None);

        match command {
            WebCommand::Start => {
                if let Some(ref mut server) = self.web_server {
                    if let Err(e) = server.start() {
                        log::error!("Failed to start web server: {}", e);
                    } else {
                        lock(&self.shared_state).web_enabled = true;
                        log::info!("Web server started at {}", server.get_url());
                    }
                }
            }
            WebCommand::Stop => {
                if let Some(ref mut server) = self.web_server {
                    server.stop();
                    lock(&self.shared_state).web_enabled = false;
                    log::info!("Web server stopped");
                }
            }
            WebCommand::SetPort(port) => {
                if let Some(ref mut server) = self.web_server {
                    server.stop();
                    let config = WebConfig {
                        port,
                        app_name: "rustjay".to_string(),
                        enabled: false,
                    };
                    let (new_server, cmd_tx) = WebServer::new(config);
                    *server = new_server;
                    self.web_command_tx = Some(cmd_tx);
                    let mut state = lock(&self.shared_state);
                    state.web_port = port;
                    state.web_enabled = false;
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
                                    state.output_fullscreen = value > 0.5;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
}
