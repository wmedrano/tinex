use super::tracks::UiTrack;
use std::cmp::min;
use tinex_core::{id::Id, track::Track};
use tinex_widgets::{TextRenderer, Theme};
use vello::Scene;
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::{BlendMode, Fill};
use winit::dpi::LogicalSize;

const PLUGIN_PAGE_INSET: f64 = 32.0;
const PLUGIN_LIST_TOP: f64 = 80.0;
const PLUGIN_MIN_PAGE_WIDTH: f64 = 160.0;
const PLUGIN_ROW_HEIGHT: f64 = 56.0;
pub(super) const PLUGIN_ROW_STRIDE: f64 = 64.0;
const PLUGIN_ROW_GAP: f64 = PLUGIN_ROW_STRIDE - PLUGIN_ROW_HEIGHT;
const PLUGIN_CARD_RADIUS: f64 = 6.0;
const PLUGIN_NAME_INSET: f64 = 16.0;
const PLUGIN_ADD_BUTTON_WIDTH: f64 = 56.0;
const PLUGIN_ADD_BUTTON_INSET: f64 = 12.0;
const PLUGIN_ADD_BUTTON_RADIUS: f64 = 4.0;
const PLUGIN_ADD_LABEL_LEFT_INSET: f64 = 12.0;
// TextRenderer's origin is above the visible glyphs, so this is smaller than centered padding.
const PLUGIN_ADD_LABEL_TOP_INSET: f64 = 4.0;
const PLUGIN_MENU_WIDTH: f64 = 240.0;
const PLUGIN_MENU_ACTION_HEIGHT: f64 = 32.0;
const PLUGIN_MENU_SECTION_HEIGHT: f64 = 28.0;
const PLUGIN_MENU_TRACK_HEIGHT: f64 = 32.0;
const PLUGIN_MENU_RADIUS: f64 = 6.0;
const PLUGIN_MENU_SECTION_RADIUS: f64 = 2.0;
const PLUGIN_MENU_TEXT_LEFT_INSET: f64 = 10.0;
const PLUGIN_MENU_ITEM_TEXT_TOP_INSET: f64 = 8.0;
const PLUGIN_MENU_SECTION_TEXT_TOP_INSET: f64 = 5.0;

#[derive(Clone, Copy)]
struct PluginMenuLayout {
    bounds: Rect,
    new_track: Rect,
    section: Rect,
    tracks_top: Rect,
    visible_tracks: usize,
    first_track: usize,
}

struct PluginMenu {
    plugin: tinex_plugins::PluginBuilder,
    anchor: Point,
    track_scroll: f64,
}

pub(super) enum PluginMenuAction {
    NewTrack(tinex_plugins::PluginBuilder),
    AddToTrack(tinex_plugins::PluginBuilder, Id<Track>),
}

pub(super) struct UiPlugins {
    plugins: Vec<tinex_plugins::PluginBuilder>,
    scroll_offset: f64,
    menu: Option<PluginMenu>,
}

impl Default for UiPlugins {
    fn default() -> Self {
        Self {
            plugins: tinex_plugins::FACTORY.to_vec(),
            scroll_offset: 0.0,
            menu: None,
        }
    }
}

impl UiPlugins {
    fn visible_count(size: LogicalSize<f64>, sidebar_width: f64) -> usize {
        if size.width < sidebar_width + PLUGIN_MIN_PAGE_WIDTH {
            return 0;
        }
        ((size.height - PLUGIN_PAGE_INSET - PLUGIN_LIST_TOP + PLUGIN_ROW_GAP) / PLUGIN_ROW_STRIDE)
            .max(0.0) as usize
    }

    fn scroll_offset(&self, size: LogicalSize<f64>, sidebar_width: f64) -> f64 {
        self.scroll_offset.min(
            self.plugins
                .len()
                .saturating_sub(Self::visible_count(size, sidebar_width)) as f64,
        )
    }

