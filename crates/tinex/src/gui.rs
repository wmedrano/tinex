mod graphics;
mod sidebar;

use graphics::Graphics;
use sidebar::{Control, Page, Sidebar};
use tinex_widgets::{LevelMeter, TextRenderer, Theme, Tooltip};

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use vello::Scene;
use vello::kurbo::{Affine, Line, Point, Rect, Stroke};
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
        self.draw_create_tooltip(scale);
        self.draw_remove_tooltip(size, scale);
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
        if self.page == Page::Settings {
            self.text.draw(
                &mut self.scene,
                "Settings",
                Point::new(self.sidebar.width() + 32.0, 32.0),
                scale,
                self.theme.foreground,
            );
            return;
        }
        let button_color = if self.create_track_button.hovered(self.sidebar.width()) {
            if self.create_track_button.pressed {
                self.theme.create_button_pressed
            } else {
                self.theme.create_button_hovered
            }
        } else {
            self.theme.button_background
        };
        self.scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            button_color,
            None,
            &CreateTrackButton::rect(self.sidebar.width()).to_rounded_rect(6.0),
        );
        // A plus sign is the initial create-track affordance.
        for bar in [
            Rect::new(
                self.sidebar.width() + 46.0,
                50.5,
                self.sidebar.width() + 66.0,
                53.5,
            ),
            Rect::new(
                self.sidebar.width() + 54.5,
                42.0,
                self.sidebar.width() + 57.5,
                62.0,
            ),
        ] {
            self.scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                self.theme.foreground,
                None,
                &bar,
            );
        }
        for (track, row) in self.tracks.rows(size, self.sidebar.width()) {
            self.scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                self.theme.surface,
                None,
                &row.to_rounded_rect(6.0),
            );
            self.text.draw(
                &mut self.scene,
                &track.name,
                Point::new(row.x0 + 8.0, row.y0 + 4.0),
                scale,
                self.theme.foreground,
            );
            LevelMeter(track.output_level).draw(
                &mut self.scene,
                &self.theme,
                level_meter_rect(row),
                scale,
            );
            let button = remove_button_rect(row);
            let hovered = self.tracks.hovered(size, self.sidebar.width()) == Some(track.id);
            let color = if hovered && self.tracks.pressed == Some(track.id) {
                self.theme.remove_button_pressed
            } else if hovered {
                self.theme.remove_button_hovered
            } else {
                self.theme.button_background
            };
            self.scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                color,
                None,
                &button.to_rounded_rect(4.0),
            );
            draw_trash_icon(&mut self.scene, button, &self.theme, scale);
        }
    }

    fn draw_create_tooltip(&mut self, scale: f64) {
        if self.page != Page::Tracks || !self.create_track_button.hovered(self.sidebar.width()) {
            return;
        }
        let button = CreateTrackButton::rect(self.sidebar.width());
        let x = button.x0;
        let y = button.y1 + 4.0;
        Tooltip("Create track").draw(
            &mut self.scene,
            &mut self.text,
            &self.theme,
            Rect::new(x, y, x + 120.0, y + 36.0),
            scale,
        );
    }

    fn draw_remove_tooltip(&mut self, size: LogicalSize<f64>, scale: f64) {
        if self.page != Page::Tracks {
            return;
        }
        let Some(id) = self.tracks.hovered(size, self.sidebar.width()) else {
            return;
        };
        let Some((_, row)) = self
            .tracks
            .rows(size, self.sidebar.width())
            .find(|(track, _)| track.id == id)
        else {
            return;
        };
        let button = remove_button_rect(row);
        let x = (button.x1 - 120.0).max(0.0);
        let y = (button.y0 - 40.0).max(0.0);
        Tooltip("Remove track").draw(
            &mut self.scene,
            &mut self.text,
            &self.theme,
            Rect::new(x, y, x + 120.0, y + 36.0),
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
                    .with_min_inner_size(LogicalSize::new(400.0, 200.0))
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
                    && let Err(error) = self.handle.requests.send(TinexRequest::NewTrack(
                        Track::new(tinex_plugins::EPiano::new(
                            self.handle.client.as_client().sample_rate() as f32,
                        )),
                    ))
                {
                    warn!(?error, "Could not request track creation");
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
            WindowEvent::MouseWheel { delta, .. } if self.page == Page::Tracks => {
                let window = self.window.as_ref().unwrap();
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -f64::from(y),
                    MouseScrollDelta::PixelDelta(position) => {
                        -position.y / window.scale_factor() / TRACK_ROW_STRIDE
                    }
                };
                self.tracks.scroll(
                    rows,
                    content_size(window.inner_size().to_logical(window.scale_factor())),
                    self.sidebar.width(),
                );
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

#[derive(Default)]
struct CreateTrackButton {
    cursor: Option<Point>,
    pressed: bool,
}

impl CreateTrackButton {
    fn rect(sidebar_width: f64) -> Rect {
        Rect::new(sidebar_width + 32.0, 32.0, sidebar_width + 80.0, 72.0)
    }

    fn hovered(&self, sidebar_width: f64) -> bool {
        self.cursor
            .is_some_and(|point| Self::rect(sidebar_width).contains(point))
    }

    fn mouse_input(&mut self, state: ElementState, sidebar_width: f64) -> bool {
        match state {
            ElementState::Pressed => {
                self.pressed = self.hovered(sidebar_width);
                false
            }
            ElementState::Released => {
                let clicked = self.pressed && self.hovered(sidebar_width);
                self.pressed = false;
                clicked
            }
        }
    }
}

/// Frontend metadata for a track confirmed by the audio engine.
#[derive(Debug)]
struct UiTrack {
    id: Id<Track>,
    name: String,
    output_level: f32,
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
const TRACK_ROW_HEIGHT: f64 = 48.0;
const TRACK_ROW_STRIDE: f64 = 56.0;
const TRACK_LIST_TOP: f64 = 88.0;

#[derive(Default)]
struct UiTracks {
    tracks: Vec<UiTrack>,
    next_number: usize,
    scroll_offset: f64,
    cursor: Option<Point>,
    pressed: Option<Id<Track>>,
}

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

impl UiTracks {
    fn update_levels(&mut self, levels: &HashMap<Id<Track>, f32>) {
        for track in &mut self.tracks {
            if let Some(level) = levels.get(&track.id) {
                track.output_level = *level;
            }
        }
    }

    /// Observes engine notifications to keep frontend track metadata in sync.
    fn on_notification(&mut self, notification: TinexNotification) {
        match notification {
            TinexNotification::TrackCreated(id) => {
                if self.tracks.iter().any(|track| track.id == id) {
                    return;
                }
                self.next_number += 1;
                self.tracks.push(UiTrack {
                    id,
                    name: format!("track-{}", self.next_number),
                    output_level: 0.0,
                });
                info!(?id, "Track created");
            }
            TinexNotification::TrackDeleted(track) => {
                let id = track.id();
                self.tracks.retain(|track| track.id != id);
                if self.pressed == Some(id) {
                    self.pressed = None;
                }
                info!(?id, "Track deleted");
            }
            TinexNotification::TrackCreationFailed(_) => warn!("Track creation failed"),
            TinexNotification::OutputLevel { tracks, .. } => self.update_levels(&tracks),
        }
    }

    fn visible_count(size: LogicalSize<f64>, sidebar_width: f64) -> usize {
        if size.width < sidebar_width + 240.0 {
            return 0;
        }
        ((size.height - 32.0 - TRACK_LIST_TOP + 8.0) / TRACK_ROW_STRIDE).max(0.0) as usize
    }

    fn scroll_offset(&self, size: LogicalSize<f64>, sidebar_width: f64) -> f64 {
        self.scroll_offset.min(
            self.tracks
                .len()
                .saturating_sub(Self::visible_count(size, sidebar_width)) as f64,
        )
    }

    fn first_visible(&self, size: LogicalSize<f64>, sidebar_width: f64) -> usize {
        self.scroll_offset(size, sidebar_width).round() as usize
    }

    fn rows(
        &self,
        size: LogicalSize<f64>,
        sidebar_width: f64,
    ) -> impl Iterator<Item = (&UiTrack, Rect)> {
        self.tracks
            .iter()
            .skip(self.first_visible(size, sidebar_width))
            .take(Self::visible_count(size, sidebar_width))
            .enumerate()
            .map(move |(index, track)| {
                let y = TRACK_LIST_TOP + index as f64 * TRACK_ROW_STRIDE;
                (
                    track,
                    Rect::new(
                        sidebar_width + 32.0,
                        y,
                        size.width - 32.0,
                        y + TRACK_ROW_HEIGHT,
                    ),
                )
            })
    }

    fn hovered(&self, size: LogicalSize<f64>, sidebar_width: f64) -> Option<Id<Track>> {
        let cursor = self.cursor?;
        self.rows(size, sidebar_width)
            .find(|(_, row)| remove_button_rect(*row).contains(cursor))
            .map(|(track, _)| track.id)
    }

    fn mouse_input(
        &mut self,
        state: ElementState,
        size: LogicalSize<f64>,
        sidebar_width: f64,
    ) -> Option<Id<Track>> {
        match state {
            ElementState::Pressed => {
                self.pressed = self.hovered(size, sidebar_width);
                None
            }
            ElementState::Released => self
                .pressed
                .take()
                .filter(|id| Some(*id) == self.hovered(size, sidebar_width)),
        }
    }

    fn reset_input(&mut self) {
        self.cursor = None;
        self.pressed = None;
    }

    fn scroll(&mut self, rows: f64, size: LogicalSize<f64>, sidebar_width: f64) {
        self.scroll_offset = (self.scroll_offset(size, sidebar_width) + rows).clamp(
            0.0,
            self.tracks
                .len()
                .saturating_sub(Self::visible_count(size, sidebar_width)) as f64,
        );
        self.pressed = None;
    }
}

fn draw_trash_icon(scene: &mut Scene, button: Rect, theme: &Theme, scale: f64) {
    let center = button.center();
    let transform = Affine::scale(scale) * Affine::translate((center.x, center.y));
    let stroke = Stroke::new(1.8);
    scene.stroke(
        &stroke,
        transform,
        theme.remove_icon,
        None,
        &Rect::new(-7.0, -5.0, 7.0, 10.0).to_rounded_rect(2.0),
    );
    scene.stroke(
        &stroke,
        transform,
        theme.remove_icon,
        None,
        &Rect::new(-3.0, -10.0, 3.0, -7.0).to_rounded_rect(1.0),
    );
    for line in [
        Line::new((-10.0, -7.0), (10.0, -7.0)),
        Line::new((-2.5, -2.0), (-2.5, 6.0)),
        Line::new((2.5, -2.0), (2.5, 6.0)),
    ] {
        scene.stroke(&stroke, transform, theme.remove_icon, None, &line);
    }
}

fn remove_button_rect(row: Rect) -> Rect {
    Rect::new(row.x1 - 82.0, row.y0 + 6.0, row.x1 - 8.0, row.y1 - 6.0)
}

fn level_meter_rect(row: Rect) -> Rect {
    Rect::new(row.x0 + 8.0, row.y0 + 32.0, row.x1 - 90.0, row.y0 + 38.0)
}

#[cfg(test)]
mod gui_tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn content_geometry_and_hits_follow_sidebar_width() {
        let mut tracks = UiTracks::default();
        for _ in 0..64 {
            tracks.on_notification(TinexNotification::TrackCreated(Id::new()));
        }
        for size in [
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 450.0),
        ] {
            let size = content_size(size);
            tracks.scroll(3.0, size, 160.0);
            let offset = tracks.scroll_offset;
            let first = tracks.rows(size, 160.0).next().map(|(track, _)| track.id);
            for width in [64.0, 160.0] {
                assert_eq!(
                    tracks.rows(size, width).next().map(|(track, _)| track.id),
                    first
                );
                let create = CreateTrackButton::rect(width);
                assert_eq!(create.x0, width + 32.0);
                assert!(create.x1 <= size.width);
                let mut button = CreateTrackButton {
                    cursor: Some(create.center()),
                    pressed: false,
                };
                button.mouse_input(ElementState::Pressed, width);
                assert!(button.mouse_input(ElementState::Released, width));
                let Some((id, row)) = tracks
                    .rows(size, width)
                    .next()
                    .map(|(track, row)| (track.id, row))
                else {
                    assert_eq!(UiTracks::visible_count(size, width), 0);
                    continue;
                };
                assert_eq!(row.x0, width + 32.0);
                assert!(row.y1 <= size.height);
                assert!(level_meter_rect(row).width() > 0.0);
                tracks.cursor = Some(remove_button_rect(row).center());
                tracks.mouse_input(ElementState::Pressed, size, width);
                assert_eq!(
                    tracks.mouse_input(ElementState::Released, size, width),
                    Some(id)
                );
                tracks.reset_input();
                assert_eq!(tracks.scroll_offset, offset);
            }
        }
        let narrow = LogicalSize::new(350.0, 200.0);
        assert_eq!(UiTracks::visible_count(narrow, 160.0), 0);
        assert!(UiTracks::visible_count(narrow, 64.0) > 0);
    }

    #[test]
    fn levels_follow_track_ids_and_ignore_deleted_tracks() {
        let mut tracks = UiTracks::default();
        let first = Track::new(tinex_core::plugin::Silence);
        let second_id = Id::new();
        tracks.on_notification(TinexNotification::TrackCreated(first.id()));
        tracks.on_notification(TinexNotification::TrackCreated(second_id));
        assert!(tracks.tracks.iter().all(|track| track.output_level == 0.0));

        tracks.on_notification(TinexNotification::OutputLevel {
            output_level: 0.9,
            tracks: HashMap::from([(second_id, 0.75), (first.id(), 0.25)]),
        });
        assert_eq!(tracks.tracks[0].output_level, 0.25);
        assert_eq!(tracks.tracks[1].output_level, 0.75);

        let first_id = first.id();
        tracks.on_notification(TinexNotification::TrackDeleted(first));
        tracks.on_notification(TinexNotification::OutputLevel {
            output_level: 0.9,
            tracks: HashMap::from([(first_id, 1.0), (second_id, 0.5)]),
        });
        assert_eq!(tracks.tracks.len(), 1);
        assert_eq!(tracks.tracks[0].id, second_id);
        assert_eq!(tracks.tracks[0].output_level, 0.5);
    }

    #[test]
    fn level_polling_is_paced_and_waits_for_a_response() {
        let (sender, receiver) = mpsc::channel();
        let mut tracks = UiTracks::default();
        let mut meters = UiMeters::default();
        let first = Track::new(tinex_core::plugin::Silence);
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

    #[test]
    fn meters_fit_visible_rows() {
        let mut tracks = UiTracks::default();
        for _ in 0..64 {
            tracks.on_notification(TinexNotification::TrackCreated(Id::new()));
        }
        for size in [
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 450.0),
        ] {
            tracks.scroll(100.0, size, 160.0);
            for (_, row) in tracks.rows(size, 160.0) {
                let meter = level_meter_rect(row);
                assert!(meter.width() > 0.0);
                assert!(row.contains(meter.origin()));
                assert!(row.contains(Point::new(meter.x1, meter.y1)));
                assert!(meter.x1 < remove_button_rect(row).x0);
            }
        }
    }

    #[test]
    fn create_track_requires_a_complete_click_inside_button() {
        let mut button = CreateTrackButton {
            cursor: Some(Point::new(160.0 + 56.0, 52.0)),
            ..Default::default()
        };
        assert!(!button.mouse_input(ElementState::Released, 160.0));
        assert!(!button.mouse_input(ElementState::Pressed, 160.0));
        assert!(button.mouse_input(ElementState::Released, 160.0));
        assert!(!button.mouse_input(ElementState::Released, 160.0));

        button.mouse_input(ElementState::Pressed, 160.0);
        button.cursor = Some(Point::new(100.0, 100.0));
        assert!(!button.mouse_input(ElementState::Released, 160.0));
        button.mouse_input(ElementState::Pressed, 160.0);
        button.cursor = Some(Point::new(160.0 + 56.0, 52.0));
        assert!(!button.mouse_input(ElementState::Released, 160.0));
    }

    #[test]
    fn track_metadata_follows_confirmed_lifecycle() {
        let mut tracks = UiTracks::default();
        let first = Track::new(tinex_core::plugin::Silence);
        let second = Track::new(tinex_core::plugin::Silence);
        let first_id = first.id();
        let second_id = second.id();
        for notification in [
            TinexNotification::TrackCreated(first_id),
            TinexNotification::OutputLevel {
                output_level: 0.7,
                tracks: Default::default(),
            },
            TinexNotification::TrackCreated(second_id),
            TinexNotification::TrackCreationFailed(Track::new(tinex_core::plugin::Silence)),
        ] {
            tracks.on_notification(notification);
        }
        assert_eq!(tracks.tracks[0].id, first_id);
        assert_eq!(tracks.tracks[0].name, "track-1");
        assert_eq!(tracks.tracks[1].name, "track-2");
        tracks.on_notification(TinexNotification::TrackDeleted(first));
        assert_eq!(tracks.tracks.len(), 1);
        assert_eq!(tracks.tracks[0].id, second_id);
        assert_eq!(tracks.tracks[0].name, "track-2");
        for notification in [
            TinexNotification::TrackDeleted(Track::new(tinex_core::plugin::Silence)),
            TinexNotification::TrackCreated(Id::new()),
        ] {
            tracks.on_notification(notification);
        }
        assert_eq!(tracks.tracks.len(), 2);
        assert_eq!(tracks.tracks[1].name, "track-3");
    }

    #[test]
    fn remove_click_targets_id_even_when_rows_shift() {
        let size = LogicalSize::new(800.0, 450.0);
        let first = Track::new(tinex_core::plugin::Silence);
        let second = Track::new(tinex_core::plugin::Silence);
        let mut tracks = UiTracks::default();
        for notification in [
            TinexNotification::TrackCreated(first.id()),
            TinexNotification::TrackCreated(second.id()),
        ] {
            tracks.on_notification(notification);
        }
        let button = remove_button_rect(tracks.rows(size, 160.0).nth(1).unwrap().1);
        tracks.cursor = Some(button.center());
        assert_eq!(
            tracks.mouse_input(ElementState::Released, size, 160.0),
            None
        );
        tracks.mouse_input(ElementState::Pressed, size, 160.0);
        tracks.on_notification(TinexNotification::TrackDeleted(first));
        assert_eq!(
            tracks.mouse_input(ElementState::Released, size, 160.0),
            None
        );
        let button = remove_button_rect(tracks.rows(size, 160.0).next().unwrap().1);
        tracks.cursor = Some(button.center());
        tracks.mouse_input(ElementState::Pressed, size, 160.0);
        assert_eq!(
            tracks.mouse_input(ElementState::Released, size, 160.0),
            Some(second.id())
        );
    }

    #[test]
    fn scrolling_reaches_every_track_with_rows_inside_window() {
        let mut tracks = UiTracks::default();
        for _ in 0..64 {
            tracks.on_notification(TinexNotification::TrackCreated(Id::new()));
        }
        for size in [
            LogicalSize::new(400.0, 200.0),
            LogicalSize::new(800.0, 450.0),
        ] {
            tracks.scroll_offset = 0.0;
            assert_eq!(tracks.rows(size, 160.0).next().unwrap().0.name, "track-1");
            tracks.scroll(100.0, size, 160.0);
            let rows: Vec<_> = tracks.rows(size, 160.0).collect();
            assert_eq!(rows.last().unwrap().0.name, "track-64");
            for (_, row) in rows {
                assert!(row.y1 <= size.height - 32.0);
                assert!(row.x1 <= size.width - 32.0);
            }
        }
    }
}
