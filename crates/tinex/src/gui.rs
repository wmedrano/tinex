use anyhow::{Context, Result, bail};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};
use vello::kurbo::{Affine, BezPath, Ellipse, Point, Rect};
use vello::peniko::{Color, Fill};
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, Window, WindowId};

use crate::jack::TinexHandle;
use crate::tinex::track::Track;
use crate::tinex::{TinexNotification, TinexRequest};

pub fn run(handle: TinexHandle) -> Result<()> {
    let event_loop = EventLoop::new().context("Could not create GUI event loop")?;
    let mut app = App::new(handle);
    event_loop
        .run_app(&mut app)
        .context("GUI event loop failed")?;
    if let Some(error) = app.error.take() {
        return Err(error);
    }
    Ok(())
}

struct App {
    handle: TinexHandle,
    graphics: Option<Graphics>,
    window: Option<Arc<Window>>,
    context: RenderContext,
    scene: Scene,
    track_symbols: TrackSymbols,
    create_track_button: CreateTrackButton,
    next_update: Instant,
    error: Option<anyhow::Error>,
    active: bool,
}

impl App {
    fn new(handle: TinexHandle) -> Self {
        Self {
            handle,
            graphics: None,
            window: None,
            context: RenderContext::new(),
            scene: Scene::new(),
            track_symbols: TrackSymbols::default(),
            create_track_button: CreateTrackButton::default(),
            next_update: Instant::now(),
            error: None,
            active: false,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn initialize_graphics(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let Some(window) = &self.window else {
            return Ok(());
        };
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
        self.graphics = Some(Graphics { surface, renderer });
        window.request_redraw();
        Ok(())
    }

    fn resize(&mut self) -> Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if let Some(graphics) = &mut self.graphics {
            if (
                graphics.surface.config.width,
                graphics.surface.config.height,
            ) != (size.width, size.height)
            {
                self.context
                    .resize_surface(&mut graphics.surface, size.width, size.height);
            }
            window.request_redraw();
            Ok(())
        } else {
            self.initialize_graphics()
        }
    }

    fn draw(&mut self) -> Result<()> {
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let Some(graphics) = &mut self.graphics else {
            return Ok(());
        };
        let (frame, suboptimal) = match graphics.surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.context.configure_surface(&graphics.surface);
                window.request_redraw();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.graphics = None;
                return self.initialize_graphics();
            }
            wgpu::CurrentSurfaceTexture::Validation => bail!("GPU surface validation failed"),
        };

