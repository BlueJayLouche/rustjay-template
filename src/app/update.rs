use super::App;
use crate::core::InputType;

impl App {
    /// Update input and upload frames to GPU
    pub(super) fn update_input(&mut self) {
        if let Some(ref mut manager) = self.input_manager {
            // Detect NDI source loss and surface it in shared state
            #[cfg(feature = "ndi")]
            if manager.input_type() == InputType::Ndi && manager.is_ndi_source_lost() {
                log::warn!("[NDI] Source lost — clearing active input state");
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                state.input.is_active = false;
                state.input.source_name = "Signal lost".to_string();
            }

            manager.update();

            // Handle Syphon texture (GPU blit path)
            #[cfg(target_os = "macos")]
            if manager.input_type() == InputType::Syphon {
                if manager.has_frame() {
                    let dims = manager.syphon_output_texture()
                        .map(|t| (t.width(), t.height()));

                    if let Some((width, height)) = dims {
                        if let Some(texture) = manager.syphon_output_texture() {
                            if let Some(ref mut engine) = self.output_engine {
                                // Zero-copy: point the renderer at the Syphon
                                // output texture directly — no GPU copy needed.
                                engine.input_texture.set_external_texture(texture);
                            }
                        }
                        manager.clear_syphon_frame();
                        let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                        state.input.width = width;
                        state.input.height = height;
                    }
                }
            } else {
                // CPU fallback path
                if let Some(frame_data) = manager.take_frame() {
                    let (width, height) = manager.resolution();

                    if let Some(ref mut engine) = self.output_engine {
                        engine.input_texture.update(&frame_data, width, height);
                    }

                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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

                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                    state.input.width = width;
                    state.input.height = height;
                }
            }
        }
    }

    /// Update audio analysis
    pub(super) fn update_audio(&mut self) {
        // Reconnect if the stream reported an error (e.g. device unplugged)
        if let Some(ref analyzer) = self.audio_analyzer {
            if analyzer.take_stream_error() {
                let device = {
                    let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                    state.audio.selected_device.clone()
                };
                log::warn!("[Audio] Stream error detected — attempting reconnect (device: {:?})", device);
                drop(analyzer); // release immutable borrow before we need mutable
                if let Some(ref mut analyzer) = self.audio_analyzer {
                    if let Err(e) = analyzer.start_with_device(device.as_deref()) {
                        log::error!("[Audio] Reconnect failed: {}", e);
                    }
                }
            }
        }

        // Sync settings from shared state TO analyzer
        if let Some(ref analyzer) = self.audio_analyzer {
            let (amplitude, smoothing, normalize, pink_noise) = {
                let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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

            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
            if state.audio.enabled {
                state.audio.fft = fft;
                state.audio.volume = volume;
                state.audio.beat = beat;
                state.audio.beat_phase = phase;

                // Process audio routing (updates internal smoothed values)
                // Actual application of modulation happens in render step
                if state.audio_routing.enabled {
                    let delta_time = self.frame_delta_time;
                    state.audio_routing.matrix.process(&fft, delta_time);
                }
            }
        }
    }

    /// Update LFO phases (modulation applied in final composite step)
    pub(super) fn update_lfo(&mut self) {
        let delta_time = self.frame_delta_time;
        let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
        let bpm = state.audio.bpm;
        let beat_phase = state.audio.beat_phase;
        state.lfo.bank.update(bpm, delta_time, beat_phase);
    }

    /// Update MIDI - apply mapped values to state (only when changed)
    pub(super) fn update_midi(&mut self) {
        // Periodically check whether the connected MIDI device is still present
        if let Some(ref mut manager) = self.midi_manager {
            if let Some(false) = manager.check_device_available_if_needed() {
                let name = manager.state().lock()
                    .map(|s| s.selected_device.clone().unwrap_or_default())
                    .unwrap_or_default();
                log::warn!("[MIDI] Device '{}' no longer available — disconnecting", name);
                manager.disconnect();
                // Surface the disconnection in shared state so the GUI can show a warning
                let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                state.midi_command = crate::core::MidiCommand::None;
            }
        }

        if let Some(ref manager) = self.midi_manager {
            // Collect only dirty values
            let mut dirty_values: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

            {
                let midi_state_arc = manager.state();
                let mut midi_state = midi_state_arc.lock().unwrap_or_else(|e| e.into_inner());

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
    pub(super) fn update_osc(&mut self) {
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

    /// Update web server with current state
    pub(super) fn update_web(&mut self) {
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

    /// Poll for background device discovery completion and update the GUI when done.
    pub(super) fn poll_device_discovery(&mut self) {
        let done = self.input_manager.as_mut().map_or(false, |m| m.poll_discovery());
        if done {
            if let (Some(ref manager), Some(ref mut gui)) =
                (self.input_manager.as_ref(), self.control_gui.as_mut())
            {
                gui.update_device_lists(manager);
            }
            self.shared_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .input_discovering = false;
        }
    }

    /// Update preview textures for GUI
    pub(super) fn update_preview_textures(&mut self) {
        // Skip all GPU preview copies when previews are hidden — saves overhead
        let show_preview = self.shared_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .show_preview;
        if !show_preview {
            return;
        }

        // When Syphon is active, `input_texture.texture` is None (zero-copy external path).
        // Fall back to `render_target` so the input preview still shows something.
        let input_uses_external = self.output_engine.as_ref()
            .map(|e| e.input_texture.has_external_texture())
            .unwrap_or(false);

        if let (Some(ref mut renderer), Some(ref gui)) =
            (self.imgui_renderer.as_mut(), self.control_gui.as_ref())
        {
            // Single encoder/submit for both preview copies.
            let mut encoder = renderer.device().create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("Preview Encoder") },
            );
            let mut any_work = false;

            // Update input preview
            {
                let input_src = if input_uses_external {
                    // Syphon zero-copy path: use render_target as a proxy
                    self.output_engine.as_ref().map(|e| &e.render_target.texture)
                } else {
                    self.output_engine
                        .as_ref()
                        .and_then(|e| e.input_texture.texture.as_ref().map(|t| &t.texture))
                };
                if let (Some(tex), Some(preview_id)) = (input_src, gui.input_preview_texture_id) {
                    renderer.update_preview_texture(preview_id, tex, &mut encoder);
                    any_work = true;
                }
            }

            // Update output preview
            {
                let output_src = self.output_engine.as_ref().map(|e| &e.render_target.texture);
                if let (Some(tex), Some(preview_id)) = (output_src, gui.output_preview_texture_id) {
                    renderer.update_preview_texture(preview_id, tex, &mut encoder);
                    any_work = true;
                }
            }

            if any_work {
                renderer.queue().submit(std::iter::once(encoder.finish()));
            }
        }
    }
}
