mod action;
mod cpu_polling;
mod dispatch;
mod meter_polling;
mod state;
mod theme;
mod update;
mod view;

use anyhow::{Context, Result};
use cpu_polling::CpuPolling;
use dispatch::{dispatch_action, poll_notifications};
use masonry_winit::app::{AppDriver, EventLoop, MasonryState, MasonryUserEvent};
use meter_polling::MeterPolling;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use xilem::core::{MessageResult, fork, map_message, map_state};
use xilem::view::task;
use xilem::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, StartCause, WindowEvent},
    event_loop::ActiveEventLoop,
    window::WindowId,
};
use xilem::{WidgetView, WindowOptions, Xilem};

use crate::jack::TinexHandle;
use action::Action;
#[cfg(test)]
use action::UiAction;
use state::AppState;

const NOTIFICATION_POLL_INTERVAL: Duration = Duration::from_millis(16);

pub fn run(handle: TinexHandle) -> Result<()> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("Could not create GUI event loop")?;
    let visibility = Rc::new(Cell::new(Visibility::default()));
    let app = App {
        state: AppState {
            sample_rate: handle.client.as_client().sample_rate() as f32,
            ..AppState::default()
        },
        handle,
        meter_polling: MeterPolling::default(),
        cpu_polling: CpuPolling::default(),
        visibility: visibility.clone(),
    };
    let proxy = event_loop.create_proxy();
    let (driver, windows) = Xilem::new_simple(
        app,
        app_view,
        WindowOptions::new("Tinex")
            .with_initial_inner_size(LogicalSize::new(800.0, 450.0))
            .with_min_inner_size(LogicalSize::new(400.0, 280.0)),
    )
    .with_font(xilem::Blob::new(std::sync::Arc::new(
        tinex_widgets::FONT_SANS.to_vec(),
    )))
    .with_font(xilem::Blob::new(std::sync::Arc::new(
        tinex_widgets::FONT_EMOJI.to_vec(),
    )))
    .into_driver_and_windows(move |event| proxy.send_event(event).map_err(|err| err.0));
    let mut runner = Lifecycle {
        masonry: MasonryState::new(event_loop.create_proxy(), windows, theme::properties()),
        driver: Box::new(driver),
        visibility,
    };
    event_loop
        .run_app(&mut runner)
        .context("GUI event loop failed")
}

struct App {
    state: AppState,
    handle: TinexHandle,
    meter_polling: MeterPolling,
    cpu_polling: CpuPolling,
    visibility: Rc<Cell<Visibility>>,
}

impl App {
    fn dispatch(&mut self, action: Action) -> bool {
        self.state.sample_rate = self.handle.client.as_client().sample_rate() as f32;
        dispatch_action(
            &mut self.state,
            action,
            &self.handle.requests,
            &self.handle.notifications,
        )
    }

    fn poll(&mut self) -> (bool, Duration) {
        let mut changed = poll_notifications(
            &mut self.state,
            &self.handle.notifications,
            &self.handle.requests,
        );
        let visible = self.visibility.get().visible();
        let now = Instant::now();
        if let Some(action) = self.meter_polling.action(now, &self.state, visible) {
            changed |= self.dispatch(action);
        }
        if self.cpu_polling.due(now, visible && self.state.connected) {
            changed |= self.dispatch(Action::CpuLoad(self.handle.client.as_client().cpu_load()));
        }
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
        (changed, deadline.saturating_duration_since(now))
    }
}

fn changed_result(changed: bool) -> MessageResult<()> {
    if changed {
        MessageResult::Action(())
    } else {
        MessageResult::Nop
    }
}

fn app_view(app: &mut App) -> impl WidgetView<App> + use<> {
    let pages = map_message(
        map_state(view::view(&app.state), |app: &mut App| &mut app.state),
        |app: &mut App, result| match result {
            MessageResult::Action(action) => changed_result(app.dispatch(Action::Ui(action))),
            other => other.map(|_| ()),
        },
    );
    fork(pages, polling(App::poll))
}

/// A lifecycle-bound wakeup source. Unchanged ticks do not rebuild or redraw.
fn polling<State: 'static, F>(
    on_tick: F,
) -> impl xilem::core::View<State, (), xilem::ViewCtx, Element = xilem::core::NoElement> + Send + Sync
where
    F: Fn(&mut State) -> (bool, Duration) + Send + Sync + 'static,
{
    map_message(
        task(
            |proxy| async move {
                // Only wake the GUI thread. Never touch the JACK handle, domain
                // state, or engine-owned notifications on a Tokio worker. Awaiting
                // acknowledgement bounds the queue to one tick, even during stalls.
                let mut delay = NOTIFICATION_POLL_INTERVAL;
                loop {
                    xilem::tokio::time::sleep(delay).await;
                    let (ack, received) = xilem::tokio::sync::oneshot::channel();
                    if proxy.message(ack).is_err() {
                        break;
                    }
                    let Ok(next_delay) = received.await else {
                        break;
                    };
                    delay = next_delay;
                }
            },
            move |state: &mut State, ack: xilem::tokio::sync::oneshot::Sender<Duration>| {
                let (changed, delay) = on_tick(state);
                let _ = ack.send(delay);
                changed
            },
        ),
        |_: &mut State, result| match result {
            MessageResult::Action(changed) => changed_result(changed),
            other => other.map(|_| ()),
        },
    )
}

#[derive(Clone, Copy, Default)]
struct Visibility {
    active: bool,
    occluded: bool,
    zero_size: bool,
}
impl Visibility {
    fn visible(self) -> bool {
        self.active && !self.occluded && !self.zero_size
    }
}

/// Xilem 0.4 has no window-visibility callback. This wrapper observes lifecycle
/// only; Masonry handles every event, surface, input, scaling, and redraw.
struct Lifecycle {
    masonry: MasonryState<'static>,
    driver: Box<dyn AppDriver>,
    visibility: Rc<Cell<Visibility>>,
}
impl Lifecycle {
    fn set_visibility(&self, f: impl FnOnce(&mut Visibility)) {
        let mut visibility = self.visibility.get();
        f(&mut visibility);
        self.visibility.set(visibility);
    }
}
impl ApplicationHandler<MasonryUserEvent> for Lifecycle {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.set_visibility(|v| {
            v.active = true;
            v.occluded = false;
        });
        self.masonry.handle_resumed(event_loop, &mut *self.driver);
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.set_visibility(|v| v.active = false);
        self.masonry.handle_suspended(event_loop);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::Occluded(occluded) => self.set_visibility(|v| v.occluded = *occluded),
            WindowEvent::Resized(size) => {
                self.set_visibility(|v| v.zero_size = size.width == 0 || size.height == 0)
            }
            _ => {}
        }
        self.masonry
            .handle_window_event(event_loop, id, event, &mut *self.driver);
    }
    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: xilem::winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        self.masonry
            .handle_device_event(event_loop, id, event, &mut *self.driver);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MasonryUserEvent) {
        self.masonry
            .handle_user_event(event_loop, event, &mut *self.driver);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry.handle_about_to_wait(event_loop);
    }
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.masonry.handle_new_events(event_loop, cause);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry.handle_exiting(event_loop);
    }
    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry.handle_memory_warning(event_loop);
    }
}

#[cfg(test)]
mod tests;
