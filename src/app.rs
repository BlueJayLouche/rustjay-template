//! # Application Handler
//!
//! Dual-window application handler implementing winit's ApplicationHandler.

use crate::audio::AudioAnalyzer;
use crate::core::{InputCommand, InputType, OutputCommand, SharedState};
use crate::engine::WgpuEngine;
use crate::gui::{ControlGui, ImGuiRenderer};
use crate::input::InputManager;
use crate::output::{OutputManager};

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

    // Modifier state
    shift_pressed: bool,
}

impl App {
    fn new(shared_state: Arc<std::sync::Mutex<SharedState>>) -> Self {
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
            shift_pressed: false,
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

    /// Update audio analysis
    fn update_audio(&mut self) {
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
            }
        }
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

                // Initialize ImGui renderer
                match pollster::block_on(ImGuiRenderer::new(
                    instance,
                    adapter,
                    device,
                    queue,
                    window,
                    1.0,
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
                                    event_loop.exit();
                                }
                                winit::keyboard::Key::Character(ch) => {
                                    let key = ch.to_lowercase();
                                    if self.shift_pressed && key == "f" {
                                        self.toggle_fullscreen();
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
                        event_loop.exit();
                    }
                    WindowEvent::Resized(size) => {
                        if let Some(ref mut renderer) = self.imgui_renderer {
                            renderer.resize(size.width, size.height);
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if let (Some(ref mut renderer), Some(ref mut gui)) =
                            (self.imgui_renderer.as_mut(), self.control_gui.as_mut())
                        {
                            let window_size = control_window.inner_size();
                            renderer.set_display_size(window_size.width as f32, window_size.height as f32);

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

        // Process commands
        self.process_input_commands();
        self.process_output_commands();

        // Update systems
        self.update_input();
        self.update_audio();

        // Request redraws
        if let Some(ref window) = self.output_window {
            window.request_redraw();
        }
        if let Some(ref window) = self.control_window {
            window.request_redraw();
        }
    }
}
