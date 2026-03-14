//! # Control GUI
//!
//! Main ImGui interface for controlling the application.

#![allow(deprecated)]

use crate::core::{AudioCommand, GuiTab, HsbParams, InputCommand, OutputCommand, SharedState, MidiCommand, OscCommand, PresetCommand, WebCommand};
use crate::core::lfo::{LfoTarget, Waveform, beat_division_to_hz};
use crate::input::InputManager;
use std::sync::{Arc, Mutex};

/// Main control GUI
pub struct ControlGui {
    shared_state: Arc<Mutex<SharedState>>,

    // Device lists
    webcam_devices: Vec<String>,
    ndi_sources: Vec<String>,
    #[cfg(target_os = "macos")]
    syphon_servers: Vec<crate::input::SyphonServerInfo>,
    audio_devices: Vec<String>,

    // Selection state
    selected_webcam: usize,
    selected_ndi: usize,
    #[cfg(target_os = "macos")]
    selected_syphon: usize,
    selected_audio_device: usize,

    // NDI output name
    ndi_output_name: String,

    // Syphon output name (macOS)
    #[cfg(target_os = "macos")]
    syphon_output_name: String,

    // Preview texture IDs
    pub input_preview_texture_id: Option<imgui::TextureId>,
    pub output_preview_texture_id: Option<imgui::TextureId>,
    
    // Pending resolution changes
    pending_internal_width: u32,
    pending_internal_height: u32,
    pending_output_width: u32,
    pending_output_height: u32,
}

impl ControlGui {
    /// Create a new control GUI
    pub fn new(shared_state: Arc<Mutex<SharedState>>) -> anyhow::Result<Self> {
        let (ndi_name, syphon_name) = {
            let state = shared_state.lock().unwrap();
            #[cfg(target_os = "macos")]
            let syphon = state.syphon_output.server_name.clone();
            #[cfg(not(target_os = "macos"))]
            let syphon = String::new();
            (state.ndi_output.stream_name.clone(), syphon)
        };

        // Initialize pending resolutions from current state
        let (internal_w, internal_h, output_w, output_h) = {
            let state = shared_state.lock().unwrap();
            (
                state.resolution.internal_width,
                state.resolution.internal_height,
                state.output_width,
                state.output_height,
            )
        };
        
        Ok(Self {
            shared_state,
            webcam_devices: Vec::new(),
            ndi_sources: Vec::new(),
            #[cfg(target_os = "macos")]
            syphon_servers: Vec::new(),
            audio_devices: Vec::new(),
            selected_webcam: 0,
            selected_ndi: 0,
            #[cfg(target_os = "macos")]
            selected_syphon: 0,
            selected_audio_device: 0,
            ndi_output_name: ndi_name,
            #[cfg(target_os = "macos")]
            syphon_output_name: syphon_name,
            input_preview_texture_id: None,
            output_preview_texture_id: None,
            pending_internal_width: internal_w,
            pending_internal_height: internal_h,
            pending_output_width: output_w,
            pending_output_height: output_h,
        })
    }

    /// Set input preview texture ID
    pub fn set_input_preview_texture(&mut self, texture_id: imgui::TextureId) {
        self.input_preview_texture_id = Some(texture_id);
    }

    /// Set output preview texture ID
    pub fn set_output_preview_texture(&mut self, texture_id: imgui::TextureId) {
        self.output_preview_texture_id = Some(texture_id);
    }
    
    /// Update FPS counter (deprecated - FPS now tracked in output engine)
    #[allow(dead_code)]
    pub fn update_fps(&mut self) {
        // FPS is now tracked in WgpuEngine and stored in SharedState.performance
    }

    /// Refresh device lists
    pub fn refresh_devices(&mut self, input_manager: &mut InputManager) {
        input_manager.refresh_devices();
        self.webcam_devices = input_manager.webcam_devices().to_vec();
        self.ndi_sources = input_manager.ndi_sources().to_vec();
        #[cfg(target_os = "macos")]
        {
            self.syphon_servers = input_manager.syphon_servers().to_vec();
        }
        
        // Refresh audio devices
        self.audio_devices = crate::audio::list_audio_devices();
        log::info!("[GUI] Found {} audio device(s)", self.audio_devices.len());
        for device in &self.audio_devices {
            log::info!("  - {}", device);
        }
        
        // Update shared state with available devices
        if let Ok(mut state) = self.shared_state.lock() {
            state.audio.available_devices = self.audio_devices.clone();
        }
    }

    /// Build the ImGui UI
    pub fn build_ui(&mut self, ui: &mut imgui::Ui) {
        let window_size = ui.io().display_size;

        // Main control window
        ui.window("RustJay Template - Controls")
            .position([10.0, 10.0], imgui::Condition::FirstUseEver)
            .size([400.0, window_size[1] - 20.0], imgui::Condition::FirstUseEver)
            .movable(false)
            .collapsible(false)
            .resizable(true)
            .build(|| {
                self.build_menu_bar(ui);
                self.build_tabs(ui);
            });

        // Input preview window
        let preview_pos = [420.0, 10.0];
        let preview_size = [
            (window_size[0] - preview_pos[0] - 10.0).max(200.0),
            (window_size[1] / 2.0 - 15.0).max(200.0),
        ];

        ui.window("Input Preview")
            .position(preview_pos, imgui::Condition::FirstUseEver)
            .size(preview_size, imgui::Condition::FirstUseEver)
            .build(|| {
                self.build_input_preview(ui, preview_size);
            });

        // Output preview window
        let output_preview_pos = [420.0, window_size[1] / 2.0 + 5.0];
        let output_preview_size = [
            (window_size[0] - output_preview_pos[0] - 10.0).max(200.0),
            (window_size[1] / 2.0 - 15.0).max(200.0),
        ];

        ui.window("Output Preview")
            .position(output_preview_pos, imgui::Condition::FirstUseEver)
            .size(output_preview_size, imgui::Condition::FirstUseEver)
            .build(|| {
                self.build_output_preview(ui, output_preview_size);
            });

        // LFO Control Window (conditional)
        let show_lfo = {
            let state = self.shared_state.lock().unwrap();
            state.lfo.show_window
        };
        
        if show_lfo {
            self.build_lfo_window(ui);
        }
    }

    /// Build the menu bar
    fn build_menu_bar(&mut self, ui: &imgui::Ui) {
        ui.menu_bar(|| {
            ui.menu("File", || {
                if ui.menu_item("Exit") {
                    // Signal exit - handled by app
                }
            });

            ui.menu("Devices", || {
                if ui.menu_item("Refresh All") {
                    // Signal device refresh
                    let mut state = self.shared_state.lock().unwrap();
                    state.input_command = InputCommand::RefreshDevices;
                }
            });
        });
    }

    /// Build the main tabs
    fn build_tabs(&mut self, ui: &imgui::Ui) {
        let tabs = [
            GuiTab::Input,
            GuiTab::Color,
            GuiTab::Audio,
            GuiTab::Output,
            GuiTab::Presets,
            GuiTab::Midi,
            GuiTab::Osc,
            GuiTab::Web,
            GuiTab::Settings,
        ];

        let current_tab = {
            let state = self.shared_state.lock().unwrap();
            state.current_tab
        };

        if let Some(_tab_bar) = ui.tab_bar("##main_tabs") {
            for tab in &tabs {
                let is_selected = current_tab == *tab;
                if let Some(_tab_item) = ui.tab_item(tab.name()) {
                    if !is_selected {
                        let mut state = self.shared_state.lock().unwrap();
                        state.current_tab = *tab;
                    }
                }
            }
        }

        ui.separator();

        // Build content for current tab
        match current_tab {
            GuiTab::Input => self.build_input_tab(ui),
            GuiTab::Color => self.build_color_tab(ui),
            GuiTab::Audio => self.build_audio_tab(ui),
            GuiTab::Output => self.build_output_tab(ui),
            GuiTab::Presets => self.build_presets_tab(ui),
            GuiTab::Midi => self.build_midi_tab(ui),
            GuiTab::Osc => self.build_osc_tab(ui),
            GuiTab::Web => self.build_web_tab(ui),
            GuiTab::Settings => self.build_settings_tab(ui),
        }
    }

