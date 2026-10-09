use std::collections::HashMap;
use tinex_core::TinexNotification;
use tinex_core::id::Id;
use tinex_core::track::Track;
use tinex_widgets::{LevelMeter, TextRenderer, Theme, Tooltip};
use tracing::{info, warn};
use vello::Scene;
use vello::kurbo::{Affine, Line, Point, Rect, Stroke};
use vello::peniko::Fill;
use winit::dpi::LogicalSize;
use winit::event::ElementState;

#[derive(Default)]
pub(super) struct CreateTrackButton {
    pub(super) cursor: Option<Point>,
    pub(super) pressed: bool,
}

impl CreateTrackButton {
    fn rect(sidebar_width: f64) -> Rect {
        Rect::new(sidebar_width + 32.0, 32.0, sidebar_width + 80.0, 72.0)
    }

    pub(super) fn hovered(&self, sidebar_width: f64) -> bool {
        self.cursor
            .is_some_and(|point| Self::rect(sidebar_width).contains(point))
    }

    pub(super) fn mouse_input(&mut self, state: ElementState, sidebar_width: f64) -> bool {
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
pub(super) struct UiTrack {
    pub(super) id: Id<Track>,
    pub(super) name: String,
    pub(super) output_level: f32,
}

const TRACK_ROW_HEIGHT: f64 = 48.0;
pub(super) const TRACK_ROW_STRIDE: f64 = 56.0;
const TRACK_LIST_TOP: f64 = 88.0;

#[derive(Default)]
pub(super) struct UiTracks {
    pub(super) tracks: Vec<UiTrack>,
    pending_names: HashMap<Id<Track>, String>,
    next_number: usize,
    scroll_offset: f64,
    pub(super) cursor: Option<Point>,
    pressed: Option<Id<Track>>,
}

impl UiTracks {
    pub(super) fn reserve_plugin_name(&mut self, id: Id<Track>, plugin_name: &str) {
        let name_is_taken = |name: &str| {
            self.tracks.iter().any(|track| track.name == name)
                || self.pending_names.values().any(|pending| pending == name)
        };
        let name = if !name_is_taken(plugin_name) {
            plugin_name.to_owned()
        } else {
            (1..)
                .map(|number| format!("{plugin_name} {number}"))
                .find(|name| !name_is_taken(name))
                .unwrap()
        };
        self.pending_names.insert(id, name);
    }

    pub(super) fn cancel_pending_name(&mut self, id: Id<Track>) {
        self.pending_names.remove(&id);
    }

    pub(super) fn draw_tooltips(
        &self,
        button: &CreateTrackButton,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        size: LogicalSize<f64>,
        sidebar_width: f64,
        scale: f64,
    ) {
        if button.hovered(sidebar_width) {
            let bounds = CreateTrackButton::rect(sidebar_width);
            let x = bounds.x0;
            let y = bounds.y1 + 4.0;
            Tooltip("Create track").draw(
                scene,
                text,
                theme,
                Rect::new(x, y, x + 120.0, y + 36.0),
                scale,
            );
        }
        if let Some(id) = self.hovered(size, sidebar_width)
            && let Some((_, row)) = self
                .rows(size, sidebar_width)
                .find(|(track, _)| track.id == id)
        {
            let button = remove_button_rect(row);
            let x = (button.x1 - 120.0).max(0.0);
            let y = (button.y0 - 40.0).max(0.0);
            Tooltip("Remove track").draw(
                scene,
                text,
                theme,
                Rect::new(x, y, x + 120.0, y + 36.0),
                scale,
            );
        }
    }

    pub(super) fn draw(
        &self,
        button: &CreateTrackButton,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        size: LogicalSize<f64>,
        sidebar_width: f64,
        scale: f64,
    ) {
        let button_color = if button.hovered(sidebar_width) {
            if button.pressed {
                theme.create_button_pressed
            } else {
                theme.create_button_hovered
            }
        } else {
            theme.button_background
        };
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            button_color,
            None,
            &CreateTrackButton::rect(sidebar_width).to_rounded_rect(6.0),
        );
        // A plus sign is the initial create-track affordance.
        for bar in [
            Rect::new(sidebar_width + 46.0, 50.5, sidebar_width + 66.0, 53.5),
            Rect::new(sidebar_width + 54.5, 42.0, sidebar_width + 57.5, 62.0),
        ] {
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                theme.foreground,
                None,
                &bar,
            );
        }
        for (track, row) in self.rows(size, sidebar_width) {
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                theme.surface,
                None,
                &row.to_rounded_rect(6.0),
            );
            text.draw(
                scene,
                &track.name,
                Point::new(row.x0 + 8.0, row.y0 + 4.0),
                scale,
                theme.foreground,
            );
            LevelMeter(track.output_level).draw(scene, theme, level_meter_rect(row), scale);
            let button = remove_button_rect(row);
            let hovered = self.hovered(size, sidebar_width) == Some(track.id);
            let color = if hovered && self.pressed == Some(track.id) {
                theme.remove_button_pressed
            } else if hovered {
                theme.remove_button_hovered
            } else {
                theme.button_background
            };
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                color,
                None,
                &button.to_rounded_rect(4.0),
            );
            draw_trash_icon(scene, button, theme, scale);
        }
    }
}

