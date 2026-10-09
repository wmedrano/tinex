use super::tracks::{UiTrack, draw_trash_icon, level_meter_rect, remove_button_rect};
use tinex_core::{id::Id, track::Track};
use tinex_widgets::{LevelMeter, TextRenderer, Theme, Tooltip};
use vello::Scene;
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::Fill;
use winit::{dpi::LogicalSize, event::ElementState};

const PLUGIN_TOP: f64 = 174.0;
const PLUGIN_STRIDE: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrackAction {
    Previous,
    Next,
    Remove(Id<Track>),
}

#[derive(Default)]
pub(super) struct UiTrackPage {
    selected: Option<Id<Track>>,
    cursor: Option<Point>,
    pressed: Option<TrackAction>,
    scroll_offset: usize,
}

impl UiTrackPage {
    pub(super) fn selected_index(&self, tracks: &[UiTrack]) -> Option<usize> {
        tracks
            .iter()
            .position(|track| Some(track.id) == self.selected)
    }

    pub(super) fn reconcile(&mut self, tracks: &[UiTrack], previous_index: Option<usize>) {
        if self.selected_index(tracks).is_none() {
            self.selected = tracks
                .get(
                    previous_index
                        .unwrap_or(0)
                        .min(tracks.len().saturating_sub(1)),
                )
                .map(|track| track.id);
            self.scroll_offset = 0;
            self.pressed = None;
        }
    }

    pub(super) fn set_cursor(&mut self, cursor: Option<Point>) {
        self.cursor = cursor;
    }

    pub(super) fn reset_input(&mut self) {
        self.cursor = None;
        self.pressed = None;
    }

    pub(super) fn navigate(&mut self, action: TrackAction, tracks: &[UiTrack]) {
        let Some(index) = self.selected_index(tracks) else {
            return;
        };
        let next = match action {
            TrackAction::Previous => (index + tracks.len() - 1) % tracks.len(),
            TrackAction::Next => (index + 1) % tracks.len(),
            TrackAction::Remove(_) => return,
        };
        self.selected = Some(tracks[next].id);
        self.scroll_offset = 0;
    }

    fn buttons(size: LogicalSize<f64>, sidebar_width: f64) -> Option<[Rect; 3]> {
        let left = sidebar_width + 24.0;
        let right = size.width - 24.0;
        (right - left >= 136.0).then(|| {
            let track = Self::track_card(size, sidebar_width);
            [
                Rect::new(left, 24.0, left + 36.0, 60.0),
                Rect::new(left + 40.0, 24.0, left + 76.0, 60.0),
                remove_button_rect(track),
            ]
        })
    }

    fn track_card(size: LogicalSize<f64>, sidebar_width: f64) -> Rect {
        Rect::new(sidebar_width + 24.0, 80.0, size.width - 24.0, 128.0)
    }

    fn hovered(
        &self,
        size: LogicalSize<f64>,
        sidebar_width: f64,
        tracks: &[UiTrack],
    ) -> Option<TrackAction> {
        let cursor = self.cursor?;
        let id = tracks.get(self.selected_index(tracks)?)?.id;
        let [previous, next, remove] = Self::buttons(size, sidebar_width)?;
        if previous.contains(cursor) {
            Some(TrackAction::Previous)
        } else if next.contains(cursor) {
            Some(TrackAction::Next)
        } else if remove.contains(cursor) {
            Some(TrackAction::Remove(id))
        } else {
            None
        }
    }

