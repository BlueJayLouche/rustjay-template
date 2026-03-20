use super::App;
use crate::gui::{ControlGui, ImGuiRenderer};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowAttributes;
use crate::engine::WgpuEngine;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create wgpu instance
        if self.wgpu_instance.is_none() {
            let backends = if cfg!(target_os = "macos") {
                wgpu::Backends::METAL
            } else {
                wgpu::Backends::all()
            };
            self.wgpu_instance = Some(wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends,
                ..Default::default()
            }));
        }
        let Some(instance) = self.wgpu_instance.as_ref() else { return; };

        // Create output window
        if self.output_window.is_none() {
            let (output_width, output_height, fullscreen) = {
                let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                (state.output_width, state.output_height, state.output_fullscreen)
            };

            let window_attrs = WindowAttributes::default()
                .with_title("RustJay Output")
                .with_inner_size(winit::dpi::LogicalSize::new(output_width, output_height))
                .with_resizable(true)
                .with_decorations(true);

            let window = match event_loop.create_window(window_attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    log::error!("Failed to create output window: {}", e);
                    event_loop.exit();
                    return;
                }
            };

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

                    // Now that we have GPU resources, initialize InputManager
                    if let (Some(ref mut manager), Some(ref device), Some(ref queue)) =
                        (self.input_manager.as_mut(), self.wgpu_device.as_ref(), self.wgpu_queue.as_ref())
                    {
                        manager.initialize(device, queue);
                        log::info!("InputManager initialized with GPU resources");
                    }
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

                let window = match event_loop.create_window(window_attrs) {
                    Ok(w) => Arc::new(w),
                    Err(e) => {
                        log::error!("Failed to create control window: {}", e);
                        return;
                    }
                };
                self.control_window = Some(Arc::clone(&window));

                let adapter = match self.wgpu_adapter.as_ref() {
                    Some(a) => a,
                    None => {
                        log::error!("wgpu adapter not initialized before control window");
                        return;
                    }
                };

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

                                self.control_gui = Some(gui);

                                // Queue an initial device refresh to run in about_to_wait()
                                // rather than calling refresh_devices() here. Running it
                                // inside resumed() would block NDI discovery for ~2 s and,
                                // before the syphon crate fix, also spun the NSRunLoop
                                // causing winit's re-entrancy guard to panic.
                                {
                                    let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                    state.input_command = crate::core::InputCommand::RefreshDevices;
                                }
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
        window_id: winit::window::WindowId,
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
                        let state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                            let _ = bank.apply_slot(1, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F2) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                            let _ = bank.apply_slot(2, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F3) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                            let _ = bank.apply_slot(3, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F4) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                            let _ = bank.apply_slot(4, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F5) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                            let _ = bank.apply_slot(5, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F6) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                            let _ = bank.apply_slot(6, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F7) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
                                            let _ = bank.apply_slot(7, &mut state);
                                        }
                                    }
                                }
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::F8) => {
                                    if self.shift_pressed {
                                        if let Some(ref mut bank) = self.preset_bank {
                                            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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
        // Compute real elapsed time since last frame (capped to avoid spiral-of-death).
        let now = std::time::Instant::now();
        self.frame_delta_time = now
            .duration_since(self.last_frame_time)
            .as_secs_f32()
            .clamp(0.001, 0.1);
        self.last_frame_time = now;

        // Process all pending subsystem commands
        self.dispatch_commands();

        // Check if background device discovery has finished
        self.poll_device_discovery();

        // Update systems
        self.update_input();
        self.update_audio();
        self.update_lfo();
        self.update_midi();
        self.update_osc();
        self.update_web();

        // Check for settings save request
        let should_save = {
            let mut state = self.shared_state.lock().unwrap_or_else(|e| e.into_inner());
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

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        log::info!("Event loop exiting — shutting down");

        // Save settings before tearing down anything
        self.save_settings();

        // Explicit ordered shutdown: stop producers before consumers
        // (Drop impls are the fallback, but ordering matters for clean logs)
        if let Some(ref mut analyzer) = self.audio_analyzer {
            analyzer.stop();
        }
        if let Some(ref mut manager) = self.midi_manager {
            manager.disconnect();
        }
        if let Some(ref mut server) = self.osc_server {
            server.stop();
        }
        if let Some(ref mut server) = self.web_server {
            server.stop();
        }
        if let Some(ref mut manager) = self.input_manager {
            manager.stop();
        }

        log::info!("Shutdown complete");
    }
}