impl UiTracks {
    pub(super) fn update_levels(&mut self, levels: &HashMap<Id<Track>, f32>) {
        for track in &mut self.tracks {
            if let Some(level) = levels.get(&track.id) {
                track.output_level = *level;
            }
        }
    }

    /// Observes engine notifications to keep frontend track metadata in sync.
    pub(super) fn on_notification(&mut self, notification: TinexNotification) {
        match notification {
            TinexNotification::TrackCreated(id) => {
                if self.tracks.iter().any(|track| track.id == id) {
                    return;
                }
                let name = self.pending_names.remove(&id).unwrap_or_else(|| {
                    loop {
                        self.next_number += 1;
                        let name = format!("Track {}", self.next_number);
                        if !self.tracks.iter().any(|track| track.name == name)
                            && !self.pending_names.values().any(|pending| pending == &name)
                        {
                            break name;
                        }
                    }
                });
                self.tracks.push(UiTrack {
                    id,
                    name,
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
            TinexNotification::TrackCreationFailed(track) => {
                self.cancel_pending_name(track.id());
                warn!("Track creation failed");
            }
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

    pub(super) fn rows(
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

    pub(super) fn hovered(&self, size: LogicalSize<f64>, sidebar_width: f64) -> Option<Id<Track>> {
        let cursor = self.cursor?;
        self.rows(size, sidebar_width)
            .find(|(_, row)| remove_button_rect(*row).contains(cursor))
            .map(|(track, _)| track.id)
    }

    pub(super) fn mouse_input(
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

    pub(super) fn reset_input(&mut self) {
        self.cursor = None;
        self.pressed = None;
    }

    pub(super) fn scroll(&mut self, rows: f64, size: LogicalSize<f64>, sidebar_width: f64) {
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
mod tests {
    use super::super::content_size;
    use super::*;

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
        let first = Track::with_plugin(tinex_core::plugin::Silence);
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
        let first = Track::with_plugin(tinex_core::plugin::Silence);
        let second = Track::with_plugin(tinex_core::plugin::Silence);
        let first_id = first.id();
        let second_id = second.id();
        for notification in [
            TinexNotification::TrackCreated(first_id),
            TinexNotification::OutputLevel {
                output_level: 0.7,
                tracks: Default::default(),
            },
            TinexNotification::TrackCreated(second_id),
            TinexNotification::TrackCreationFailed(Track::with_plugin(tinex_core::plugin::Silence)),
        ] {
            tracks.on_notification(notification);
        }
        assert_eq!(tracks.tracks[0].id, first_id);
        assert_eq!(tracks.tracks[0].name, "Track 1");
        assert_eq!(tracks.tracks[1].name, "Track 2");
        tracks.on_notification(TinexNotification::TrackDeleted(first));
        assert_eq!(tracks.tracks.len(), 1);
        assert_eq!(tracks.tracks[0].id, second_id);
        assert_eq!(tracks.tracks[0].name, "Track 2");
        for notification in [
            TinexNotification::TrackDeleted(Track::with_plugin(tinex_core::plugin::Silence)),
            TinexNotification::TrackCreated(Id::new()),
        ] {
            tracks.on_notification(notification);
        }
        assert_eq!(tracks.tracks.len(), 2);
        assert_eq!(tracks.tracks[1].name, "Track 3");
    }

    #[test]
    fn plugin_track_names_use_first_available_suffix() {
        let mut tracks = UiTracks::default();
        let first = Track::new();
        let second = Track::new();
        let third = Track::new();
        tracks.reserve_plugin_name(first.id(), "EPiano");
        tracks.reserve_plugin_name(second.id(), "EPiano");
        tracks.reserve_plugin_name(third.id(), "EPiano");
        tracks.on_notification(TinexNotification::TrackCreated(second.id()));
        tracks.on_notification(TinexNotification::TrackCreated(first.id()));
        tracks.on_notification(TinexNotification::TrackCreationFailed(third));
        assert_eq!(tracks.tracks[0].name, "EPiano 1");
        assert_eq!(tracks.tracks[1].name, "EPiano");

        let fourth = Track::new();
        tracks.reserve_plugin_name(fourth.id(), "EPiano");
        tracks.on_notification(TinexNotification::TrackCreated(fourth.id()));
        assert_eq!(tracks.tracks[2].name, "EPiano 2");

        tracks.on_notification(TinexNotification::TrackDeleted(second));
        let fifth = Track::new();
        tracks.reserve_plugin_name(fifth.id(), "EPiano");
        tracks.on_notification(TinexNotification::TrackCreated(fifth.id()));
        assert_eq!(tracks.tracks[2].name, "EPiano 1");
    }

    #[test]
    fn remove_click_targets_id_even_when_rows_shift() {
        let size = LogicalSize::new(800.0, 450.0);
        let first = Track::with_plugin(tinex_core::plugin::Silence);
        let second = Track::with_plugin(tinex_core::plugin::Silence);
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
            assert_eq!(tracks.rows(size, 160.0).next().unwrap().0.name, "Track 1");
            tracks.scroll(100.0, size, 160.0);
            let rows: Vec<_> = tracks.rows(size, 160.0).collect();
            assert_eq!(rows.last().unwrap().0.name, "Track 64");
            for (_, row) in rows {
                assert!(row.y1 <= size.height - 32.0);
                assert!(row.x1 <= size.width - 32.0);
            }
        }
    }
}
