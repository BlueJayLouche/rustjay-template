//! # wgpu Renderer
//!
//! Main rendering engine with HSB color manipulation.

use crate::core::{HsbParams, SharedState};
use crate::core::vertex::Vertex;
use crate::engine::texture::{InputTexture, Texture};
use crate::output::OutputManager;

use anyhow::Result;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

/// GPU representation of HSB parameters
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct HsbUniforms {
    /// hue_shift, saturation, brightness, _padding
    values: [f32; 4],
}

impl From<&HsbParams> for HsbUniforms {
    fn from(params: &HsbParams) -> Self {
        Self {
            values: [params.hue_shift, params.saturation, params.brightness, 0.0],
        }
    }
}

/// Main wgpu rendering engine
pub struct WgpuEngine {
    #[allow(dead_code)]
    instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,

    // Window size
    window_width: u32,
    window_height: u32,

    // Shared state
    shared_state: Arc<std::sync::Mutex<SharedState>>,

    // Render pipeline
    render_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    uniform_bind_group_layout: wgpu::BindGroupLayout,

    // Render target (internal resolution)
    pub render_target: Texture,

    // Input texture
    pub input_texture: InputTexture,

    // Vertex buffer
    vertex_buffer: wgpu::Buffer,

    // Uniform buffer for HSB parameters
    hsb_uniform_buffer: wgpu::Buffer,

    // Frame counter
    frame_count: u64,

    // FPS tracking
    fps_last_time: std::time::Instant,
    fps_frame_count: u32,
    fps_current: f32,

    // Output manager (NDI, Syphon)
    output_manager: OutputManager,
}

