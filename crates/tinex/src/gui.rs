mod action;
mod cpu_polling;
mod dispatch;
mod graphics;
mod meter_polling;
mod state;
mod theme;
mod update;
mod view;

use anyhow::{Context, Result};
use cpu_polling::CpuPolling;
use dispatch::{dispatch_action, poll_notifications};
use graphics::Graphics;
use meter_polling::MeterPolling;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::Theme;
use tinex_widgets::{ActivationKey, Input, TextRenderer, Ui};
use vello::Scene;
use vello::kurbo::{Point, Size};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::jack::TinexHandle;
use action::{Action, UiAction};
use state::AppState;

const NOTIFICATION_POLL_INTERVAL: Duration = Duration::from_millis(16);

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
    graphics: Graphics,
    window: Option<Arc<Window>>,
    scene: Scene,
    theme: Theme,
    state: AppState,
    meter_polling: MeterPolling,
    cpu_polling: CpuPolling,
    ui: Ui<UiAction>,
    text: TextRenderer,
    cursor: Option<PhysicalPosition<f64>>,
    shift: bool,
    redraw_pending: bool,
    redraw_retry_at: Option<Instant>,
    occluded: bool,
    error: Option<anyhow::Error>,
    active: bool,
}

impl App {
    fn new(handle: TinexHandle) -> Self {
        let state = AppState {
            sample_rate: handle.client.as_client().sample_rate() as f32,
            ..AppState::default()
        };
        let ui = Ui::new(view::view(&state), Size::new(800.0, 450.0));
        Self {
            handle,
            graphics: Graphics::new(),
            window: None,
            scene: Scene::new(),
            theme: Theme::default(),
            state,
            meter_polling: MeterPolling::default(),
            cpu_polling: CpuPolling::default(),
            ui,
            text: TextRenderer::new(),
            cursor: None,
            shift: false,
            redraw_pending: true,
            redraw_retry_at: None,
            occluded: false,
            error: None,
            active: false,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn rebuild_view(&mut self) {
        if let Some(window) = &self.window {
            let size = window.inner_size().to_logical::<f64>(window.scale_factor());
            self.ui
                .set_view(view::view(&self.state), Size::new(size.width, size.height));
            if let Some(cursor) = self.cursor {
                self.ui.input(Input::PointerMoved(logical_point(
                    cursor,
                    window.scale_factor(),
                )));
            }
        }
        self.redraw_pending = true;
        self.redraw_retry_at = None;
    }

    fn dispatch(&mut self, action: Action) {
        self.state.sample_rate = self.handle.client.as_client().sample_rate() as f32;
        if dispatch_action(
            &mut self.state,
            action,
            &self.handle.requests,
            &self.handle.notifications,
        ) {
            self.rebuild_view();
        }
    }

    fn input(&mut self, input: Input) {
        // Finish traversing the widgets before dispatch changes application state.
        let actions = self.ui.input(input);
        for action in actions {
            self.dispatch(Action::Ui(action));
        }
        self.redraw_pending = true;
        self.redraw_retry_at = None;
    }

    fn cancel_input(&mut self) {
        self.cursor = None;
        self.shift = false;
        self.input(Input::Cancel);
    }

    fn initialize_graphics(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let Some(window) = &self.window else {
            return Ok(());
        };
        self.graphics.initialize(window)
    }

    fn resize(&mut self) -> Result<()> {
        self.rebuild_view();
        if !self.active {
            return Ok(());
        }
        if !self.graphics.is_ready() {
            return self.initialize_graphics();
        }
        let Some(window) = &self.window else {
            return Ok(());
        };
        self.graphics.resize(window)
    }

    fn draw(&mut self) -> Result<()> {
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 || self.occluded {
            return Ok(());
        }
        // Keep the dirty flag until a frame actually presents, but throttle
        // failed acquisitions so an occluded surface cannot create a busy loop.
        self.redraw_retry_at = Some(Instant::now() + NOTIFICATION_POLL_INTERVAL);
        let Some(frame) = self.graphics.begin_frame(&window)? else {
            return Ok(());
        };
        self.scene.reset();
        self.ui.render(
            &mut self.scene,
            &mut self.text,
            &self.theme.widgets,
            window.scale_factor(),
        );
        self.graphics
            .present(frame, &self.scene, self.theme.background, &window)?;
        self.redraw_pending = false;
        self.redraw_retry_at = None;
        Ok(())
    }
}

fn logical_point(position: PhysicalPosition<f64>, scale: f64) -> Point {
    Point::new(position.x / scale, position.y / scale)
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.active = true;
        self.occluded = false;
        if self.window.is_none() {
            match event_loop.create_window(
                Window::default_attributes()
                    .with_title("Tinex")
                    .with_min_inner_size(LogicalSize::new(400.0, 280.0))
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
        self.rebuild_view();
        if !self.graphics.is_ready()
            && let Err(error) = self.initialize_graphics()
        {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.active = false;
        self.cancel_input();
        self.graphics.suspend();
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
            WindowEvent::RedrawRequested if self.active => {
                // OS-requested redraws also need retry semantics.
                self.redraw_pending = true;
                self.draw()
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.redraw_pending = true;
                self.redraw_retry_at = None;
                Ok(())
            }
            WindowEvent::Focused(false) => {
                self.cancel_input();
                Ok(())
            }
            WindowEvent::CursorMoved { position, .. } if self.active => {
                self.cursor = Some(position);
                let scale = self.window.as_ref().unwrap().scale_factor();
                self.input(Input::PointerMoved(logical_point(position, scale)));
                Ok(())
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.input(Input::PointerLeft);
                Ok(())
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } if self.active => {
                self.input(if state == ElementState::Pressed {
                    Input::PointerPressed
                } else {
                    Input::PointerReleased
                });
                Ok(())
            }
            WindowEvent::MouseWheel { delta, .. } if self.active => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -f64::from(y) * 40.0,
                    MouseScrollDelta::PixelDelta(position) => {
                        -position.y / self.window.as_ref().unwrap().scale_factor()
                    }
                };
                self.input(Input::Scroll(delta));
                Ok(())
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift = modifiers.state().shift_key();
                Ok(())
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if self.active => {
                match event.logical_key {
                    Key::Named(NamedKey::Tab)
                        if event.state == ElementState::Pressed && !event.repeat =>
                    {
                        self.input(Input::Tab {
                            backwards: self.shift,
                        })
                    }
                    Key::Named(NamedKey::Enter) => self.input(Input::Key {
                        key: ActivationKey::Enter,
                        pressed: event.state == ElementState::Pressed,
                        repeat: event.repeat,
                    }),
                    Key::Named(NamedKey::Space) => self.input(Input::Key {
                        key: ActivationKey::Space,
                        pressed: event.state == ElementState::Pressed,
                        repeat: event.repeat,
                    }),
                    _ => {}
                }
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let mut changed = poll_notifications(
            &mut self.state,
            &self.handle.notifications,
            &self.handle.requests,
        );
        let visible = self.active
            && !self.occluded
            && self.window.as_ref().is_some_and(|window| {
                window.inner_size().width > 0 && window.inner_size().height > 0
            });
        let now = Instant::now();
        if let Some(action) = self.meter_polling.action(now, &self.state, visible) {
            changed |= dispatch_action(
                &mut self.state,
                action,
                &self.handle.requests,
                &self.handle.notifications,
            );
        }
        if self.cpu_polling.due(now, visible && self.state.connected) {
            let load = self.handle.client.as_client().cpu_load();
            changed |= dispatch_action(
                &mut self.state,
                Action::CpuLoad(load),
                &self.handle.requests,
                &self.handle.notifications,
            );
        }
        if changed {
            self.rebuild_view();
        }
        if self.redraw_pending
            && self
                .redraw_retry_at
                .is_none_or(|deadline| Instant::now() >= deadline)
            && self.active
            && !self.occluded
            && self.graphics.is_ready()
            && let Some(window) = &self.window
            && window.inner_size().width > 0
            && window.inner_size().height > 0
        {
            window.request_redraw();
        }
        // Notification polling remains active during suspension. Waiting on an
        // overdue sample while a response is in flight would create a busy loop.
        let now = Instant::now();
        let mut deadline = now + NOTIFICATION_POLL_INTERVAL;
        if let Some(sample) = self.meter_polling.deadline(now, &self.state, visible) {
            deadline = deadline.min(sample);
        }
        if let Some(sample) = self
            .cpu_polling
            .deadline(now, visible && self.state.connected)
        {
            deadline = deadline.min(sample);
        }
        if visible
            && self.redraw_pending
            && let Some(retry) = self.redraw_retry_at
            && retry > now
        {
            deadline = deadline.min(retry);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
    }
}

#[cfg(test)]
mod tests;