    pub(super) fn is_hovered(
        &self,
        size: LogicalSize<f64>,
        sidebar_width: f64,
        tracks: &[UiTrack],
    ) -> bool {
        self.hovered(size, sidebar_width, tracks).is_some()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_tooltip(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        size: LogicalSize<f64>,
        sidebar_width: f64,
        scale: f64,
        tracks: &[UiTrack],
    ) {
        if !matches!(
            self.hovered(size, sidebar_width, tracks),
            Some(TrackAction::Remove(_))
        ) {
            return;
        }
        let Some([_, _, button]) = Self::buttons(size, sidebar_width) else {
            return;
        };
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

    pub(super) fn mouse_input(
        &mut self,
        state: ElementState,
        size: LogicalSize<f64>,
        sidebar_width: f64,
        tracks: &[UiTrack],
    ) -> Option<TrackAction> {
        match state {
            ElementState::Pressed => {
                self.pressed = self.hovered(size, sidebar_width, tracks);
                None
            }
            ElementState::Released => self
                .pressed
                .take()
                .filter(|action| Some(*action) == self.hovered(size, sidebar_width, tracks)),
        }
    }

    pub(super) fn scroll(&mut self, rows: f64, size: LogicalSize<f64>, tracks: &[UiTrack]) {
        let Some(track) = self
            .selected_index(tracks)
            .and_then(|index| tracks.get(index))
        else {
            return;
        };
        let visible = Self::visible_plugins(size);
        let max = track.plugins.len().saturating_sub(visible);
        self.scroll_offset = (self.scroll_offset as f64 + rows)
            .round()
            .clamp(0.0, max as f64) as usize;
    }

    fn visible_plugins(size: LogicalSize<f64>) -> usize {
        if size.height < PLUGIN_TOP + 48.0 {
            0
        } else {
            1 + ((size.height - PLUGIN_TOP - 48.0) / PLUGIN_STRIDE).floor() as usize
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        size: LogicalSize<f64>,
        sidebar_width: f64,
        scale: f64,
        tracks: &[UiTrack],
    ) {
        let left = sidebar_width + 24.0;
        let right = size.width - 24.0;
        let Some(index) = self.selected_index(tracks) else {
            let bounds = Rect::new(left, 24.0, right, 80.0);
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                theme.surface,
                None,
                &bounds.to_rounded_rect(5.0),
            );
            text.draw(
                scene,
                "No tracks",
                Point::new(left + 12.0, 38.0),
                scale,
                theme.foreground,
            );
            return;
        };
        let Some([previous, next, remove]) = Self::buttons(size, sidebar_width) else {
            return;
        };
        let track = &tracks[index];
        for (button, label, action) in [
            (previous, "‹", TrackAction::Previous),
            (next, "›", TrackAction::Next),
        ] {
            let hovered = self.hovered(size, sidebar_width, tracks) == Some(action);
            let color = if hovered && self.pressed == Some(action) {
                theme.create_button_pressed
            } else if hovered {
                theme.create_button_hovered
            } else {
                theme.button_background
            };
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                color,
                None,
                &button.to_rounded_rect(5.0),
            );
            text.draw(
                scene,
                label,
                Point::new(button.x0 + 9.0, button.y0 + 6.0),
                scale,
                theme.foreground,
            );
        }
        text.draw(
            scene,
            &format!("{} / {}", index + 1, tracks.len()),
            Point::new(left + 88.0, 31.0),
            scale,
            theme.foreground,
        );
        let card = Self::track_card(size, sidebar_width);
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            theme.surface,
            None,
            &card.to_rounded_rect(6.0),
        );
        text.draw(
            scene,
            &track.name,
            Point::new(card.x0 + 8.0, card.y0 + 4.0),
            scale,
            theme.foreground,
        );
        LevelMeter(track.output_level).draw(scene, theme, level_meter_rect(card), scale);
        let hovered =
            self.hovered(size, sidebar_width, tracks) == Some(TrackAction::Remove(track.id));
        let color = if hovered && self.pressed == Some(TrackAction::Remove(track.id)) {
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
            &remove.to_rounded_rect(4.0),
        );
        draw_trash_icon(scene, remove, theme, scale);
        text.draw(
            scene,
            "Plugins",
            Point::new(left + 8.0, 144.0),
            scale,
            theme.foreground,
        );
        if track.plugins.is_empty() {
            text.draw(
                scene,
                "No plugins",
                Point::new(left + 8.0, PLUGIN_TOP),
                scale,
                theme.foreground,
            );
        }
        let visible = Self::visible_plugins(size);
        let first = self
            .scroll_offset
            .min(track.plugins.len().saturating_sub(visible));
        for (row_index, plugin) in track.plugins.iter().skip(first).take(visible).enumerate() {
            let top = PLUGIN_TOP + row_index as f64 * PLUGIN_STRIDE;
            let row = Rect::new(left, top, right, top + 32.0);
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                theme.surface,
                None,
                &row.to_rounded_rect(5.0),
            );
            text.draw(
                scene,
                plugin,
                Point::new(left + 8.0, top + 5.0),
                scale,
                theme.foreground,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::content_size;
    use tinex_core::TinexNotification;

    #[test]
    fn navigation_wraps_and_selection_follows_deletion() {
        let mut tracks = super::super::tracks::UiTracks::default();
        let mut models: Vec<_> = (0..3).map(|_| Track::new()).collect();
        for track in &models {
            tracks.on_notification(TinexNotification::TrackCreated(track.id()));
        }
        let mut page = UiTrackPage::default();
        page.reconcile(&tracks.tracks, None);
        assert_eq!(page.selected_index(&tracks.tracks), Some(0));
        page.navigate(TrackAction::Previous, &tracks.tracks);
        assert_eq!(page.selected_index(&tracks.tracks), Some(2));
        page.navigate(TrackAction::Next, &tracks.tracks);
        assert_eq!(page.selected_index(&tracks.tracks), Some(0));
        page.navigate(TrackAction::Next, &tracks.tracks);
        let old_index = page.selected_index(&tracks.tracks);
        tracks.on_notification(TinexNotification::TrackDeleted(models.remove(1)));
        page.reconcile(&tracks.tracks, old_index);
        assert_eq!(page.selected_index(&tracks.tracks), Some(1));
        tracks.on_notification(TinexNotification::TrackDeleted(models.remove(1)));
        page.reconcile(&tracks.tracks, Some(1));
        assert_eq!(page.selected_index(&tracks.tracks), Some(0));
        tracks.on_notification(TinexNotification::TrackDeleted(models.remove(0)));
        page.reconcile(&tracks.tracks, Some(0));
        assert_eq!(page.selected_index(&tracks.tracks), None);
    }

    #[test]
    fn buttons_require_complete_click_and_render_at_supported_sizes() {
        let mut tracks = super::super::tracks::UiTracks::default();
        tracks.on_notification(TinexNotification::TrackCreated(Id::new()));
        let mut page = UiTrackPage::default();
        page.reconcile(&tracks.tracks, None);
        let mut text = TextRenderer::new();
        let theme = Theme::default();
        for width in [64.0, 160.0] {
            for physical in [
                LogicalSize::new(400.0, 280.0),
                LogicalSize::new(800.0, 450.0),
            ] {
                let size = content_size(physical);
                let [previous, next, remove] = UiTrackPage::buttons(size, width).unwrap();
                let card = UiTrackPage::track_card(size, width);
                assert!(previous.x0 > width);
                assert_eq!(next.x0 - previous.x1, 4.0);
                assert!(next.x1 <= size.width);
                assert!(remove.y1 <= size.height);
                assert_eq!(remove, remove_button_rect(card));
                assert!(level_meter_rect(card).x1 < remove.x0);
                assert!(UiTrackPage::visible_plugins(size) >= 1);
                page.set_cursor(Some(remove.center()));
                assert_eq!(
                    page.mouse_input(ElementState::Released, size, width, &tracks.tracks),
                    None
                );
                page.mouse_input(ElementState::Pressed, size, width, &tracks.tracks);
                page.set_cursor(Some(previous.center()));
                assert_eq!(
                    page.mouse_input(ElementState::Released, size, width, &tracks.tracks),
                    None
                );
                page.set_cursor(Some(remove.center()));
                page.mouse_input(ElementState::Pressed, size, width, &tracks.tracks);
                assert_eq!(
                    page.mouse_input(ElementState::Released, size, width, &tracks.tracks),
                    Some(TrackAction::Remove(tracks.tracks[0].id))
                );
                page.set_cursor(Some(next.center()));
                page.mouse_input(ElementState::Pressed, size, width, &tracks.tracks);
                assert_eq!(
                    page.mouse_input(ElementState::Released, size, width, &tracks.tracks),
                    Some(TrackAction::Next)
                );
                for scale in [1.0, 1.5, 2.0] {
                    let mut scene = Scene::new();
                    page.draw(
                        &mut scene,
                        &mut text,
                        &theme,
                        size,
                        width,
                        scale,
                        &tracks.tracks,
                    );
                    assert!(!scene.encoding().is_empty());
                    let mut empty = Scene::new();
                    page.draw(&mut empty, &mut text, &theme, size, width, scale, &[]);
                    assert!(!empty.encoding().is_empty());
                }
            }
        }
    }
}
