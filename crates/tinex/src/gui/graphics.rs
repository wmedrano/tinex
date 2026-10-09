use std::sync::Arc;

use anyhow::{Context, Result, bail};
use vello::peniko::Color;
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};
use winit::window::Window;

pub(super) struct Graphics {
    // Drop the surface and renderer before their render context.
    state: Option<SurfaceState>,
    context: RenderContext,
}

struct SurfaceState {
    surface: RenderSurface<'static>,
    renderer: Renderer,
}

/// An acquired frame that does not borrow the graphics state.
pub(super) struct Frame {
    texture: wgpu::SurfaceTexture,
    suboptimal: bool,
}

impl Graphics {
    pub(super) fn new() -> Self {
        Self {
            state: None,
            context: RenderContext::new(),
        }
    }

    pub(super) fn initialize(&mut self, window: &Arc<Window>) -> Result<()> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let surface = pollster::block_on(self.context.create_surface(
            Arc::clone(window),
            size.width,
            size.height,
            wgpu::PresentMode::AutoVsync,
        ))
        .context("Could not create GPU surface")?;
        let renderer = Renderer::new(
            &self.context.devices[surface.dev_id].device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )
        .context("Could not initialize Vello renderer")?;
        self.state = Some(SurfaceState { surface, renderer });
        window.request_redraw();
        Ok(())
    }

    pub(super) fn resize(&mut self, window: &Arc<Window>) -> Result<()> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let Some(state) = &mut self.state else {
            return self.initialize(window);
        };
        if (state.surface.config.width, state.surface.config.height) != (size.width, size.height) {
            self.context
                .resize_surface(&mut state.surface, size.width, size.height);
        }
        window.request_redraw();
        Ok(())
    }

    pub(super) fn suspend(&mut self) {
        self.state = None;
    }

    pub(super) fn is_ready(&self) -> bool {
        self.state.is_some()
    }

    pub(super) fn begin_frame(&mut self, window: &Arc<Window>) -> Result<Option<Frame>> {
        let Some(state) = &self.state else {
            return Ok(None);
        };
        let (texture, suboptimal) = match state.surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.context.configure_surface(&state.surface);
                window.request_redraw();
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.suspend();
                self.initialize(window)?;
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Validation => bail!("GPU surface validation failed"),
        };
        Ok(Some(Frame {
            texture,
            suboptimal,
        }))
    }

    pub(super) fn present(
        &mut self,
        frame: Frame,
        scene: &Scene,
        background: Color,
        window: &Window,
    ) -> Result<()> {
        let state = self
            .state
            .as_mut()
            .context("GPU surface unavailable for presentation")?;
        let device = &self.context.devices[state.surface.dev_id];
        state
            .renderer
            .render_to_texture(
                &device.device,
                &device.queue,
                scene,
                &state.surface.target_view,
                &RenderParams {
                    base_color: background,
                    width: state.surface.config.width,
                    height: state.surface.config.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .context("Could not render GUI")?;
        let view = frame.texture.texture.create_view(&Default::default());
        let mut encoder = device.device.create_command_encoder(&Default::default());
        state.surface.blitter.copy(
            &device.device,
            &mut encoder,
            &state.surface.target_view,
            &view,
        );
        device.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        frame.texture.present();
        if frame.suboptimal {
            self.context.configure_surface(&state.surface);
        }
        Ok(())
    }
}
