use super::ControlGui;
use crate::core::{OutputCommand, PresetCommand};

impl ControlGui {
    /// Build the Settings tab
    pub(super) fn build_settings_tab(&mut self, ui: &imgui::Ui) {
        ui.text("Application Settings");
        ui.separator();

        let mut ui_scale = {
            let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
            state.ui_scale
        };

        ui.text("UI Scale:");
        if ui.slider("Scale", 0.5, 2.0, &mut ui_scale) {
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
            let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
            let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                state.resolution.internal_width = self.pending_internal_width;
                state.resolution.internal_height = self.pending_internal_height;
                state.output_width = self.pending_output_width;
                state.output_height = self.pending_output_height;
            }
            // Signal resolution change command
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
            let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
            state.save_settings_requested = true;
            log::info!("Save settings requested from GUI");
        }

        ui.text_disabled("Settings are auto-saved on exit, or manually with this button.");
    }

    /// Build the Presets tab
    pub(super) fn build_presets_tab(&mut self, ui: &imgui::Ui) {
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
                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
}
