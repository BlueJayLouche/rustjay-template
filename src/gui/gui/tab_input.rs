use super::ControlGui;
use crate::core::InputCommand;

impl ControlGui {
    /// Build the Input tab
    pub(super) fn build_input_tab(&mut self, ui: &imgui::Ui) {
        let (is_active, source_type, source_name) = {
            let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
            state.input_command = InputCommand::StopInput;
        }
    }

    /// Build the input preview
    pub(super) fn build_input_preview(&mut self, ui: &imgui::Ui, available_size: [f32; 2]) {
        if let Some(texture_id) = self.input_preview_texture_id {
            let (input_width, input_height) = {
                let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
    pub(super) fn build_output_preview(&mut self, ui: &imgui::Ui, available_size: [f32; 2]) {
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