    fn rows(
        &self,
        size: LogicalSize<f64>,
        sidebar_width: f64,
    ) -> impl Iterator<Item = (&tinex_plugins::PluginBuilder, Rect)> {
        self.plugins
            .iter()
            .skip(self.scroll_offset(size, sidebar_width).round() as usize)
            .take(Self::visible_count(size, sidebar_width))
            .enumerate()
            .map(move |(index, plugin)| {
                let y = PLUGIN_LIST_TOP + index as f64 * PLUGIN_ROW_STRIDE;
                (
                    plugin,
                    Rect::new(
                        sidebar_width + PLUGIN_PAGE_INSET,
                        y,
                        size.width - PLUGIN_PAGE_INSET,
                        y + PLUGIN_ROW_HEIGHT,
                    ),
                )
            })
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
        text.draw(
            scene,
            "Plugins",
            Point::new(sidebar_width + PLUGIN_PAGE_INSET, PLUGIN_PAGE_INSET),
            scale,
            theme.foreground,
        );
        for (plugin, row) in self.rows(size, sidebar_width) {
            let card = row.to_rounded_rect(PLUGIN_CARD_RADIUS);
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                theme.surface,
                None,
                &card,
            );
            scene.push_layer(
                Fill::NonZero,
                BlendMode::default(),
                1.0,
                Affine::scale(scale),
                &card,
            );
            text.draw(
                scene,
                plugin.name(),
                Point::new(row.x0 + PLUGIN_NAME_INSET, row.y0 + PLUGIN_NAME_INSET),
                scale,
                theme.foreground,
            );
            let button = plugin_add_button_rect(row);
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                theme.button_background,
                None,
                &button.to_rounded_rect(PLUGIN_ADD_BUTTON_RADIUS),
            );
            text.draw(
                scene,
                "Add",
                Point::new(
                    button.x0 + PLUGIN_ADD_LABEL_LEFT_INSET,
                    button.y0 + PLUGIN_ADD_LABEL_TOP_INSET,
                ),
                scale,
                theme.foreground,
            );
            scene.pop_layer();
        }
        self.draw_menu(scene, text, theme, size, tracks, scale);
    }

    fn draw_menu(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        size: LogicalSize<f64>,
        tracks: &[UiTrack],
        scale: f64,
    ) {
        let Some(menu) = &self.menu else {
            return;
        };
        let layout = plugin_menu_layout(menu, size, tracks.len());
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            theme.button_background,
            None,
            &layout.bounds.to_rounded_rect(PLUGIN_MENU_RADIUS),
        );
        text.draw(
            scene,
            &format!("New track with {}", menu.plugin.name()),
            Point::new(
                layout.bounds.x0 + PLUGIN_MENU_TEXT_LEFT_INSET,
                layout.bounds.y0 + PLUGIN_MENU_ITEM_TEXT_TOP_INSET,
            ),
            scale,
            theme.foreground,
        );
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            theme.surface,
            None,
            &layout.section.to_rounded_rect(PLUGIN_MENU_SECTION_RADIUS),
        );
        text.draw(
            scene,
            "Add to existing track",
            Point::new(
                layout.section.x0 + PLUGIN_MENU_TEXT_LEFT_INSET,
                layout.section.y0 + PLUGIN_MENU_SECTION_TEXT_TOP_INSET,
            ),
            scale,
            theme.foreground,
        );
        if tracks.is_empty() {
            text.draw(
                scene,
                "No tracks",
                Point::new(
                    layout.tracks_top.x0 + PLUGIN_MENU_TEXT_LEFT_INSET,
                    layout.tracks_top.y0 + PLUGIN_MENU_ITEM_TEXT_TOP_INSET,
                ),
                scale,
                theme.foreground,
            );
            return;
        }
        for (index, track) in tracks
            .iter()
            .skip(layout.first_track)
            .take(layout.visible_tracks)
            .enumerate()
        {
            let row = plugin_menu_track_rect(layout, index);
            text.draw(
                scene,
                &track.name,
                Point::new(
                    row.x0 + PLUGIN_MENU_TEXT_LEFT_INSET,
                    row.y0 + PLUGIN_MENU_ITEM_TEXT_TOP_INSET,
                ),
                scale,
                theme.foreground,
            );
        }
    }

    pub(super) fn open_menu(
        &mut self,
        cursor: Option<Point>,
        size: LogicalSize<f64>,
        sidebar_width: f64,
    ) {
        let plugin = cursor.and_then(|cursor| {
            self.rows(size, sidebar_width)
                .find(|(_, row)| row.contains(cursor))
                .map(|(plugin, _)| *plugin)
        });
        if let (Some(anchor), Some(plugin)) = (cursor, plugin) {
            self.open_plugin_menu(anchor, plugin, size);
        } else {
            self.close_menu();
        }
    }

    pub(super) fn open_plugin_menu(
        &mut self,
        anchor: Point,
        plugin: tinex_plugins::PluginBuilder,
        size: LogicalSize<f64>,
    ) {
        self.menu = Some(PluginMenu {
            plugin,
            anchor,
            track_scroll: 0.0,
        });
        if let Some(menu) = &mut self.menu {
            let layout = plugin_menu_layout(menu, size, 0);
            menu.anchor = Point::new(layout.bounds.x0, layout.bounds.y0);
        }
    }

    pub(super) fn close_menu(&mut self) {
        self.menu = None;
    }

    pub(super) fn menu_is_open(&self) -> bool {
        self.menu.is_some()
    }

    pub(super) fn add_button_plugin(
        &self,
        cursor: Option<Point>,
        size: LogicalSize<f64>,
        sidebar_width: f64,
    ) -> Option<tinex_plugins::PluginBuilder> {
        let cursor = cursor?;
        self.rows(size, sidebar_width)
            .find(|(_, row)| plugin_add_button_rect(*row).contains(cursor))
            .map(|(plugin, _)| *plugin)
    }

    pub(super) fn menu_action(
        &self,
        cursor: Option<Point>,
        size: LogicalSize<f64>,
        tracks: &[UiTrack],
    ) -> Option<PluginMenuAction> {
        let cursor = cursor?;
        let menu = self.menu.as_ref()?;
        let layout = plugin_menu_layout(menu, size, tracks.len());
        if layout.new_track.contains(cursor) {
            return Some(PluginMenuAction::NewTrack(menu.plugin));
        }
        if tracks.is_empty() {
            return None;
        }
        tracks
            .iter()
            .skip(layout.first_track)
            .take(layout.visible_tracks)
            .enumerate()
            .find(|(index, _)| plugin_menu_track_rect(layout, *index).contains(cursor))
            .map(|(_, track)| PluginMenuAction::AddToTrack(menu.plugin, track.id))
    }

    pub(super) fn menu_contains(
        &self,
        cursor: Option<Point>,
        size: LogicalSize<f64>,
        tracks: &[UiTrack],
    ) -> bool {
        cursor.is_some_and(|cursor| {
            self.menu.as_ref().is_some_and(|menu| {
                plugin_menu_layout(menu, size, tracks.len())
                    .bounds
                    .contains(cursor)
            })
        })
    }

    pub(super) fn scroll_menu(
        &mut self,
        rows: f64,
        size: LogicalSize<f64>,
        tracks: &[UiTrack],
    ) -> bool {
        let Some(menu) = &mut self.menu else {
            return false;
        };
        let max = tracks
            .len()
            .saturating_sub(plugin_menu_layout(menu, size, tracks.len()).visible_tracks);
        menu.track_scroll = (menu.track_scroll + rows).clamp(0.0, max as f64);
        true
    }

    pub(super) fn scroll(&mut self, rows: f64, size: LogicalSize<f64>, sidebar_width: f64) {
        self.scroll_offset = (self.scroll_offset(size, sidebar_width) + rows).clamp(
            0.0,
            self.plugins
                .len()
                .saturating_sub(Self::visible_count(size, sidebar_width)) as f64,
        );
    }
}

