mod graphics;
mod plugins;
mod sidebar;
mod tracks;

use graphics::Graphics;
use plugins::{PLUGIN_ROW_STRIDE, PluginMenuAction, UiPlugins};
use sidebar::{Control, Page, Sidebar};
use tinex_widgets::{LevelMeter, TextRenderer, Theme};
use tracks::{CreateTrackButton, TRACK_ROW_STRIDE, UiTracks};

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tracing::warn;
use vello::Scene;
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::{BlendMode, Fill};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, Window, WindowId};

use crate::jack::TinexHandle;
use tinex_core::id::Id;
use tinex_core::track::Track;
use tinex_core::{TinexNotification, TinexRequest};

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
    tracks: UiTracks,
    plugins: UiPlugins,
    meters: UiMeters,
    create_track_button: CreateTrackButton,
    page: Page,
    sidebar: Sidebar,
    text: TextRenderer,
    theme: Theme,
    cpu_load: Option<f32>,
    next_cpu_update: Instant,
    error: Option<anyhow::Error>,
    active: bool,
}

impl App {
    fn new(handle: TinexHandle) -> Self {
        Self {
            handle,
            graphics: Graphics::new(),
            window: None,
            scene: Scene::new(),
            tracks: UiTracks::default(),
            plugins: UiPlugins::default(),
            meters: UiMeters::default(),
            create_track_button: CreateTrackButton::default(),
            page: Page::Tracks,
            sidebar: Sidebar::default(),
            text: TextRenderer::new(),
            theme: Theme::default(),
            cpu_load: None,
            next_cpu_update: Instant::now(),
            error: None,
            active: false,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        event_loop.exit();
    }

    fn activate_plugin_menu(&mut self, action: PluginMenuAction) {
        let sample_rate = self.handle.client.as_client().sample_rate() as f32;
        let result = match action {
            PluginMenuAction::NewTrack(plugin) => {
                let mut track = Track::new();
                track.push_boxed_plugin(plugin.build(sample_rate));
                let id = track.id();
                self.tracks.reserve_plugin_name(id, plugin.name());
                let result = self.handle.requests.send(TinexRequest::NewTrack(track));
                if result.is_err() {
                    self.tracks.cancel_pending_name(id);
                }
                result
            }
            PluginMenuAction::AddToTrack(plugin, track_id) => {
                self.handle.requests.send(TinexRequest::AddPlugin {
                    track_id,
                    plugin: plugin.build(sample_rate),
                })
            }
        };
        if let Err(error) = result {
            warn!(?error, "Could not request plugin assignment");
        }
        self.plugins.close_menu();
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
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let Some(frame) = self.graphics.begin_frame(&window)? else {
            return Ok(());
        };
        let scale = window.scale_factor();
        self.draw_scene(size.to_logical(scale), scale);
        self.graphics
            .present(frame, &self.scene, self.theme.background, &window)
    }

    fn draw_scene(&mut self, size: LogicalSize<f64>, scale: f64) {
        self.scene.reset();
        self.draw_bottom_bar(size, scale);
        let size = content_size(size);
        self.scene.push_layer(
            Fill::NonZero,
            BlendMode::default(),
            1.0,
            Affine::scale(scale),
            &Rect::new(0.0, 0.0, size.width, size.height),
        );
        self.draw_content(size, scale);
        if self.page == Page::Tracks {
            self.tracks.draw_tooltips(
                &self.create_track_button,
                &mut self.scene,
                &mut self.text,
                &self.theme,
                size,
                self.sidebar.width(),
                scale,
            );
        }
        self.sidebar
            .draw_tooltip(&mut self.scene, &mut self.text, &self.theme, size, scale);
        self.scene.pop_layer();
    }

    fn draw_content(&mut self, size: LogicalSize<f64>, scale: f64) {
        self.sidebar.draw(
            &mut self.scene,
            &mut self.text,
            &self.theme,
            self.page,
            size,
            scale,
        );
        match self.page {
            Page::Tracks => {}
            Page::Plugins => {
                self.plugins.draw(
                    &mut self.scene,
                    &mut self.text,
                    &self.theme,
                    size,
                    self.sidebar.width(),
                    scale,
                    &self.tracks.tracks,
                );
                return;
            }
            Page::Settings => {
                self.text.draw(
                    &mut self.scene,
                    "Settings",
                    Point::new(self.sidebar.width() + 32.0, 32.0),
                    scale,
                    self.theme.foreground,
                );
                return;
            }
        }
        self.tracks.draw(
            &self.create_track_button,
            &mut self.scene,
            &mut self.text,
            &self.theme,
            size,
            self.sidebar.width(),
            scale,
        );
    }

    fn refresh_content_cursor(&mut self) {
        let cursor = (self.page == Page::Tracks)
            .then(|| self.sidebar.cursor())
            .flatten();
        self.create_track_button.cursor = cursor;
        self.tracks.cursor = cursor;
        if let Some(window) = &self.window {
            let size = content_size(window.inner_size().to_logical(window.scale_factor()));
            window.set_cursor(
                if self.sidebar.hovered().is_some()
                    || self.create_track_button.hovered(self.sidebar.width())
                    || self.tracks.hovered(size, self.sidebar.width()).is_some()
                {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                },
            );
        }
    }

    fn draw_bottom_bar(&mut self, size: LogicalSize<f64>, scale: f64) {
        let top = content_size(size).height;
        let bar = Rect::new(0.0, top, size.width, size.height);
        self.scene.push_layer(
            Fill::NonZero,
            BlendMode::default(),
            1.0,
            Affine::scale(scale),
            &bar,
        );
        self.scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            self.theme.surface,
            None,
            &bar,
        );
        let load = self.cpu_load.map_or_else(
            || "🖥️ CPU --".to_string(),
            |load| format!("🖥️ CPU {load:.1}%"),
        );
        self.text.draw_right(
            &mut self.scene,
            &load,
            Point::new((size.width - 16.0).max(0.0), top + 8.0),
            scale,
            self.theme.foreground,
        );
        if let Some(meter) = bottom_meter_rect(size) {
            self.text.draw(
                &mut self.scene,
                "Output",
                Point::new(16.0, top + 8.0),
                scale,
                self.theme.foreground,
            );
            LevelMeter(self.meters.output_level).draw(&mut self.scene, &self.theme, meter, scale);
        }
        self.scene.pop_layer();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.active = true;
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
        if !self.graphics.is_ready()
            && let Err(error) = self.initialize_graphics()
        {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.active = false;
        self.graphics.suspend();
        self.create_track_button = CreateTrackButton::default();
        self.sidebar.reset_input();
        self.tracks.reset_input();
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
                if position.y
                    < content_size(window.inner_size().to_logical(window.scale_factor())).height
                {
                    self.sidebar.set_cursor(Point::new(position.x, position.y));
                } else {
                    self.sidebar.reset_input();
                }
                self.refresh_content_cursor();
                self.window.as_ref().unwrap().request_redraw();
                Ok(())
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
                self.sidebar.reset_input();
                self.tracks.reset_input();
                self.plugins.close_menu();
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
                if self.page == Page::Plugins && self.plugins.menu_is_open() {
                    if state == ElementState::Released {
                        let window = self.window.as_ref().unwrap();
                        let size =
                            content_size(window.inner_size().to_logical(window.scale_factor()));
                        if let Some(action) = self.plugins.menu_action(
                            self.sidebar.cursor(),
                            size,
                            &self.tracks.tracks,
                        ) {
                            self.activate_plugin_menu(action);
                        } else if !self.plugins.menu_contains(
                            self.sidebar.cursor(),
                            size,
                            &self.tracks.tracks,
                        ) {
                            self.plugins.close_menu();
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                if self.page == Page::Plugins && state == ElementState::Released {
                    let window = self.window.as_ref().unwrap();
                    let size = content_size(window.inner_size().to_logical(window.scale_factor()));
                    if let Some(plugin) = self.plugins.add_button_plugin(
                        self.sidebar.cursor(),
                        size,
                        self.sidebar.width(),
                    ) {
                        self.plugins
                            .open_plugin_menu(self.sidebar.cursor().unwrap(), plugin, size);
                        window.request_redraw();
                        return;
                    }
                }
                if let Some(control) = self.sidebar.mouse_input(state) {
                    if let Control::Page(page) = control {
                        self.page = page;
                    }
                    self.tracks.reset_input();
                    self.create_track_button = CreateTrackButton::default();
                    self.refresh_content_cursor();
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                if self.page == Page::Tracks
                    && self
                        .create_track_button
                        .mouse_input(state, self.sidebar.width())
                {
                    let track = Track::with_plugin(tinex_plugins::EPiano::new(
                        self.handle.client.as_client().sample_rate() as f32,
                    ));
                    if let Err(error) = self.handle.requests.send(TinexRequest::NewTrack(track)) {
                        warn!(?error, "Could not request track creation");
                    }
                }
                if self.page == Page::Tracks {
                    let window = self.window.as_ref().unwrap();
                    let size = content_size(window.inner_size().to_logical(window.scale_factor()));
                    if let Some(id) = self.tracks.mouse_input(state, size, self.sidebar.width())
                        && let Err(error) = self.handle.requests.send(TinexRequest::DeleteTrack(id))
                    {
                        warn!(?error, "Could not request track deletion");
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
                Ok(())
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } if self.page == Page::Plugins => {
                let window = self.window.as_ref().unwrap();
                let size = content_size(window.inner_size().to_logical(window.scale_factor()));
                self.plugins
                    .open_menu(self.sidebar.cursor(), size, self.sidebar.width());
                window.request_redraw();
                Ok(())
            }
            WindowEvent::MouseWheel { delta, .. }
                if matches!(self.page, Page::Tracks | Page::Plugins) =>
            {
                let window = self.window.as_ref().unwrap();
                let stride = if self.page == Page::Plugins {
                    PLUGIN_ROW_STRIDE
                } else {
                    TRACK_ROW_STRIDE
                };
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -f64::from(y),
                    MouseScrollDelta::PixelDelta(position) => {
                        -position.y / window.scale_factor() / stride
                    }
                };
                let size = content_size(window.inner_size().to_logical(window.scale_factor()));
                if self.page == Page::Plugins
                    && self.plugins.menu_is_open()
                    && self.plugins.scroll_menu(rows, size, &self.tracks.tracks)
                {
                    window.request_redraw();
                    return;
                }
                match self.page {
                    Page::Tracks => self.tracks.scroll(rows, size, self.sidebar.width()),
                    Page::Plugins => self.plugins.scroll(rows, size, self.sidebar.width()),
                    Page::Settings => unreachable!(),
                }
                window.request_redraw();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        for notification in self.handle.notifications.try_iter() {
            match notification {
                TinexNotification::OutputLevel {
                    output_level,
                    tracks,
                } => {
                    self.tracks.update_levels(&tracks);
                    self.meters.on_levels(output_level, tracks);
                }
                notification => self.tracks.on_notification(notification),
            }
        }
        if !self.active {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        let now = Instant::now();
        if cpu_update_due(now, &mut self.next_cpu_update) {
            self.cpu_load = Some(self.handle.client.as_client().cpu_load());
        }
        self.meters
            .request_levels(now, &self.handle.requests, &self.tracks);
        // Keep a redraw queued; presentation and the window system pace frames.
        if self.graphics.is_ready()
            && let Some(window) = &self.window
        {
            let size = window.inner_size();
            if size.width != 0 && size.height != 0 {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_update(
            self.next_cpu_update,
            self.meters.next_level_update,
        )));
    }
}

const BOTTOM_BAR_HEIGHT: f64 = 40.0;
const CPU_UPDATE_INTERVAL: Duration = Duration::from_secs(2);

fn content_size(size: LogicalSize<f64>) -> LogicalSize<f64> {
    LogicalSize::new(size.width, (size.height - BOTTOM_BAR_HEIGHT).max(0.0))
}

fn bottom_meter_rect(size: LogicalSize<f64>) -> Option<Rect> {
    let top = content_size(size).height;
    let right = size.width - 192.0;
    (right >= 104.0).then(|| Rect::new(80.0, top + 16.0, right, top + 24.0))
}

fn cpu_update_due(now: Instant, next: &mut Instant) -> bool {
    if now < *next {
        return false;
    }
    *next = now + CPU_UPDATE_INTERVAL;
    true
}

fn next_update(cpu: Instant, levels: Option<Instant>) -> Instant {
    levels.map_or(cpu, |levels| cpu.min(levels))
}

const LEVEL_UPDATE_INTERVAL: Duration = Duration::from_millis(16);
#[derive(Default)]
struct UiMeters {
    output_level: f32,
    level_buffer: HashMap<Id<Track>, f32>,
    level_pending: bool,
    next_level_update: Option<Instant>,
}

impl UiMeters {
    fn request_levels(&mut self, now: Instant, requests: &Sender<TinexRequest>, tracks: &UiTracks) {
        if self.next_level_update.is_some_and(|next| now < next) {
            return;
        }
        // Wake periodically even while awaiting a reply: the notification channel
        // does not wake the window event loop itself.
        self.next_level_update = Some(now + LEVEL_UPDATE_INTERVAL);
        if self.level_pending {
            return;
        }
        self.level_buffer
            .retain(|id, _| tracks.tracks.iter().any(|track| track.id == *id));
        for track in &tracks.tracks {
            self.level_buffer.entry(track.id).or_insert(0.0);
        }
        match requests.send(TinexRequest::OutputLevel(std::mem::take(
            &mut self.level_buffer,
        ))) {
            Ok(()) => self.level_pending = true,
            Err(error) => {
                warn!("Could not request output levels");
                if let TinexRequest::OutputLevel(tracks) = error.0 {
                    self.level_buffer = tracks;
                }
            }
        }
    }

    fn on_levels(&mut self, output_level: f32, tracks: HashMap<Id<Track>, f32>) {
        self.output_level = output_level;
        self.level_buffer = tracks;
        self.level_pending = false;
    }
}

#[cfg(test)]
mod gui_tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn level_polling_is_paced_and_waits_for_a_response() {
        let (sender, receiver) = mpsc::channel();
        let mut tracks = UiTracks::default();
        let mut meters = UiMeters::default();
        let first = Track::with_plugin(tinex_core::plugin::Silence);
        let second_id = Id::new();
        tracks.on_notification(TinexNotification::TrackCreated(first.id()));
        tracks.on_notification(TinexNotification::TrackCreated(second_id));
        let now = Instant::now();
        meters.request_levels(now, &sender, &tracks);
        let TinexRequest::OutputLevel(mut levels) = receiver.try_recv().unwrap() else {
            panic!("expected level request");
        };
        assert_eq!(levels.len(), 2);
        assert!(levels.contains_key(&first.id()));
        assert!(levels.contains_key(&second_id));
        let capacity = levels.capacity();

        meters.request_levels(now + LEVEL_UPDATE_INTERVAL, &sender, &tracks);
        assert!(receiver.try_recv().is_err());
        levels.insert(second_id, 0.5);
        let notification = TinexNotification::OutputLevel {
            output_level: 0.5,
            tracks: levels,
        };
        let TinexNotification::OutputLevel {
            output_level,
            tracks: levels,
        } = notification
        else {
            panic!("expected levels");
        };
        tracks.update_levels(&levels);
        meters.on_levels(output_level, levels);
        assert_eq!(meters.output_level, 0.5);
        assert_eq!(tracks.tracks[1].output_level, 0.5);
        tracks.on_notification(TinexNotification::TrackDeleted(first));
        let third_id = Id::new();
        tracks.on_notification(TinexNotification::TrackCreated(third_id));

        meters.request_levels(now + LEVEL_UPDATE_INTERVAL, &sender, &tracks);
        assert!(receiver.try_recv().is_err());
        meters.request_levels(now + LEVEL_UPDATE_INTERVAL * 2, &sender, &tracks);
        let TinexRequest::OutputLevel(levels) = receiver.try_recv().unwrap() else {
            panic!("expected level request");
        };
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[&second_id], 0.5);
        assert_eq!(levels[&third_id], 0.0);
        assert_eq!(levels.capacity(), capacity);
    }

    #[test]
    fn level_polling_handles_empty_tracks_and_disconnected_engine() {
        let (sender, receiver) = mpsc::channel();
        let mut tracks = UiTracks::default();
        let mut meters = UiMeters::default();
        let now = Instant::now();
        meters.request_levels(now, &sender, &tracks);
        let TinexRequest::OutputLevel(levels) = receiver.try_recv().unwrap() else {
            panic!("expected overall output request with no tracks");
        };
        assert!(levels.is_empty());
        meters.on_levels(0.0, levels);
        tracks.on_notification(TinexNotification::TrackCreated(Id::new()));
        drop(receiver);
        meters.request_levels(now + LEVEL_UPDATE_INTERVAL, &sender, &tracks);
        assert!(!meters.level_pending);
        assert_eq!(meters.level_buffer.len(), 1);
    }

    #[test]
    fn cpu_updates_every_two_seconds_and_wakes_with_levels() {
        let now = Instant::now();
        let mut next = now;
        assert!(cpu_update_due(now, &mut next));
        assert_eq!(next, now + Duration::from_secs(2));
        assert!(!cpu_update_due(now + Duration::from_secs(1), &mut next));
        assert!(cpu_update_due(now + Duration::from_secs(2), &mut next));
        assert_eq!(
            next_update(next, Some(now + LEVEL_UPDATE_INTERVAL)),
            now + LEVEL_UPDATE_INTERVAL
        );
        assert_eq!(next_update(now, Some(next)), now);
        assert_eq!(next_update(next, None), next);
    }

    #[test]
    fn bottom_bar_reserves_content_and_hides_meter_when_narrow() {
        for size in [
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 450.0),
        ] {
            let content = content_size(size);
            assert_eq!(content.height, size.height - BOTTOM_BAR_HEIGHT);
            let meter = bottom_meter_rect(size).unwrap();
            assert!(meter.y0 >= content.height);
            assert!(meter.y1 <= size.height);
            assert!(meter.x1 <= size.width - 192.0);
            let mut tracks = UiTracks::default();
            for _ in 0..64 {
                tracks.on_notification(TinexNotification::TrackCreated(Id::new()));
            }
            tracks.scroll(100.0, content, 160.0);
            assert!(
                tracks
                    .rows(content, 160.0)
                    .all(|(_, row)| row.y1 <= content.height)
            );
            tracks.cursor = Some(Point::new(size.width - 50.0, content.height + 20.0));
            assert!(tracks.hovered(content, 160.0).is_none());
        }
        assert!(bottom_meter_rect(LogicalSize::new(200.0, 200.0)).is_none());
        assert_eq!(content_size(LogicalSize::new(20.0, 20.0)).height, 0.0);
    }
}