    /// Build the Input tab
    fn build_input_tab(&mut self, ui: &imgui::Ui) {
        let (is_active, source_type, source_name) = {
            let state = self.shared_state.lock().unwrap();
            (
                state.input.is_active,
                state.input.input_type,
                state.input.source_name.clone(),
            )
        };

        ui.text("Video Input Source");
        ui.separator();

        // Status
        if is_active {
            ui.text_colored([0.0, 1.0, 0.0, 1.0], &format!("Active: {}", source_name));
        } else {
            ui.text_colored([0.5, 0.5, 0.5, 1.0], "No input active");
        }

        ui.spacing();

        // Refresh Sources button - prominently at the top
        let _btn_color = ui.push_style_color(imgui::StyleColor::Button, [0.2, 0.6, 0.8, 1.0]);
        let _btn_hover = ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.3, 0.7, 0.9, 1.0]);
        let _btn_active = ui.push_style_color(imgui::StyleColor::ButtonActive, [0.1, 0.5, 0.7, 1.0]);
        if ui.button_with_size("Refresh Sources", [ui.content_region_avail()[0], 30.0]) {
            let mut state = self.shared_state.lock().unwrap();
            state.input_command = InputCommand::RefreshDevices;
        }
        // Tokens are automatically popped when they go out of scope

        ui.spacing();
        ui.separator();
        ui.spacing();

        // Webcam section
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Webcam");
        if !self.webcam_devices.is_empty() {
            let device_names: Vec<&str> = self.webcam_devices.iter().map(|s| s.as_str()).collect();
            ui.combo_simple_string("Select Webcam", &mut self.selected_webcam, &device_names);

            if ui.button("Start Webcam") {
                let mut state = self.shared_state.lock().unwrap();
                state.input_command = InputCommand::StartWebcam {
                    device_index: self.selected_webcam as usize,
                    width: 1920,
                    height: 1080,
                    fps: 30,
                };
            }
        } else {
            ui.text_disabled("No webcams found");
        }

        ui.spacing();
        ui.separator();
        ui.spacing();

        // NDI section
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "NDI");
        if !self.ndi_sources.is_empty() {
            let source_names: Vec<&str> = self.ndi_sources.iter().map(|s| s.as_str()).collect();
            ui.combo_simple_string("Select NDI Source", &mut self.selected_ndi, &source_names);

            if ui.button("Start NDI") {
                let source_name = self.ndi_sources.get(self.selected_ndi as usize)
                    .cloned()
                    .unwrap_or_default();
                let mut state = self.shared_state.lock().unwrap();
                state.input_command = InputCommand::StartNdi { source_name };
            }
        } else {
            ui.text_disabled("No NDI sources found");
        }

        // Syphon section (macOS only)
        #[cfg(target_os = "macos")]
        {
            ui.spacing();
            ui.separator();
            ui.spacing();

            ui.text_colored([0.0, 1.0, 1.0, 1.0], "Syphon (macOS)");
            if !self.syphon_servers.is_empty() {
                let server_names: Vec<String> = self.syphon_servers.iter()
                    .map(|s| format!("{} - {}", s.app_name, s.name))
                    .collect();
                let server_name_refs: Vec<&str> = server_names.iter().map(|s| s.as_str()).collect();
                let selected = self.selected_syphon;
                ui.combo_simple_string("Select Syphon Server", &mut self.selected_syphon, &server_name_refs);

                if ui.button("Start Syphon") {
                    let server_name = self.syphon_servers.get(selected)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    let mut state = self.shared_state.lock().unwrap();
                    state.input_command = InputCommand::StartSyphon { server_name };
                }
            } else {
                ui.text_disabled("No Syphon servers found");
            }
        }

        ui.spacing();
        ui.separator();
        ui.spacing();

        // Stop button
        if is_active && ui.button("Stop Input") {
            let mut state = self.shared_state.lock().unwrap();
            state.input_command = InputCommand::StopInput;
        }
    }

    /// Build the Color tab
    fn build_color_tab(&mut self, ui: &imgui::Ui) {
        // Read base values from audio_routing (not modulated hsb_params)
        let (mut enabled, mut hue, mut sat, mut bright) = {
            let state = self.shared_state.lock().unwrap();
            (
                state.color_enabled,
                state.audio_routing.base_hue,
                state.audio_routing.base_saturation,
                state.audio_routing.base_brightness,
            )
        };
        // Create HsbParams for convenience
        let mut hsb = HsbParams {
            hue_shift: hue,
            saturation: sat,
            brightness: bright,
        };

        ui.text("HSB Color Adjustment");
        ui.separator();

        // Enable/disable
        if ui.checkbox("Enable Color Adjustment", &mut enabled) {
            let mut state = self.shared_state.lock().unwrap();
            state.color_enabled = enabled;
        }

        ui.spacing();

        if enabled {
            // Hue shift
            ui.text("Hue Shift");
            if ui.slider_config("Hue", -180.0, 180.0)
                .display_format("%.0f°")
                .build(&mut hsb.hue_shift)
            {
                let mut state = self.shared_state.lock().unwrap();
                state.hsb_params.hue_shift = hsb.hue_shift;
                // Update audio routing base values
                state.audio_routing.update_base_values(hsb.hue_shift, hsb.saturation, hsb.brightness);
            }

            // Saturation
            ui.text("Saturation");
            if ui.slider_config("Saturation", 0.0, 2.0)
                .display_format("%.2fx")
                .build(&mut hsb.saturation)
            {
                let mut state = self.shared_state.lock().unwrap();
                state.hsb_params.saturation = hsb.saturation;
                // Update audio routing base values
                state.audio_routing.update_base_values(hsb.hue_shift, hsb.saturation, hsb.brightness);
            }

            // Brightness
            ui.text("Brightness");
            if ui.slider_config("Brightness", 0.0, 2.0)
                .display_format("%.2fx")
                .build(&mut hsb.brightness)
            {
                let mut state = self.shared_state.lock().unwrap();
                state.hsb_params.brightness = hsb.brightness;
                // Update audio routing base values
                state.audio_routing.update_base_values(hsb.hue_shift, hsb.saturation, hsb.brightness);
            }

            ui.spacing();
            ui.separator();
            ui.spacing();

            // Reset button
            if ui.button("Reset to Default") {
                hsb.reset();
                let mut state = self.shared_state.lock().unwrap();
                state.hsb_params = hsb;
                // Update audio routing base values
                state.audio_routing.update_base_values(hsb.hue_shift, hsb.saturation, hsb.brightness);
            }
            
            ui.spacing();
            ui.separator();
            ui.spacing();
            
            // LFO Controls
            ui.text("LFO Modulation");
            
            if ui.button("Open LFO Window") {
                let mut state = self.shared_state.lock().unwrap();
                state.lfo.show_window = true;
            }
            
            // Display active LFO count
            let active_lfos = {
                let state = self.shared_state.lock().unwrap();
                state.lfo.bank.lfos.iter().filter(|b| b.enabled).count()
            };
            
            if active_lfos > 0 {
                ui.same_line();
                ui.text_colored(
                    [0.2, 0.8, 0.2, 1.0],
                    &format!("({} active)", active_lfos)
                );
            }
        } else {
            ui.text_disabled("Color adjustment is disabled");
        }
    }

    /// Build the Audio tab
    fn build_audio_tab(&mut self, ui: &imgui::Ui) {
        let (mut enabled, mut amplitude, mut smoothing, fft, volume, selected_device) = {
            let state = self.shared_state.lock().unwrap();
            (
                state.audio.enabled,
                state.audio.amplitude,
                state.audio.smoothing,
                state.audio.fft,
                state.audio.volume,
                state.audio.selected_device.clone(),
            )
        };

        ui.text("Audio Analysis");
        ui.separator();

        // Audio Device Selection
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Input Device");
        
        // Refresh button
        if ui.button("Refresh Audio Devices") {
            let mut state = self.shared_state.lock().unwrap();
            state.audio_command = AudioCommand::RefreshDevices;
        }
        
        ui.spacing();
        
        // Device dropdown
        if !self.audio_devices.is_empty() {
            let device_names: Vec<&str> = self.audio_devices.iter().map(|s| s.as_str()).collect();
            
            // Find current selection index
            if let Some(ref current) = selected_device {
                if let Some(idx) = self.audio_devices.iter().position(|d| d == current) {
                    self.selected_audio_device = idx;
                }
            }
            
            if ui.combo_simple_string("Select Audio Device", &mut self.selected_audio_device, &device_names) {
                let device_name = self.audio_devices.get(self.selected_audio_device).cloned();
                let mut state = self.shared_state.lock().unwrap();
                state.audio_command = AudioCommand::SelectDevice(device_name.unwrap_or_default());
            }
            
            // Show currently selected
            if let Some(ref device) = selected_device {
                ui.text(format!("Active: {}", device));
            }
        } else {
            ui.text_disabled("No audio devices found. Click Refresh.");
        }

        ui.spacing();
        ui.separator();
        ui.spacing();

        // Enable/disable
        if ui.checkbox("Enable Audio Analysis", &mut enabled) {
            let mut state = self.shared_state.lock().unwrap();
            state.audio.enabled = enabled;
            if enabled {
                state.audio_command = AudioCommand::Start;
            } else {
                state.audio_command = AudioCommand::Stop;
            }
        }

        ui.spacing();

        if enabled {
            // Get additional audio settings
            let (mut normalize, mut pink_noise) = {
                let state = self.shared_state.lock().unwrap();
                (state.audio.normalize, state.audio.pink_noise_shaping)
            };

            // Amplitude
            ui.text("Input Amplitude");
            if ui.slider("Amplitude", 0.1, 5.0, &mut amplitude) {
                let mut state = self.shared_state.lock().unwrap();
                state.audio.amplitude = amplitude;
            }

            // Smoothing
            ui.text("Smoothing (0 = instant, 0.99 = very slow)");
            if ui.slider("Smoothing", 0.0, 0.95, &mut smoothing) {
                let mut state = self.shared_state.lock().unwrap();
                state.audio.smoothing = smoothing.clamp(0.0, 0.99);
            }

            ui.spacing();
            ui.separator();
            ui.spacing();

            // Processing options
            ui.text("Processing Options");
            
            if ui.checkbox("Normalize Bands", &mut normalize) {
                let mut state = self.shared_state.lock().unwrap();
                state.audio.normalize = normalize;
            }
            ui.same_line();
            ui.text_disabled("(Scales all bands to max)");

            if ui.checkbox("+3dB/Octave Shaping", &mut pink_noise) {
                let mut state = self.shared_state.lock().unwrap();
                state.audio.pink_noise_shaping = pink_noise;
            }
            ui.same_line();
            ui.text_disabled("(Compensates for pink noise spectrum)");

            ui.spacing();
            ui.separator();
            ui.spacing();

            // Tap Tempo section
            ui.text_colored([0.0, 1.0, 1.0, 1.0], "Tempo");
            
            let (bpm, tap_info) = {
                let state = self.shared_state.lock().unwrap();
                (state.audio.bpm, state.audio.tap_tempo_info.clone())
            };
            
            ui.text(format!("BPM: {:.1}", bpm));
            
            // Tap tempo button
            let _btn_color = ui.push_style_color(imgui::StyleColor::Button, [0.8, 0.3, 0.3, 1.0]);
            let _btn_hover = ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.9, 0.4, 0.4, 1.0]);
            let _btn_active = ui.push_style_color(imgui::StyleColor::ButtonActive, [1.0, 0.5, 0.5, 1.0]);
            
            if ui.button_with_size("TAP", [60.0, 30.0]) {
                self.handle_tap_tempo();
            }
            
            ui.same_line();
            ui.text_disabled(&tap_info);
            
            ui.spacing();
            ui.separator();
            ui.spacing();

            // FFT visualization
            ui.text("Frequency Bands");
            let band_names = ["Sub", "Bass", "Low", "Mid", "High", "Presence", "Brilliance", "Air"];
            for (i, (&value, name)) in fft.iter().zip(band_names.iter()).enumerate() {
                let width = 200.0 * value;
                ui.text(format!("{}", name));
                ui.same_line();
                ui.text(format!(": {:.2}", value));
                // Draw a simple bar
                let draw_list = ui.get_window_draw_list();
                let pos = ui.cursor_screen_pos();
                draw_list.add_rect(
                    pos,
                    [pos[0] + width, pos[1] + 10.0],
                    [0.0, 1.0, 0.0, 1.0],
                ).filled(true).build();
                ui.new_line();
            }

            ui.spacing();
            ui.text(format!("Volume: {:.2}", volume));
            
            ui.spacing();
            ui.separator();
            ui.spacing();
            
            // Audio Routing section
            self.build_audio_routing_section(ui);
            
        } else {
            ui.text_disabled("Audio analysis is disabled");
        }
    }
    
    /// Build the audio routing section in the Audio tab
    fn build_audio_routing_section(&mut self, ui: &imgui::Ui) {
        use crate::audio::routing::{FftBand, ModulationTarget};
        
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Audio Reactivity Routing");
        
        let (routing_enabled, show_window, can_add_route) = {
            let state = self.shared_state.lock().unwrap();
            let routing = &state.audio_routing;
            (routing.enabled, routing.show_window, routing.matrix.can_add_route())
        };
        
        // Enable/disable toggle
        let mut enabled = routing_enabled;
        if ui.checkbox("Enable Audio Routing", &mut enabled) {
            let mut state = self.shared_state.lock().unwrap();
            state.audio_routing.enabled = enabled;
        }
        
        if !enabled {
            ui.text_disabled("Audio routing is disabled");
            return;
        }
        
        ui.same_line();
        
        // Open routing window button
        let mut show = show_window;
        if ui.button("Open Routing Matrix") {
            show = !show;
            let mut state = self.shared_state.lock().unwrap();
            state.audio_routing.show_window = show;
        }
        
        // Show current routes summary
        let route_count = {
            let state = self.shared_state.lock().unwrap();
            state.audio_routing.matrix.len()
        };
        
        if route_count > 0 {
            ui.text(format!("Active routes: {}", route_count));
            
            // Show a mini list of routes
            let state = self.shared_state.lock().unwrap();
            for (i, route) in state.audio_routing.matrix.routes().iter().enumerate() {
                if !route.enabled { continue; }
                ui.text(format!("  {} → {} ({:.0}%)", 
                    route.band.short_name(),
                    route.target.name(),
                    route.amount * 100.0
                ));
                if i >= 3 { // Limit to 3 lines
                    let remaining = route_count - 4;
                    if remaining > 0 {
                        ui.text_disabled(format!("  ... and {} more", remaining));
                    }
                    break;
                }
            }
        } else {
            ui.text_disabled("No active routes. Click 'Open Routing Matrix' to add.");
        }
        
        // Build routing window if shown (lock is already dropped at this point)
        if show {
            self.build_routing_window(ui);
        }
    }
    
    /// Build the audio routing matrix window
    fn build_routing_window(&mut self, ui: &imgui::Ui) {
        use crate::audio::routing::{FftBand, ModulationTarget};
        
        let mut is_open = true;
        
        ui.window("Audio Routing Matrix")
            .position([500.0, 100.0], imgui::Condition::FirstUseEver)
            .size([450.0, 550.0], imgui::Condition::FirstUseEver)
            .opened(&mut is_open)
            .build(|| {
                // Get current state
                let (can_add, route_count, max_routes) = {
                    let state = self.shared_state.lock().unwrap();
                    let routing = &state.audio_routing;
                    (routing.matrix.can_add_route(), routing.matrix.len(), routing.matrix.max_routes())
                };
                
                ui.text(format!("Routes: {}/{}", route_count, max_routes));
                ui.same_line();
                
                // Clear all button
                if ui.button("Clear All") {
                    let mut state = self.shared_state.lock().unwrap();
                    state.audio_routing.matrix.clear();
                }
                
                ui.separator();
                
                // Add new route section
                ui.text_colored([0.0, 1.0, 1.0, 1.0], "Add New Route");
                
                // Get selections
                let (mut band_idx, mut target_idx) = {
                    let state = self.shared_state.lock().unwrap();
                    (state.audio_routing.selected_band, state.audio_routing.selected_target)
                };
                
                // Band selection
                let bands: Vec<&str> = FftBand::all().iter().map(|b| b.name()).collect();
                ui.combo_simple_string("Band##new", &mut band_idx, &bands);
                
                // Target selection
                let targets: Vec<&str> = ModulationTarget::all().iter().map(|t| t.name()).collect();
                ui.combo_simple_string("Target##new", &mut target_idx, &targets);
                
                // Update selections
                {
                    let mut state = self.shared_state.lock().unwrap();
                    state.audio_routing.selected_band = band_idx;
                    state.audio_routing.selected_target = target_idx;
                }
                
                ui.same_line();
                
                // Add button
                let can_add = can_add && band_idx < FftBand::all().len() && target_idx < ModulationTarget::all().len();
                if can_add {
                    if ui.button("Add Route") {
                        if let Some(band) = FftBand::from_index(band_idx) {
                            if let Some(target) = ModulationTarget::all().get(target_idx) {
                                let mut state = self.shared_state.lock().unwrap();
                                state.audio_routing.matrix.add_route(band, *target);
                            }
                        }
                    }
                } else {
                    ui.text_disabled("Max routes reached");
                }
                
                ui.separator();
                
                // Existing routes list
                ui.text_colored([0.0, 1.0, 1.0, 1.0], "Active Routes");
                
                // We need to collect route data first to avoid borrow issues
                let routes_data: Vec<_> = {
                    let state = self.shared_state.lock().unwrap();
                    state.audio_routing.matrix.routes().iter().map(|r| {
                        (r.id, r.band, r.target, r.amount, r.attack, r.release, r.enabled, r.current_value)
                    }).collect()
                };
                
                for (id, band, target, amount, attack, release, enabled, current) in &routes_data {
                    let _id_token = ui.push_id(format!("route_{}", *id));
                    
                    // Enable/disable checkbox
                    let mut is_enabled = *enabled;
                    if ui.checkbox("##enabled", &mut is_enabled) {
                        let mut state = self.shared_state.lock().unwrap();
                        if let Some(route) = state.audio_routing.matrix.get_route_mut(*id) {
                            route.enabled = is_enabled;
                        }
                    }
                    ui.same_line();
                    
                    // Route info
                    ui.text(format!("{} → {}", band.short_name(), target.name()));
                    
                    // Current value indicator
                    ui.same_line();
                    ui.text_colored([0.0, 1.0, 0.0, 1.0], format!("{:.2}", current));
                    
                    // Delete button
                    ui.same_line();
                    if ui.button("X") {
                        let mut state = self.shared_state.lock().unwrap();
                        state.audio_routing.matrix.remove_route(*id);
                    }
                    
                    // Amount slider
                    let mut amt = *amount;
                    if ui.slider("Amount", -1.0, 1.0, &mut amt) {
                        let mut state = self.shared_state.lock().unwrap();
                        if let Some(route) = state.audio_routing.matrix.get_route_mut(*id) {
                            route.amount = amt;
                        }
                    }
                    
                    // Attack/Release sliders
                    ui.columns(2, "attack_release", false);
                    let mut atk = *attack;
                    if ui.slider("Attack", 0.001, 1.0, &mut atk) {
                        let mut state = self.shared_state.lock().unwrap();
                        if let Some(route) = state.audio_routing.matrix.get_route_mut(*id) {
                            route.attack = atk;
                        }
                    }
                    ui.next_column();
                    let mut rel = *release;
                    if ui.slider("Release", 0.001, 1.0, &mut rel) {
                        let mut state = self.shared_state.lock().unwrap();
                        if let Some(route) = state.audio_routing.matrix.get_route_mut(*id) {
                            route.release = rel;
                        }
                    }
                    ui.columns(1, "", false);
                    
                    ui.separator();
                    // _id_token auto-pops when dropped
                }
                
                if routes_data.is_empty() {
                    ui.text_disabled("No routes configured. Add one above.");
                }
            });
        
        // Update window visibility
        if !is_open {
            let mut state = self.shared_state.lock().unwrap();
            state.audio_routing.show_window = false;
        }
    }

    /// Handle tap tempo button press
    pub fn handle_tap_tempo(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        
        let mut state = self.shared_state.lock().unwrap();
        
        // Clear taps if it's been too long since last tap (2 seconds)
        if now - state.audio.last_tap_time > 2.0 {
            state.audio.tap_times.clear();
            state.audio.tap_tempo_info = "Reset: new tempo sequence".to_string();
        } else {
            state.audio.tap_tempo_info = format!("{} taps recorded", state.audio.tap_times.len() + 1);
        }
        
        // Add tap time
        state.audio.tap_times.push(now);
        state.audio.last_tap_time = now;
        
        // Keep only last 8 taps for average
        if state.audio.tap_times.len() > 8 {
            state.audio.tap_times.remove(0);
        }
        
        // Reset beat phase on every tap (global sync)
        state.audio.beat_phase = 0.0;
        
        // Calculate BPM from tap intervals (need at least 4 taps for accuracy)
        if state.audio.tap_times.len() >= 4 {
            let mut intervals = Vec::new();
            for i in 1..state.audio.tap_times.len() {
                intervals.push(state.audio.tap_times[i] - state.audio.tap_times[i-1]);
            }
            
            // Average interval
            let avg_interval: f64 = intervals.iter().sum::<f64>() / intervals.len() as f64;
            
            if avg_interval > 0.1 && avg_interval < 3.0 { // Reasonable range (20-600 BPM)
                let new_bpm = (60.0 / avg_interval) as f32;
                state.audio.bpm = new_bpm.clamp(40.0, 200.0);
                state.audio.tap_tempo_info = format!("BPM: {:.1}", state.audio.bpm);
            }
        }
    }

    /// Build the Output tab
    fn build_output_tab(&mut self, ui: &imgui::Ui) {
        let (ndi_active, fullscreen) = {
            let state = self.shared_state.lock().unwrap();
            (state.ndi_output.is_active, state.output_fullscreen)
        };

        ui.text("Output Settings");
        ui.separator();

        // Fullscreen toggle
        let mut fs = fullscreen;
        if ui.checkbox("Fullscreen Output", &mut fs) {
            let mut state = self.shared_state.lock().unwrap();
            state.output_fullscreen = fs;
        }

        ui.text_disabled("Press Shift+F to toggle fullscreen");

        ui.spacing();
        ui.separator();
        ui.spacing();

        // NDI Output
        ui.text_colored([0.0, 1.0, 0.5, 1.0], "NDI Output");
        ui.input_text("Stream Name", &mut self.ndi_output_name).build();

        if !ndi_active {
            if ui.button("Start NDI Output") {
                let mut state = self.shared_state.lock().unwrap();
                state.ndi_output.stream_name = self.ndi_output_name.clone();
                state.output_command = OutputCommand::StartNdi;
            }
        } else {
            if ui.button("Stop NDI Output") {
                let mut state = self.shared_state.lock().unwrap();
                state.output_command = OutputCommand::StopNdi;
            }
            ui.text_colored([0.0, 1.0, 0.0, 1.0], "NDI Active");
        }

        // Syphon Output (macOS)
        #[cfg(target_os = "macos")]
        {
            ui.spacing();
            ui.separator();
            ui.spacing();

            let syphon_enabled = {
                let state = self.shared_state.lock().unwrap();
                state.syphon_output.enabled
            };

            ui.text_colored([1.0, 0.5, 0.0, 1.0], "Syphon Output (macOS)");
            ui.input_text("Server Name", &mut self.syphon_output_name).build();

            if !syphon_enabled {
                if ui.button("Start Syphon Output") {
                    let mut state = self.shared_state.lock().unwrap();
                    state.syphon_output.server_name = self.syphon_output_name.clone();
                    state.output_command = OutputCommand::StartSyphon;
                }
            } else {
                if ui.button("Stop Syphon Output") {
                    let mut state = self.shared_state.lock().unwrap();
                    state.output_command = OutputCommand::StopSyphon;
                }
                ui.text_colored([0.0, 1.0, 0.0, 1.0], "Syphon Active");
            }
        }
    }

    /// Build the Settings tab
    fn build_settings_tab(&mut self, ui: &imgui::Ui) {
        ui.text("Application Settings");
        ui.separator();

        let mut ui_scale = {
            let state = self.shared_state.lock().unwrap();
            state.ui_scale
        };

        ui.text("UI Scale:");
        if ui.slider("Scale", 0.5, 2.0, &mut ui_scale) {
            let mut state = self.shared_state.lock().unwrap();
            state.ui_scale = ui_scale;
        }

        ui.separator();
        ui.spacing();

        // Resolution settings with dropdown presets
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Resolution Settings");
        
        // Resolution preset dropdown
        let presets = [
            ("Custom", 0, 0),
            ("480p (640x480)", 640, 480),
            ("720p (1280x720)", 1280, 720),
            ("1080p (1920x1080)", 1920, 1080),
            ("1440p (2560x1440)", 2560, 1440),
            ("4K UHD (3840x2160)", 3840, 2160),
            ("Square 1:1 (1080x1080)", 1080, 1080),
            ("Vertical 9:16 (1080x1920)", 1080, 1920),
        ];
        
        let preset_names: Vec<&str> = presets.iter().map(|(name, _, _)| *name).collect();
        
        // Internal Resolution Section
        ui.text("Internal Resolution (Processing):");
        
        let (current_internal_w, current_internal_h) = {
            let state = self.shared_state.lock().unwrap();
            (state.resolution.internal_width, state.resolution.internal_height)
        };
        
        // Find current preset index
        let mut internal_preset_idx = 0;
        for (i, (_, w, h)) in presets.iter().enumerate().skip(1) {
            if *w == current_internal_w && *h == current_internal_h {
                internal_preset_idx = i;
                break;
            }
        }
        
        let old_internal_preset = internal_preset_idx;
        if ui.combo_simple_string("Preset##internal", &mut internal_preset_idx, &preset_names) {
            if internal_preset_idx != old_internal_preset && internal_preset_idx > 0 {
                let (_, w, h) = presets[internal_preset_idx];
                self.pending_internal_width = w;
                self.pending_internal_height = h;
            }
        }
        
        // Manual input
        let mut w = self.pending_internal_width as i32;
        let mut h = self.pending_internal_height as i32;
        ui.text("Custom:");
        ui.input_int("Width##internal", &mut w).step(1).build();
        ui.input_int("Height##internal", &mut h).step(1).build();
        self.pending_internal_width = w.max(320) as u32;
        self.pending_internal_height = h.max(240) as u32;
        
        // Output Resolution Section
        ui.spacing();
        ui.text("Output Resolution (Display/NDI):");
        
        let (current_output_w, current_output_h) = {
            let state = self.shared_state.lock().unwrap();
            (state.output_width, state.output_height)
        };
        
        // Find current preset index
        let mut output_preset_idx = 0;
        for (i, (_, w, h)) in presets.iter().enumerate().skip(1) {
            if *w == current_output_w && *h == current_output_h {
                output_preset_idx = i;
                break;
            }
        }
        
        let old_output_preset = output_preset_idx;
        if ui.combo_simple_string("Preset##output", &mut output_preset_idx, &preset_names) {
            if output_preset_idx != old_output_preset && output_preset_idx > 0 {
                let (_, w, h) = presets[output_preset_idx];
                self.pending_output_width = w;
                self.pending_output_height = h;
            }
        }
        
        // Manual input
        let mut ow = self.pending_output_width as i32;
        let mut oh = self.pending_output_height as i32;
        ui.text("Custom:");
        ui.input_int("Width##output", &mut ow).step(1).build();
        ui.input_int("Height##output", &mut oh).step(1).build();
        self.pending_output_width = ow.max(320) as u32;
        self.pending_output_height = oh.max(240) as u32;
        
        // Apply button
        ui.spacing();
        let _btn_color = ui.push_style_color(imgui::StyleColor::Button, [0.2, 0.7, 0.3, 1.0]);
        let _btn_hover = ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.3, 0.8, 0.4, 1.0]);
        let _btn_active = ui.push_style_color(imgui::StyleColor::ButtonActive, [0.1, 0.6, 0.2, 1.0]);
        
        if ui.button_with_size("Apply Resolution Changes", [ui.content_region_avail()[0], 30.0]) {
            // Apply internal resolution
            {
                let mut state = self.shared_state.lock().unwrap();
                state.resolution.internal_width = self.pending_internal_width;
                state.resolution.internal_height = self.pending_internal_height;
                state.output_width = self.pending_output_width;
                state.output_height = self.pending_output_height;
            }
            // Signal resolution change command
            let mut state = self.shared_state.lock().unwrap();
            state.output_command = OutputCommand::ResizeOutput;
            // Also signal to save settings
            state.save_settings_requested = true;
            log::info!("Resolution changed - Internal: {}x{}, Output: {}x{}", 
                self.pending_internal_width, self.pending_internal_height,
                self.pending_output_width, self.pending_output_height);
        }
        
        ui.spacing();
        ui.separator();
        ui.spacing();

        ui.text("Keyboard Shortcuts:");
        ui.bullet_text("Shift+F - Toggle Fullscreen");
        ui.bullet_text("Shift+T - Tap Tempo");
        ui.bullet_text("Escape - Exit Application");

        ui.separator();

        // Performance section with FPS counter (from output window)
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Performance (Output Window)");
        
        // Get FPS from shared state (updated by WgpuEngine)
        let (fps, frame_time_ms) = {
            let state = self.shared_state.lock().unwrap();
            (state.performance.fps, state.performance.frame_time_ms)
        };
        
        ui.text(format!("Output FPS: {:.1}", fps));
        ui.text(format!("Frame Time: {:.2} ms", frame_time_ms));
        
        ui.text_disabled("All textures use native BGRA format for optimal macOS performance.");
        
        ui.separator();
        
        // Save settings button
        ui.spacing();
        let _btn_color = ui.push_style_color(imgui::StyleColor::Button, [0.2, 0.5, 0.8, 1.0]);
        let _btn_hover = ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.3, 0.6, 0.9, 1.0]);
        let _btn_active = ui.push_style_color(imgui::StyleColor::ButtonActive, [0.1, 0.4, 0.7, 1.0]);
        
        if ui.button_with_size("Save All Settings", [ui.content_region_avail()[0], 30.0]) {
            let mut state = self.shared_state.lock().unwrap();
            state.save_settings_requested = true;
            log::info!("Save settings requested from GUI");
        }
        
        ui.text_disabled("Settings are auto-saved on exit, or manually with this button.");
    }

    /// Build the input preview
    fn build_input_preview(&mut self, ui: &imgui::Ui, available_size: [f32; 2]) {
        if let Some(texture_id) = self.input_preview_texture_id {
            let (input_width, input_height) = {
                let state = self.shared_state.lock().unwrap();
                (state.input.width, state.input.height)
            };

            let aspect = if input_width > 0 && input_height > 0 {
                input_width as f32 / input_height as f32
            } else {
                16.0 / 9.0
            };

            let max_width = available_size[0] - 16.0;
            let max_height = available_size[1] - 40.0;

            let mut tex_width = max_width;
            let mut tex_height = tex_width / aspect;

            if tex_height > max_height {
                tex_height = max_height;
                tex_width = tex_height * aspect;
            }

            let x_offset = (available_size[0] - tex_width) / 2.0;
            ui.set_cursor_pos([x_offset, 30.0]);

            imgui::Image::new(texture_id, [tex_width, tex_height])
                .uv0([0.0, 0.0])
                .uv1([1.0, 1.0])
                .build(ui);
        } else {
            ui.text_disabled("No input preview available");
        }
    }

    /// Build the output preview
    fn build_output_preview(&mut self, ui: &imgui::Ui, available_size: [f32; 2]) {
        if let Some(texture_id) = self.output_preview_texture_id {
            let aspect = 16.0 / 9.0;
            let max_width = available_size[0] - 16.0;
            let max_height = available_size[1] - 40.0;

            let mut tex_width = max_width;
            let mut tex_height = tex_width / aspect;

            if tex_height > max_height {
                tex_height = max_height;
                tex_width = tex_height * aspect;
            }

            let x_offset = (available_size[0] - tex_width) / 2.0;
            ui.set_cursor_pos([x_offset, 30.0]);

            imgui::Image::new(texture_id, [tex_width, tex_height])
                .uv0([0.0, 0.0])
                .uv1([1.0, 1.0])
                .build(ui);
        } else {
            ui.text_disabled("No output preview available");
        }
    }

    /// Build the Presets tab
    fn build_presets_tab(&mut self, ui: &imgui::Ui) {
        ui.text("Quick Presets");
        ui.separator();
        
        // Quick preset slots (1-8)
        let button_size = [80.0, 60.0];
        let spacing = 8.0;
        let total_width = 4.0 * button_size[0] + 3.0 * spacing;
        let start_x = (ui.window_content_region_max()[0] - ui.window_content_region_min()[0] - total_width) / 2.0;
        
        ui.new_line();
        let draw_list = ui.get_window_draw_list();
        
        for row in 0..2 {
            let y_pos = ui.cursor_screen_pos()[1];
            
            for col in 0..4 {
                let slot = row * 4 + col + 1;
                let x_pos = start_x + col as f32 * (button_size[0] + spacing);
                
                ui.set_cursor_screen_pos([x_pos, y_pos]);
                
                let label = format!("{}", slot);
                let is_active = false; // TODO: Check if slot has preset
                
                let button_color = if is_active {
                    [0.2, 0.6, 1.0, 1.0]
                } else {
                    [0.3, 0.3, 0.3, 1.0]
                };
                
                let _style = ui.push_style_color(imgui::StyleColor::Button, button_color);
                if ui.button_with_size(&label, button_size) {
                    let mut state = self.shared_state.lock().unwrap();
                    state.preset_command = PresetCommand::ApplySlot(slot);
                }
                
                if ui.is_item_hovered() {
                    ui.tooltip_text(format!("Quick slot {} (Shift+F{})", slot, slot));
                }
                
                ui.same_line_with_spacing(0.0, spacing);
            }
            ui.new_line();
        }
        
        ui.separator();
        
        // Preset management buttons
        ui.text("Preset Management");
        
        if ui.button("Save New Preset") {
            // TODO: Open save dialog
            ui.open_popup("save_preset_popup");
        }
        ui.same_line();
        
        if ui.button("Refresh List") {
            let mut state = self.shared_state.lock().unwrap();
            state.preset_command = PresetCommand::Refresh;
        }
        
        ui.same_line();
        
        if ui.button("Import") {
            // TODO: Import preset
        }
        
        ui.same_line();
        
        if ui.button("Export") {
            // TODO: Export preset
        }
        
        // Save preset popup (using modal_popup)
        // Save preset popup
        let mut preset_name_buffer = String::with_capacity(256);
        if ui.modal_popup_config("save_preset_popup")
            .resizable(false)
            .always_auto_resize(true)
            .begin_popup()
            .is_some() 
        {
            ui.text("Enter preset name:");
            
            ui.input_text("##preset_name", &mut preset_name_buffer)
                .build();
            
            if ui.button("Save") && !preset_name_buffer.is_empty() {
                let name = preset_name_buffer.clone();
                let mut state = self.shared_state.lock().unwrap();
                state.preset_command = PresetCommand::Save { name };
                ui.close_current_popup();
            }
            
            ui.same_line();
            if ui.button("Cancel") {
                ui.close_current_popup();
            }
        }
        
        ui.separator();
        
        // Preset list
        ui.text("Available Presets");
        ui.text_disabled("(Click to load, Right-click for options)");
        
        // Placeholder list - actual list will be populated from PresetBank
        let _avail_region = ui.content_region_avail();
        
        ui.child_window("presets_list")
            .size([0.0, 200.0])
            .build(|| {
                // TODO: List actual presets from PresetBank
                ui.text_disabled("No presets loaded (PresetBank integration needed)");
            });
    }

    /// Build the MIDI tab
    fn build_midi_tab(&mut self, ui: &imgui::Ui) {
        ui.text("MIDI Control");
        ui.separator();
        
        // Device selection - TODO: Get from MidiState
        // For now, these are placeholders until MidiState is integrated with GUI
        let _enabled = false;
        let _device: Option<String> = None;
        
        ui.text("Device:");
        ui.same_line();
        
        // TODO: Device dropdown from MidiState
        if ui.button("Refresh Devices") {
            let mut state = self.shared_state.lock().unwrap();
            state.midi_command = MidiCommand::RefreshDevices;
        }
        
        ui.separator();
        
        // Learn mode section
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "MIDI Learn");
        ui.text("Click a parameter below, then move a MIDI controller to map it.");
        
        if ui.button("Clear All Mappings") {
            let mut state = self.shared_state.lock().unwrap();
            state.midi_command = MidiCommand::ClearMappings;
        }
        
        ui.separator();
        
        // Mappable parameters
        ui.text("Parameters");
        
        // Color parameters
        if ui.collapsing_header("Color", imgui::TreeNodeFlags::DEFAULT_OPEN) {
            ui.indent();
            
            if ui.button("Learn: Hue Shift") {
                let mut state = self.shared_state.lock().unwrap();
                state.midi_command = MidiCommand::StartLearn { 
                    param_path: "color/hue_shift".to_string(),
                    param_name: "Hue Shift".to_string(),
                };
            }
            ui.same_line();
            ui.text_disabled("(CC: --)");
            
            if ui.button("Learn: Saturation") {
                let mut state = self.shared_state.lock().unwrap();
                state.midi_command = MidiCommand::StartLearn { 
                    param_path: "color/saturation".to_string(),
                    param_name: "Saturation".to_string(),
                };
            }
            ui.same_line();
            ui.text_disabled("(CC: --)");
            
            if ui.button("Learn: Brightness") {
                let mut state = self.shared_state.lock().unwrap();
                state.midi_command = MidiCommand::StartLearn { 
                    param_path: "color/brightness".to_string(),
                    param_name: "Brightness".to_string(),
                };
            }
            ui.same_line();
            ui.text_disabled("(CC: --)");
            
            ui.unindent();
        }
        
        // Audio parameters
        if ui.collapsing_header("Audio", imgui::TreeNodeFlags::empty()) {
            ui.indent();
            
            if ui.button("Learn: Amplitude") {
                let mut state = self.shared_state.lock().unwrap();
                state.midi_command = MidiCommand::StartLearn { 
                    param_path: "audio/amplitude".to_string(),
                    param_name: "Audio Amplitude".to_string(),
                };
            }
            
            if ui.button("Learn: Smoothing") {
                let mut state = self.shared_state.lock().unwrap();
                state.midi_command = MidiCommand::StartLearn { 
                    param_path: "audio/smoothing".to_string(),
                    param_name: "Audio Smoothing".to_string(),
                };
            }
            
            ui.unindent();
        }
        
        ui.separator();
        
        // Active mappings
        ui.text("Active Mappings");
        ui.text_disabled("MIDI mappings will appear here");
    }

    /// Build the OSC tab
    fn build_osc_tab(&mut self, ui: &imgui::Ui) {
        ui.text("OSC Control");
        ui.separator();
        
        // Server settings - read from shared state
        let (running, port) = {
            let state = self.shared_state.lock().unwrap();
            // For now, we track running status in the state
            // In a full implementation, this would come from OscState
            (state.osc_enabled, state.osc_port)
        };
        
        // Status indicator
        let status_color = if running { [0.0, 1.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0, 1.0] };
        let status_text = if running { "Running" } else { "Stopped" };
        
        ui.text("Server Status: ");
        ui.same_line();
        ui.text_colored(status_color, status_text);
        
        ui.separator();
        
        // Port configuration
        ui.text("Port:");
        ui.same_line();
        
        let mut port_i32 = port as i32;
        ui.set_next_item_width(100.0);
        if ui.input_int("##osc_port", &mut port_i32).build() {
            let new_port = port_i32.clamp(1024, 65535) as u16;
            if new_port != port {
                let mut state = self.shared_state.lock().unwrap();
                state.osc_command = OscCommand::SetPort(new_port);
            }
        }
        
        ui.same_line();
        
        // Start/Stop button
        if running {
            if ui.button("Stop Server") {
                let mut state = self.shared_state.lock().unwrap();
                state.osc_command = OscCommand::Stop;
            }
        } else {
            if ui.button("Start Server") {
                let mut state = self.shared_state.lock().unwrap();
                state.osc_command = OscCommand::Start;
            }
        }
        
        ui.separator();
        
        // Address information
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "OSC Addresses");
        ui.text("Send OSC messages to control parameters:");
        
        if ui.collapsing_header("Color", imgui::TreeNodeFlags::DEFAULT_OPEN) {
            ui.indent();
            ui.text("/rustjay/color/hue_shift");
            ui.text_disabled("  Range: 0.0 - 1.0 (maps to -180 to 180)");
            
            ui.text("/rustjay/color/saturation");
            ui.text_disabled("  Range: 0.0 - 1.0 (maps to 0 to 2)");
            
            ui.text("/rustjay/color/brightness");
            ui.text_disabled("  Range: 0.0 - 1.0 (maps to 0 to 2)");
            
            ui.text("/rustjay/color/enabled");
            ui.text_disabled("  Range: 0.0 or 1.0");
            ui.unindent();
        }
        
        if ui.collapsing_header("Audio", imgui::TreeNodeFlags::empty()) {
            ui.indent();
            ui.text("/rustjay/audio/amplitude");
            ui.text_disabled("  Range: 0.0 - 1.0 (maps to 0 to 5)");
            
            ui.text("/rustjay/audio/smoothing");
            ui.text_disabled("  Range: 0.0 - 1.0");
            
            ui.text("/rustjay/audio/enabled");
            ui.text_disabled("  Range: 0.0 or 1.0");
            ui.unindent();
        }
        
        if ui.collapsing_header("Output", imgui::TreeNodeFlags::empty()) {
            ui.indent();
            ui.text("/rustjay/output/fullscreen");
            ui.text_disabled("  Range: 0.0 or 1.0");
            
            ui.text("/rustjay/output/width");
            ui.text_disabled("  Range: 0.0 - 1.0 (maps to 320 to 4096)");
            
            ui.text("/rustjay/output/height");
            ui.text_disabled("  Range: 0.0 - 1.0 (maps to 240 to 2160)");
            ui.unindent();
        }
        
        ui.separator();
        
        // Message log
        ui.text("Recent Messages");
        ui.text_disabled("(Last 100 OSC messages will appear here)");
    }

    /// Build the Web tab
    fn build_web_tab(&mut self, ui: &imgui::Ui) {
        ui.text("Web Remote Control");
        ui.separator();
        
        // Get current state
        let (enabled, port) = {
            let state = self.shared_state.lock().unwrap();
            (state.web_enabled, state.web_port)
        };
        
        // Status indicator
        let status_color = if enabled { [0.0, 1.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0, 1.0] };
        let status_text = if enabled { "Running" } else { "Stopped" };
        
        ui.text("Server Status: ");
        ui.same_line();
        ui.text_colored(status_color, status_text);
        
        ui.separator();
        
        // Port configuration
        ui.text("Port:");
        ui.same_line();
        
        let mut port_i32 = port as i32;
        ui.set_next_item_width(100.0);
        if ui.input_int("##web_port", &mut port_i32).build() {
            let new_port = port_i32.clamp(1024, 65535) as u16;
            if new_port != port {
                let mut state = self.shared_state.lock().unwrap();
                state.web_command = WebCommand::SetPort(new_port);
            }
        }
        
        ui.same_line();
        
        // Start/Stop button
        if enabled {
            if ui.button("Stop Server") {
                let mut state = self.shared_state.lock().unwrap();
                state.web_command = WebCommand::Stop;
            }
        } else {
            if ui.button("Start Server") {
                let mut state = self.shared_state.lock().unwrap();
                state.web_command = WebCommand::Start;
            }
        }
        
        ui.separator();
        
        // URL display
        if enabled {
            ui.text_colored([0.0, 1.0, 1.0, 1.0], "Access URL:");
            
            let local_ip = get_local_ip().unwrap_or_else(|| "localhost".to_string());
            let url = format!("http://{}:{}/rustjay", local_ip, port);
            
            ui.text(&url);
            
            if ui.button("Copy URL to Clipboard") {
                // TODO: Implement clipboard copy
                ui.tooltip_text("URL copied!");
            }
            
            ui.separator();
            
            ui.text("Scan with your phone or open in a browser on the same network.");
            ui.text_disabled("The web interface provides real-time control of all parameters.");
        } else {
            ui.text_disabled("Start the server to get the access URL.");
        }
        
        ui.separator();
        
        // Features list
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Features:");
        ui.bullet_text("Real-time bidirectional sync");
        ui.bullet_text("Works on any device with a browser");
        ui.bullet_text("Mobile-optimized touch interface");
        ui.bullet_text("Auto-generated controls for all parameters");
        ui.bullet_text("Multiple clients can connect simultaneously");
    }

    /// Build the LFO control window
    fn build_lfo_window(&mut self, ui: &imgui::Ui) {
        let mut show_window = {
            let state = self.shared_state.lock().unwrap();
            state.lfo.show_window
        };
        let mut should_close = false;
        
        ui.window("LFO Control")
            .size([520.0, 480.0], imgui::Condition::FirstUseEver)
            .opened(&mut show_window)
            .build(|| {
                ui.text("Low Frequency Oscillator Modulation");
                ui.text_disabled("Each LFO can modulate Hue, Saturation, or Brightness");
                ui.separator();
                
                // Get BPM for display
                let bpm = {
                    let state = self.shared_state.lock().unwrap();
                    state.audio.bpm
                };
                ui.text(&format!("Tempo: {:.1} BPM", bpm));
                ui.spacing();
                
                let waveforms = ["Sine", "Triangle", "Ramp Up", "Ramp Down", "Square"];
                let targets = ["None", "Hue", "Saturation", "Brightness"];
                let divisions = ["1/16", "1/8", "1/4", "1/2", "1", "2", "4", "8"];
                
                // Iterate through each LFO bank
                for i in 0..3 {
                    let mut needs_update = false;
                    
                    // Get current values (store originals for change detection)
                    let (enabled, mut rate, mut amplitude, mut waveform_idx, 
                         tempo_sync, current_division, mut phase_offset, mut target_idx) = {
                        let state = self.shared_state.lock().unwrap();
                        let bank = &state.lfo.bank.lfos[i];
                        let wf_idx = bank.waveform as usize;
                        let tgt_idx = match bank.target {
                            LfoTarget::None => 0,
                            LfoTarget::HueShift => 1,
                            LfoTarget::Saturation => 2,
                            LfoTarget::Brightness => 3,
                        };
                        (bank.enabled, bank.rate, bank.amplitude, wf_idx,
                         bank.tempo_sync, bank.division, bank.phase_offset, tgt_idx)
                    };
                    // Local mutable copy for UI
                    let mut division_idx = current_division;
                    
                    let header_color = if enabled {
                        [0.2, 0.8, 0.2, 1.0]
                    } else {
                        [0.5, 0.5, 0.5, 1.0]
                    };
                    
                    let _id_token = ui.push_id(format!("lfo_{}", i));
                    
                    if ui.collapsing_header(
                        &format!("LFO {} - {}", i + 1, if enabled { "ON" } else { "OFF" }),
                        imgui::TreeNodeFlags::DEFAULT_OPEN
                    ) {
                        // Enable/disable checkbox
                        let mut enabled_mut = enabled;
                        if ui.checkbox("Enabled", &mut enabled_mut) && enabled_mut != enabled {
                            let mut state = self.shared_state.lock().unwrap();
                            state.lfo.bank.lfos[i].enabled = enabled_mut;
                        }
                        
                        ui.separator();
                        
                        // Rate control
                        if tempo_sync {
                            // Beat division dropdown
                            let _width = ui.push_item_width(100.0);
                            if ui.combo_simple_string(
                                "Beat Division", 
                                &mut division_idx, 
                                &divisions
                            ) && division_idx != current_division {
                                let mut state = self.shared_state.lock().unwrap();
                                state.lfo.bank.lfos[i].division = division_idx;
                            }
                        } else {
                            // Free rate slider
                            let _width = ui.push_item_width(200.0);
                            if ui.slider("Rate (Hz)", 0.01, 10.0, &mut rate) {
                                let mut state = self.shared_state.lock().unwrap();
                                state.lfo.bank.lfos[i].rate = rate;
                            }
                        }
                        
                        // Tempo sync toggle
                        let mut sync = tempo_sync;
                        if ui.checkbox("Tempo Sync", &mut sync) && sync != tempo_sync {
                            let mut state = self.shared_state.lock().unwrap();
                            state.lfo.bank.lfos[i].tempo_sync = sync;
                        }
                        ui.same_line();
                        if tempo_sync {
                            ui.text_disabled(&format!("= {:.2} Hz", 
                                beat_division_to_hz(division_idx, bpm)));
                        }
                        
                        ui.separator();
                        
                        // Waveform selection
                        ui.text("Waveform:");
                        for (wf_idx, wf_name) in waveforms.iter().enumerate() {
                            if wf_idx > 0 {
                                ui.same_line();
                            }
                            let is_selected = waveform_idx == wf_idx;
                            if is_selected {
                                let _color = ui.push_style_color(
                                    imgui::StyleColor::Button, 
                                    [0.2, 0.6, 0.8, 1.0]
                                );
                                ui.button(wf_name);
                            } else if ui.button(wf_name) {
                                let mut state = self.shared_state.lock().unwrap();
                                state.lfo.bank.lfos[i].waveform = match wf_idx {
                                        0 => Waveform::Sine,
                                        1 => Waveform::Triangle,
                                        2 => Waveform::Ramp,
                                        3 => Waveform::Saw,
                                        4 => Waveform::Square,
                                        _ => Waveform::Sine,
                                    };
                            }
                        }
                        
                        // Phase offset
                        let _width = ui.push_item_width(200.0);
                        let phase_degrees = phase_offset * 360.0;
                        let mut phase_degrees_mut = phase_degrees;
                        if ui.slider("Phase Offset (°)", 0.0, 360.0, &mut phase_degrees_mut) {
                            let mut state = self.shared_state.lock().unwrap();
                            state.lfo.bank.lfos[i].phase_offset = phase_degrees_mut;
                        }
                        ui.same_line();
                        ui.text_disabled("(0° = on beat)");
                        
                        // Amplitude
                        if ui.slider("Amplitude", -1.0, 1.0, &mut amplitude) {
                            let mut state = self.shared_state.lock().unwrap();
                            state.lfo.bank.lfos[i].amplitude = amplitude;
                        }
                        
                        ui.separator();
                        
                        // Target parameter
                        let _width = ui.push_item_width(120.0);
                        if ui.combo_simple_string("Target", &mut target_idx, &targets) {
                            let new_target = match target_idx {
                                1 => LfoTarget::HueShift,
                                2 => LfoTarget::Saturation,
                                3 => LfoTarget::Brightness,
                                _ => LfoTarget::None,
                            };
                            let mut state = self.shared_state.lock().unwrap();
                            state.lfo.bank.lfos[i].target = new_target;
                        }
                        
                        // Visual indicator of modulation direction
                        if enabled && target_idx > 0 {
                            ui.spacing();
                            let indicator = match (amplitude > 0.0, target_idx) {
                                (true, 1) => "→ Shifts hue RIGHT",
                                (false, 1) => "← Shifts hue LEFT",
                                (true, 2) => "↑ Increases saturation",
                                (false, 2) => "↓ Decreases saturation",
                                (true, 3) => "☀ Increases brightness",
                                (false, 3) => "☾ Decreases brightness",
                                _ => "",
                            };
                            ui.text_colored([0.8, 0.8, 0.2, 1.0], indicator);
                        }
                    }
                    
                    // ID token dropped automatically
                }
                
                ui.separator();
                if ui.button("Close") {
                    should_close = true;
                }
                ui.same_line();
                if ui.button("Reset All LFOs") {
                    let mut state = self.shared_state.lock().unwrap();
                    state.lfo.bank.reset_all();
                }
            });
        
        // Update show_window in state if changed
        if !show_window || should_close {
            let mut state = self.shared_state.lock().unwrap();
            state.lfo.show_window = false;
        }
    }
}

/// Get local IP address helper
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                return Some(addr.ip().to_string());
            }
        }
    }
    None
}