fn plugin_add_button_rect(row: Rect) -> Rect {
    Rect::new(
        row.x1 - PLUGIN_ADD_BUTTON_WIDTH - PLUGIN_ADD_BUTTON_INSET,
        row.y0 + PLUGIN_ADD_BUTTON_INSET,
        row.x1 - PLUGIN_ADD_BUTTON_INSET,
        row.y1 - PLUGIN_ADD_BUTTON_INSET,
    )
}

fn plugin_menu_layout(
    menu: &PluginMenu,
    size: LogicalSize<f64>,
    tracks: usize,
) -> PluginMenuLayout {
    let max_track_rows = ((size.height - PLUGIN_MENU_ACTION_HEIGHT - PLUGIN_MENU_SECTION_HEIGHT)
        / PLUGIN_MENU_TRACK_HEIGHT)
        .floor()
        .max(1.0) as usize;
    let visible_tracks = min(tracks, max_track_rows);
    let first_track =
        (menu.track_scroll.round() as usize).min(tracks.saturating_sub(visible_tracks));
    let row_count = visible_tracks.max(1);
    let height = (PLUGIN_MENU_ACTION_HEIGHT
        + PLUGIN_MENU_SECTION_HEIGHT
        + row_count as f64 * PLUGIN_MENU_TRACK_HEIGHT)
        .min(size.height.max(1.0));
    let x = menu
        .anchor
        .x
        .clamp(0.0, (size.width - PLUGIN_MENU_WIDTH).max(0.0));
    let y = menu.anchor.y.clamp(0.0, (size.height - height).max(0.0));
    let bounds = Rect::new(x, y, x + PLUGIN_MENU_WIDTH, y + height);
    let new_track = Rect::new(
        bounds.x0,
        bounds.y0,
        bounds.x1,
        bounds.y0 + PLUGIN_MENU_ACTION_HEIGHT,
    );
    let section = Rect::new(
        bounds.x0,
        new_track.y1,
        bounds.x1,
        new_track.y1 + PLUGIN_MENU_SECTION_HEIGHT,
    );
    let tracks_top = Rect::new(bounds.x0, section.y1, bounds.x1, bounds.y1);
    PluginMenuLayout {
        bounds,
        new_track,
        section,
        tracks_top,
        visible_tracks,
        first_track,
    }
}

