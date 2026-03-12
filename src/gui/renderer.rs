//! # ImGui Renderer
//!
//! wgpu-based renderer for Dear ImGui.

use anyhow::Result;
use std::sync::Arc;
use winit::window::Window;

/// ImGui renderer using wgpu
pub struct ImGuiRenderer {
    context: imgui::Context,
    renderer: imgui_wgpu::Renderer,
    platform: imgui_winit_support::WinitPlatform,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    preview_textures: Vec<(imgui::TextureId, imgui_wgpu::Texture)>,
}

impl ImGuiRenderer {
    /// Create a new ImGui renderer
    pub async fn new(
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        window: Arc<Window>,
        scale_factor: f64,
    ) -> Result<Self> {
        let size = window.inner_size();

        // Create surface
        let surface = instance.create_surface(window.clone())?;

        let surface_caps = surface.get_capabilities(adapter);
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

        // Create ImGui context
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);

        // Set up platform
        let mut platform = imgui_winit_support::WinitPlatform::init(&mut context);
        platform.attach_window(context.io_mut(), &window, imgui_winit_support::HiDpiMode::Rounded);

        // Set display size and scale
        context.io_mut().display_size = [size.width as f32, size.height as f32];
        context.io_mut().display_framebuffer_scale = [scale_factor as f32, scale_factor as f32];

        // Style configuration
        let style = context.style_mut();
        style.window_rounding = 4.0;
        style.frame_rounding = 4.0;
        style.scrollbar_rounding = 4.0;

        // Create renderer
        let renderer_config = imgui_wgpu::RendererConfig {
            texture_format: surface_format,
            ..Default::default()
        };

        let renderer = imgui_wgpu::Renderer::new(&mut context, &device, &queue, renderer_config);

        Ok(Self {
            context,
            renderer,
            platform,
            device,
            queue,
            surface,
            surface_config,
            preview_textures: Vec::new(),
        })
    }

    /// Handle window event
    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) {
        let window = self.platform.window();
        self.platform.handle_event(self.context.io_mut(), window, event);
    }

    /// Set display size
    pub fn set_display_size(&mut self, width: f32, height: f32) {
        self.context.io_mut().display_size = [width, height];
    }

    /// Resize surface
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    /// Create a preview texture for ImGui display
    pub fn create_preview_texture(&mut self, width: u32, height: u32) -> imgui::TextureId {
        let texture_config = imgui_wgpu::TextureConfig {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            ..Default::default()
        };

        let texture = imgui_wgpu::Texture::new(&self.device, &self.renderer, texture_config);
        let texture_id = texture.id();
        self.preview_textures.push((texture_id, texture));
        texture_id
    }

    /// Update a preview texture with texture data
    pub fn update_preview_texture(
        &mut self,
        texture_id: imgui::TextureId,
        source_texture: &wgpu::Texture,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        // Find the ImGui texture
        if let Some((_, imgui_tex)) = self.preview_textures.iter_mut().find(|(id, _)| *id == texture_id) {
            // Copy from source texture to ImGui texture
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: source_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: imgui_tex.texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: source_texture.width().min(imgui_tex.width()),
                    height: source_texture.height().min(imgui_tex.height()),
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Render a frame
    pub fn render_frame<F>(&mut self, build_ui: F) -> Result<()>
    where
        F: FnOnce(&mut imgui::Ui),
    {
        // Prepare frame
        let io = self.context.io_mut();
        let window = self.platform.window();
        self.platform.prepare_frame(io, window)?;

        // Build UI
        let mut ui = self.context.frame();
        build_ui(&mut ui);

        // Get surface texture
        let surface_texture = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(_) => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
        };

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Create encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ImGui Encoder"),
            });

        // Render
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ImGui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.renderer
                .render(self.context.render(), &self.queue, &self.device, &mut render_pass)?;
        }

        // Submit
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        Ok(())
    }

    /// Get device reference
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get queue reference
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}
