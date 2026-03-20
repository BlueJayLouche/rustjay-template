use super::ControlGui;
use crate::core::OutputCommand;

impl ControlGui {
    /// Build the Output tab
    pub(super) fn build_output_tab(&mut self, ui: &imgui::Ui) {
        let (ndi_active, fullscreen) = {
            let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
            (state.ndi_output.is_active, state.output_fullscreen)
        };

        ui.text("Output Settings");
        ui.separator();

        // Fullscreen toggle
        let mut fs = fullscreen;
        if ui.checkbox("Fullscreen Output", &mut fs) {
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                state.ndi_output.stream_name = self.ndi_output_name.clone();
                state.output_command = OutputCommand::StartNdi;
            }
        } else {
            if ui.button("Stop NDI Output") {
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                state.syphon_output.enabled
            };

            ui.text_colored([1.0, 0.5, 0.0, 1.0], "Syphon Output (macOS)");
            ui.input_text("Server Name", &mut self.syphon_output_name).build();

            if !syphon_enabled {
                if ui.button("Start Syphon Output") {
                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                    state.syphon_output.server_name = self.syphon_output_name.clone();
                    state.output_command = OutputCommand::StartSyphon;
                }
            } else {
                if ui.button("Stop Syphon Output") {
                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                    state.output_command = OutputCommand::StopSyphon;
                }
                ui.text_colored([0.0, 1.0, 0.0, 1.0], "Syphon Active");
            }
        }

        // Spout Output (Windows)
        #[cfg(target_os = "windows")]
        {
            ui.spacing();
            ui.separator();
            ui.spacing();

            let spout_active = {
                let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                // Mirror syphon_output pattern once SpoutOutputState is added to SharedState
                // For now read from the output manager via command round-trip
                matches!(state.output_command, OutputCommand::StartSpout { .. })
            };

            ui.text_colored([0.3, 0.6, 1.0, 1.0], "Spout Output (Windows)");
            ui.input_text("Spout Sender Name##out", &mut self.spout_output_name).build();

            if !spout_active {
                if ui.button("Start Spout Output") {
                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                    state.output_command = OutputCommand::StartSpout {
                        sender_name: self.spout_output_name.clone(),
                    };
                }
            } else {
                if ui.button("Stop Spout Output") {
                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                    state.output_command = OutputCommand::StopSpout;
                }
                ui.text_colored([0.0, 1.0, 0.0, 1.0], "Spout Active");
            }
        }

        // V4L2 Loopback Output (Linux)
        #[cfg(target_os = "linux")]
        {
            ui.spacing();
            ui.separator();
            ui.spacing();

            ui.text_colored([0.8, 0.8, 0.2, 1.0], "V4L2 Loopback Output (Linux)");
            ui.text_disabled("Requires v4l2loopback kernel module");
            ui.input_text("Device Path", &mut self.v4l2_device_path).build();

            if ui.button("Start V4L2 Output") {
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                state.output_command = OutputCommand::StartV4l2 {
                    device_path: self.v4l2_device_path.clone(),
                };
            }
            ui.same_line();
            if ui.button("Stop V4L2 Output") {
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                state.output_command = OutputCommand::StopV4l2;
            }
        }
    }
}