fn plugin_menu_track_rect(layout: PluginMenuLayout, index: usize) -> Rect {
    let y0 = layout.tracks_top.y0 + index as f64 * PLUGIN_MENU_TRACK_HEIGHT;
    Rect::new(
        layout.bounds.x0,
        y0,
        layout.bounds.x1,
        (y0 + PLUGIN_MENU_TRACK_HEIGHT).min(layout.bounds.y1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::content_size;

    #[test]
    fn plugin_showcase_renders_registry_at_display_scales() {
        let plugins = UiPlugins::default();
        let registered: Vec<_> = tinex_plugins::FACTORY
            .iter()
            .map(|plugin| plugin.name())
            .collect();
        assert_eq!(
            plugins
                .plugins
                .iter()
                .map(|plugin| plugin.name())
                .collect::<Vec<_>>(),
            registered,
        );
        let mut text = TextRenderer::new();
        let theme = Theme::default();
        for size in [
            LogicalSize::new(400.0, 280.0),
            LogicalSize::new(800.0, 450.0),
        ] {
            let content = content_size(size);
            for width in [64.0, 160.0] {
                assert!(plugins.rows(content, width).next().is_some());
                for scale in [1.0, 1.5, 2.0] {
                    let mut scene = Scene::new();
                    plugins.draw(&mut scene, &mut text, &theme, content, width, scale, &[]);
                    assert!(!scene.encoding().resources.glyph_runs.is_empty());
                    assert!(
                        scene
                            .encoding()
                            .resources
                            .glyphs
                            .iter()
                            .all(|glyph| glyph.id != 0)
                    );
                }
            }
        }
    }

    #[test]
    fn plugin_scrolling_reaches_every_entry_and_clamps_after_resize() {
        let plugin = tinex_plugins::FACTORY[0];
        let mut plugins = UiPlugins {
            plugins: vec![plugin; 64],
            scroll_offset: 0.0,
            menu: None,
        };
        for size in [
            LogicalSize::new(400.0, 280.0),
            LogicalSize::new(800.0, 450.0),
        ] {
            let content = content_size(size);
            for width in [64.0, 160.0] {
                plugins.scroll(-100.0, content, width);
                let mut seen = vec![false; plugins.plugins.len()];
                for _ in 0..plugins.plugins.len() {
                    for (plugin, row) in plugins.rows(content, width) {
                        let index = plugins
                            .plugins
                            .iter()
                            .position(|entry| std::ptr::eq(entry, plugin))
                            .unwrap();
                        seen[index] = true;
                        assert_eq!(row.x0, width + PLUGIN_PAGE_INSET);
                        assert!(row.x1 <= content.width - PLUGIN_PAGE_INSET);
                        assert!(row.y0 >= PLUGIN_LIST_TOP);
                        assert!(row.y1 <= content.height - PLUGIN_PAGE_INSET);
                    }
                    plugins.scroll(1.0, content, width);
                }
                assert!(seen.into_iter().all(|seen| seen));
                plugins.scroll(100.0, content, width);
                assert!(std::ptr::eq(
                    plugins.rows(content, width).last().unwrap().0,
                    plugins.plugins.last().unwrap(),
                ));
                let tall = LogicalSize::new(800.0, 5000.0);
                plugins.scroll(-100.0, tall, width);
                assert!(std::ptr::eq(
                    plugins.rows(tall, width).next().unwrap().0,
                    &plugins.plugins[0],
                ));
            }
        }
        plugins.plugins.clear();
        plugins.scroll(100.0, LogicalSize::new(400.0, 240.0), 160.0);
        assert_eq!(plugins.scroll_offset, 0.0);
        assert_eq!(
            plugins.rows(LogicalSize::new(400.0, 240.0), 160.0).count(),
            0
        );
        assert_eq!(
            UiPlugins::visible_count(LogicalSize::new(100.0, 50.0), 160.0),
            0
        );
    }
}