impl WgpuEngine {
    /// Create a new wgpu engine
    pub async fn new(
        instance: &wgpu::Instance,
        window: Arc<Window>,
        shared_state: Arc<std::sync::Mutex<SharedState>>,
    ) -> Result<Self> {
        let size = window.inner_size();

        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: Some("Device"),
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                },
            )
            .await?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| *f == wgpu::TextureFormat::Bgra8UnormSrgb || *f == wgpu::TextureFormat::Bgra8Unorm)
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // Create render target at fixed internal resolution
        let render_target = Texture::create_render_target(&device, 1920, 1080, "Render Target");

        // Create input texture manager
        let input_texture = InputTexture::new(Arc::clone(&device), Arc::clone(&queue));

        // Create shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Main Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/main.wgsl").into()),
        });

        // Create texture bind group layout
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Create uniform bind group layout for HSB parameters
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create vertex buffer
        let vertices = Vertex::quad_vertices();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Create HSB uniform buffer
        let hsb_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HSB Uniform Buffer"),
            size: std::mem::size_of::<HsbUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            instance: instance.clone(),
            adapter,
            device: Arc::clone(&device),
            queue: Arc::clone(&queue),
            surface,
            surface_config,
            window_width: size.width,
            window_height: size.height,
            shared_state,
            render_pipeline,
            texture_bind_group_layout,
            uniform_bind_group_layout,
            render_target,
            input_texture,
            vertex_buffer,
            hsb_uniform_buffer,
            frame_count: 0,
            fps_last_time: std::time::Instant::now(),
            fps_frame_count: 0,
            fps_current: 0.0,
            output_manager: OutputManager::new(),
        })
    }

    /// Resize the surface
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.window_width = width;
            self.window_height = height;
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
            log::debug!("Resized to {}x{}", width, height);
        }
    }

    /// Start NDI output
    pub fn start_ndi_output(&mut self, name: &str, include_alpha: bool) -> anyhow::Result<()> {
        self.output_manager.start_ndi(
            name,
            self.render_target.width,
            self.render_target.height,
            include_alpha,
        )?;
        Ok(())
    }

    /// Stop NDI output
    pub fn stop_ndi_output(&mut self) {
        self.output_manager.stop_ndi();
    }

    /// Start Syphon output (macOS only)
    #[cfg(target_os = "macos")]
    pub fn start_syphon_output(&mut self, server_name: &str) -> anyhow::Result<()> {
        self.output_manager.start_syphon(
            server_name,
            Arc::clone(&self.device),
            Arc::clone(&self.queue),
        )?;
        Ok(())
    }

    /// Stop Syphon output (macOS only)
    #[cfg(target_os = "macos")]
    pub fn stop_syphon_output(&mut self) {
        self.output_manager.stop_syphon();
    }

    /// Render a frame
    pub fn render(&mut self) {
        // Get current HSB parameters and apply modulations
        let (hsb_params, color_enabled) = {
            let state = self.shared_state.lock().unwrap();
            
            // Start with base values
            let base_hue = state.audio_routing.base_hue;
            let base_sat = state.audio_routing.base_saturation;
            let base_bright = state.audio_routing.base_brightness;
            
            // Apply audio routing modulation
            let (mut hue, mut sat, mut bright) = if state.audio_routing.enabled {
                state.audio_routing.matrix.apply_to_hsb(base_hue, base_sat, base_bright)
            } else {
                (base_hue, base_sat, base_bright)
            };
            
            // Apply LFO modulation (additive)
            let (hue_mod, sat_mod, bright_mod) = state.lfo.bank.get_hsb_modulations();
            hue = (hue + hue_mod * 90.0).clamp(-180.0, 180.0);
            sat = (sat + sat_mod).clamp(0.0, 2.0);
            bright = (bright + bright_mod).clamp(0.0, 2.0);
            
            (HsbParams { hue_shift: hue, saturation: sat, brightness: bright }, state.color_enabled)
        };

        // Get surface texture
        let surface_texture = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(_) => {
                self.surface.configure(&self.device, &self.surface_config);
                return;
            }
        };

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Ensure we have an input texture
        if self.input_texture.texture.is_none() {
            self.input_texture.ensure_size(1920, 1080);
            if let Some(ref tex) = self.input_texture.texture {
                tex.clear_to_black(&self.queue);
            }
        }

        // Create texture bind group
        let texture_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        self.input_texture.view().expect("Input texture not initialized"),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(
                        &self.input_texture.texture.as_ref().unwrap().sampler,
                    ),
                },
            ],
        });

        // Update HSB uniform buffer
        let hsb_uniforms: HsbUniforms = if color_enabled {
            (&hsb_params).into()
        } else {
            // Identity HSB (no change)
            HsbUniforms {
                values: [0.0, 1.0, 1.0, 0.0],
            }
        };
        self.queue.write_buffer(
            &self.hsb_uniform_buffer,
            0,
            bytemuck::bytes_of(&hsb_uniforms),
        );

        // Create uniform bind group
        let uniform_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &self.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.hsb_uniform_buffer.as_entire_binding(),
            }],
        });

        // Render to render target
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.render_target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_bind_group(0, &texture_bind_group, &[]);
            render_pass.set_bind_group(1, &uniform_bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        // Blit to surface
        self.blit_to_surface(&mut encoder, &surface_view, &self.render_target.view);

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));

        // Present
        surface_texture.present();

        // Submit to outputs
        self.output_manager
            .submit_frame(&self.render_target.texture, &self.device, &self.queue);

        // Update FPS tracking
        self.fps_frame_count += 1;
        let elapsed = self.fps_last_time.elapsed();
        if elapsed.as_secs_f32() >= 0.5 {
            self.fps_current = self.fps_frame_count as f32 / elapsed.as_secs_f32();
            self.fps_frame_count = 0;
            self.fps_last_time = std::time::Instant::now();
            
            // Update shared state with FPS
            if let Ok(mut state) = self.shared_state.lock() {
                state.performance.fps = self.fps_current;
                state.performance.frame_time_ms = if self.fps_current > 0.0 {
                    1000.0 / self.fps_current
                } else {
                    0.0
                };
            }
        }

        self.frame_count += 1;
    }

    /// Blit render target to surface
    fn blit_to_surface(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
        source_view: &wgpu::TextureView,
    ) {
        // Simple blit using a temporary render pass with a copy shader
        // For simplicity, we reuse the main pipeline but with different target
        // In production, you might want a dedicated blit shader

        // Create temporary bind group and pipeline for blitting
        let bind_group_layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Blit Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blit Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // Simple blit shader
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blit Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
                struct VertexOutput {
                    @builtin(position) position: vec4<f32>,
                    @location(0) texcoord: vec2<f32>,
                };

                @vertex
                fn vs_main(@location(0) position: vec2<f32>, @location(1) texcoord: vec2<f32>) -> VertexOutput {
                    var out: VertexOutput;
                    out.position = vec4<f32>(position, 0.0, 1.0);
                    out.texcoord = texcoord;
                    return out;
                }

                @group(0) @binding(0)
                var source_tex: texture_2d<f32>;
                @group(0) @binding(1)
                var source_sampler: sampler;

                @fragment
                fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                    return textureSample(source_tex, source_sampler, in.texcoord);
                }
            "#
                .into(),
            ),
        });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Blit Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Blit Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Vertex::desc()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.surface_config.format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Blit Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }
}