        let scale = window.scale_factor();
        self.draw_scene(size.to_logical(scale), scale);
        let graphics = self.graphics.as_mut().unwrap();
        let device = &self.context.devices[graphics.surface.dev_id];
        graphics
            .renderer
            .render_to_texture(
                &device.device,
                &device.queue,
                &self.scene,
                &graphics.surface.target_view,
                &RenderParams {
                    base_color: Color::from_rgb8(20, 24, 30),
                    width: graphics.surface.config.width,
                    height: graphics.surface.config.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .context("Could not render GUI")?;
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.device.create_command_encoder(&Default::default());
        graphics.surface.blitter.copy(
            &device.device,
            &mut encoder,
            &graphics.surface.target_view,
            &view,
        );
        device.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        frame.present();
        if suboptimal {
            self.context.configure_surface(&graphics.surface);
        }
        Ok(())
    }

    fn draw_scene(&mut self, size: LogicalSize<f64>, scale: f64) {
        self.scene.reset();
        let button_color = if self.create_track_button.hovered() {
            if self.create_track_button.pressed {
                Color::from_rgb8(42, 126, 80)
            } else {
                Color::from_rgb8(62, 164, 106)
            }
        } else {
            Color::from_rgb8(48, 54, 64)
        };
        self.scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            button_color,
            None,
            &CreateTrackButton::DIMENSIONS.to_rounded_rect(6.0),
        );
        // A plus sign is the initial create-track affordance.
        for bar in [
            Rect::new(46.0, 50.5, 66.0, 53.5),
            Rect::new(54.5, 42.0, 57.5, 62.0),
        ] {
            self.scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                Color::from_rgb8(236, 240, 244),
                None,
                &bar,
            );
        }
        for cell in self.track_symbols.cells(size.width, size.height) {
            draw_track_symbol(&mut self.scene, cell, scale);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.active = true;
        if self.window.is_none() {
            match event_loop.create_window(
                Window::default_attributes()
                    .with_title("Tinex")
                    .with_min_inner_size(LogicalSize::new(240.0, 200.0))
                    .with_inner_size(LogicalSize::new(800.0, 450.0)),
            ) {
                Ok(window) => self.window = Some(Arc::new(window)),
                Err(error) => {
                    self.fail(
                        event_loop,
                        anyhow::Error::new(error).context("Could not create window"),
                    );
                    return;
                }
            }
        }
        if self.graphics.is_none()
            && let Err(error) = self.initialize_graphics()
        {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.active = false;
        self.graphics = None;
        self.create_track_button = CreateTrackButton::default();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().map(|window| window.id()) != Some(window_id) {
            return;
        }
        let result = match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                Ok(())
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.resize(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::CursorMoved { position, .. } => {
                let window = self.window.as_ref().unwrap();
                let position = position.to_logical::<f64>(window.scale_factor());
                self.create_track_button.cursor = Some(Point::new(position.x, position.y));
                window.set_cursor(if self.create_track_button.hovered() {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
                window.request_redraw();
                Ok(())
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
                self.create_track_button = CreateTrackButton::default();
                let window = self.window.as_ref().unwrap();
                window.set_cursor(CursorIcon::Default);
                window.request_redraw();
                Ok(())
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if self.create_track_button.mouse_input(state)
                    && let Err(error) = self.handle.requests.send(TinexRequest::NewTrack(
                        Track::new(crate::tinex::plugin::EPiano::new(
                            self.handle.client.as_client().sample_rate() as f32,
                        )),
                    ))
                {
                    warn!(?error, "Could not request track creation");
                }
                self.window.as_ref().unwrap().request_redraw();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_update {
            self.track_symbols
                .update(self.handle.notifications.try_iter());
            if self.graphics.is_some()
                && let Some(window) = &self.window
            {
                window.request_redraw();
            }
            self.next_update = now + std::time::Duration::from_millis(16);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_update));
    }
}

#[derive(Default)]
struct CreateTrackButton {
    cursor: Option<Point>,
    pressed: bool,
}

impl CreateTrackButton {
    const DIMENSIONS: Rect = Rect::new(32.0, 32.0, 80.0, 72.0);

    fn hovered(&self) -> bool {
        self.cursor
            .is_some_and(|point| Self::DIMENSIONS.contains(point))
    }

    fn mouse_input(&mut self, state: ElementState) -> bool {
        match state {
            ElementState::Pressed => {
                self.pressed = self.hovered();
                false
            }
            ElementState::Released => {
                let clicked = self.pressed && self.hovered();
                self.pressed = false;
                clicked
            }
        }
    }
}

#[derive(Default)]
struct TrackSymbols {
    count: usize,
}

impl TrackSymbols {
    fn update(&mut self, notifications: impl IntoIterator<Item = TinexNotification>) {
        for notification in notifications {
            match notification {
                TinexNotification::TrackCreated(id) => {
                    self.count += 1;
                    info!(?id, "Track created");
                }
                TinexNotification::TrackDeleted(_) => {
                    self.count = self.count.saturating_sub(1);
                    info!("Track deleted");
                }
                TinexNotification::TrackCreationFailed(_) => warn!("Track creation failed"),
                TinexNotification::OutputLevel(_) => {}
            }
        }
    }

    fn cells(&self, width: f64, height: f64) -> impl Iterator<Item = Rect> {
        let area = Rect::new(
            32.0,
            CreateTrackButton::DIMENSIONS.y1 + 16.0,
            width - 32.0,
            height - 32.0,
        );
        let count = if area.width() <= 0.0 || area.height() <= 0.0 {
            0
        } else {
            self.count
        };
        // Choose the columns that give every track the largest square cell.
        let (columns, cell_size) = (1..=count)
            .map(|columns| {
                let rows = count.div_ceil(columns);
                let size = (area.width() / columns as f64)
                    .min(area.height() / rows as f64)
                    .min(48.0);
                (columns, size)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .unwrap_or((1, 0.0));
        (0..count).map(move |index| {
            let x = area.x0 + (index % columns) as f64 * cell_size;
            let y = area.y0 + (index / columns) as f64 * cell_size;
            Rect::new(x, y, x + cell_size, y + cell_size)
        })
    }
}

fn draw_track_symbol(scene: &mut Scene, cell: Rect, scale: f64) {
    let icon_scale = cell.width() / 32.0;
    let padding = (cell.width() - 24.0 * icon_scale) * 0.5;
    let transform = Affine::scale(scale)
        * Affine::translate((cell.x0 + padding, cell.y0 + padding))
        * Affine::scale(icon_scale);
    let color = Color::from_rgb8(236, 240, 244);
    let mut flag = BezPath::new();
    flag.move_to((16.0, 2.0));
    flag.curve_to((16.0, 6.0), (24.0, 6.0), (22.0, 13.0));
    flag.line_to((20.0, 13.0));
    flag.curve_to((21.0, 9.0), (16.0, 9.0), (14.0, 7.0));
    flag.close_path();
    scene.fill(Fill::NonZero, transform, color, None, &flag);
    scene.fill(
        Fill::NonZero,
        transform,
        color,
        None,
        &Rect::new(13.0, 2.0, 16.0, 19.0),
    );
    scene.fill(
        Fill::NonZero,
        transform,
        color,
        None,
        &Ellipse::new((9.0, 19.0), (7.0, 4.0), -0.3),
    );
}

struct Graphics {
    surface: RenderSurface<'static>,
    renderer: Renderer,
}

#[cfg(test)]
mod gui_tests {
    use super::*;

    #[test]
    fn create_track_requires_a_complete_click_inside_button() {
        let mut button = CreateTrackButton {
            cursor: Some(Point::new(56.0, 52.0)),
            ..Default::default()
        };
        assert!(!button.mouse_input(ElementState::Released));
        assert!(!button.mouse_input(ElementState::Pressed));
        assert!(button.mouse_input(ElementState::Released));
        assert!(!button.mouse_input(ElementState::Released));

        button.mouse_input(ElementState::Pressed);
        button.cursor = Some(Point::new(100.0, 100.0));
        assert!(!button.mouse_input(ElementState::Released));
        button.mouse_input(ElementState::Pressed);
        button.cursor = Some(Point::new(56.0, 52.0));
        assert!(!button.mouse_input(ElementState::Released));
    }

    #[test]
    fn symbols_follow_confirmed_track_lifecycle() {
        let mut symbols = TrackSymbols::default();
        assert_eq!(symbols.count, 0);
        symbols.update([
            TinexNotification::TrackCreated(crate::tinex::id::Id::new()),
            TinexNotification::OutputLevel(0.7),
            TinexNotification::TrackCreated(crate::tinex::id::Id::new()),
            TinexNotification::TrackCreationFailed(Track::new(crate::tinex::plugin::Silence)),
        ]);
        assert_eq!(symbols.count, 2);
        symbols.update([
            TinexNotification::TrackDeleted(Track::new(crate::tinex::plugin::Silence)),
            TinexNotification::TrackCreated(crate::tinex::id::Id::new()),
            TinexNotification::TrackDeleted(Track::new(crate::tinex::plugin::Silence)),
        ]);
        assert_eq!(symbols.count, 1);
        symbols.update([TinexNotification::OutputLevel(1.0)]);
        assert_eq!(symbols.count, 1);
        symbols.update([TinexNotification::TrackDeleted(Track::new(
            crate::tinex::plugin::Silence,
        ))]);
        assert_eq!(symbols.count, 0);
    }

    #[test]
    fn grid_fits_all_tracks_without_overlap() {
        for (width, height) in [(240.0, 200.0), (800.0, 450.0), (450.0, 800.0)] {
            for count in [0, 1, 2, 10, 64] {
                let cells: Vec<_> = TrackSymbols { count }.cells(width, height).collect();
                assert_eq!(cells.len(), count);
                for (index, cell) in cells.iter().enumerate() {
                    assert!(cell.x0 >= 32.0);
                    assert!(cell.y0 >= CreateTrackButton::DIMENSIONS.y1 + 16.0);
                    assert!(cell.x1 <= width - 32.0 + 1e-9);
                    assert!(cell.y1 <= height - 32.0 + 1e-9);
                    assert!(cell.width() > 0.0 && cell.width() <= 48.0);
                    for other in &cells[..index] {
                        assert!(cell.intersect(*other).area() <= 1e-9);
                    }
                }
            }
        }
    }

    #[test]
    fn grid_is_empty_when_no_space_is_available() {
        let symbols = TrackSymbols { count: 10 };
        for (width, height) in [(64.0, 200.0), (240.0, 120.0), (0.0, 0.0)] {
            assert!(symbols.cells(width, height).next().is_none());
        }
    }
}
