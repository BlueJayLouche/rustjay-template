//! # Control GUI
//!
//! Main ImGui interface for controlling the application.

#![allow(deprecated)]

use crate::core::{GuiTab, HsbParams, InputCommand, OutputCommand, SharedState};
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

    // Selection state
    selected_webcam: usize,
    selected_ndi: usize,
    #[cfg(target_os = "macos")]
    selected_syphon: usize,

    // NDI output name
    ndi_output_name: String,

    // Syphon output name (macOS)
    #[cfg(target_os = "macos")]
    syphon_output_name: String,

    // Preview texture IDs
    pub input_preview_texture_id: Option<imgui::TextureId>,
    pub output_preview_texture_id: Option<imgui::TextureId>,
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

        Ok(Self {
            shared_state,
            webcam_devices: Vec::new(),
            ndi_sources: Vec::new(),
            #[cfg(target_os = "macos")]
            syphon_servers: Vec::new(),
            selected_webcam: 0,
            selected_ndi: 0,
            #[cfg(target_os = "macos")]
            selected_syphon: 0,
            ndi_output_name: ndi_name,
            #[cfg(target_os = "macos")]
            syphon_output_name: syphon_name,
            input_preview_texture_id: None,
            output_preview_texture_id: None,
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

    /// Refresh device lists
    pub fn refresh_devices(&mut self, input_manager: &mut InputManager) {
        input_manager.refresh_devices();
        self.webcam_devices = input_manager.webcam_devices().to_vec();
        self.ndi_sources = input_manager.ndi_sources().to_vec();
        #[cfg(target_os = "macos")]
        {
            self.syphon_servers = input_manager.syphon_servers().to_vec();
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
        let (mut hsb, mut enabled) = {
            let state = self.shared_state.lock().unwrap();
            (state.hsb_params, state.color_enabled)
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
            }

            // Saturation
            ui.text("Saturation");
            if ui.slider_config("Saturation", 0.0, 2.0)
                .display_format("%.2fx")
                .build(&mut hsb.saturation)
            {
                let mut state = self.shared_state.lock().unwrap();
                state.hsb_params.saturation = hsb.saturation;
            }

            // Brightness
            ui.text("Brightness");
            if ui.slider_config("Brightness", 0.0, 2.0)
                .display_format("%.2fx")
                .build(&mut hsb.brightness)
            {
                let mut state = self.shared_state.lock().unwrap();
                state.hsb_params.brightness = hsb.brightness;
            }

            ui.spacing();
            ui.separator();
            ui.spacing();

            // Reset button
            if ui.button("Reset to Default") {
                hsb.reset();
                let mut state = self.shared_state.lock().unwrap();
                state.hsb_params = hsb;
            }
        } else {
            ui.text_disabled("Color adjustment is disabled");
        }
    }

    /// Build the Audio tab
    fn build_audio_tab(&mut self, ui: &imgui::Ui) {
        let (mut enabled, mut amplitude, mut smoothing, fft, volume) = {
            let state = self.shared_state.lock().unwrap();
            (
                state.audio.enabled,
                state.audio.amplitude,
                state.audio.smoothing,
                state.audio.fft,
                state.audio.volume,
            )
        };

        ui.text("Audio Analysis");
        ui.separator();

        // Enable/disable
        if ui.checkbox("Enable Audio Analysis", &mut enabled) {
            let mut state = self.shared_state.lock().unwrap();
            state.audio.enabled = enabled;
        }

        ui.spacing();

        if enabled {
            // Amplitude
            ui.text("Input Amplitude");
            if ui.slider("Amplitude", 0.1, 5.0, &mut amplitude) {
                let mut state = self.shared_state.lock().unwrap();
                state.audio.amplitude = amplitude;
            }

            // Smoothing
            ui.text("Smoothing");
            if ui.slider("Smoothing", 0.0, 0.95, &mut smoothing) {
                let mut state = self.shared_state.lock().unwrap();
                state.audio.smoothing = smoothing;
            }

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
        } else {
            ui.text_disabled("Audio analysis is disabled");
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

        ui.text("Keyboard Shortcuts:");
        ui.bullet_text("Shift+F - Toggle Fullscreen");
        ui.bullet_text("Escape - Exit Application");

        ui.separator();

        ui.text("Performance:");
        ui.text_disabled("All textures use native BGRA format for optimal macOS performance.");
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
}
