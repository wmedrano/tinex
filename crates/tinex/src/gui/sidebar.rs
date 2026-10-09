use vello::Scene;
use vello::kurbo::{Affine, Point, Rect, Stroke};
use vello::peniko::Fill;
use winit::dpi::LogicalSize;
use winit::event::ElementState;

use tinex_widgets::{TextRenderer, Theme, Tooltip};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Page {
    Tracks,
    Settings,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Tracks => "Tracks",
            Self::Settings => "Settings",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Tracks => "🎛️",
            Self::Settings => "⚙️",
        }
    }

    fn rect(self, width: f64) -> Rect {
        let y = match self {
            Self::Tracks => 76.0,
            Self::Settings => 128.0,
        };
        Rect::new(12.0, y, width - 12.0, y + 44.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Control {
    Toggle,
    Page(Page),
}

pub(super) struct Sidebar {
    expanded: bool,
    cursor: Option<Point>,
    pressed: Option<Control>,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            expanded: true,
            cursor: None,
            pressed: None,
        }
    }
}

impl Sidebar {
    const TOGGLE: Rect = Rect::new(12.0, 20.0, 52.0, 60.0);

    pub(super) fn width(&self) -> f64 {
        if self.expanded { 160.0 } else { 64.0 }
    }

    pub(super) fn reset_input(&mut self) {
        self.cursor = None;
        self.pressed = None;
    }

    pub(super) fn draw(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        selected: Page,
        size: LogicalSize<f64>,
        scale: f64,
    ) {
        let transform = Affine::scale(scale);
        scene.fill(
            Fill::NonZero,
            transform,
            theme.surface,
            None,
            &Rect::new(0.0, 0.0, self.width(), size.height),
        );
        if self.hovered() == Some(Control::Toggle) {
            scene.fill(
                Fill::NonZero,
                transform,
                theme.button_background,
                None,
                &Self::TOGGLE.to_rounded_rect(6.0),
            );
        }
        let panel = Rect::new(22.0, 31.0, 42.0, 49.0);
        let pane = Rect::new(22.0, 31.0, 28.0, 49.0);
        scene.stroke(&Stroke::new(1.5), transform, theme.foreground, None, &panel);
        if self.expanded {
            scene.fill(Fill::NonZero, transform, theme.foreground, None, &pane);
        } else {
            scene.stroke(&Stroke::new(1.5), transform, theme.foreground, None, &pane);
        }
        for page in [Page::Tracks, Page::Settings] {
            let rect = page.rect(self.width());
            let color = if selected == page {
                theme.selected_background
            } else if self.hovered() == Some(Control::Page(page)) {
                theme.button_background
            } else {
                theme.surface
            };
            scene.fill(
                Fill::NonZero,
                transform,
                color,
                None,
                &rect.to_rounded_rect(6.0),
            );
            text.draw(
                scene,
                page.icon(),
                Point::new(24.0, rect.y0 + 10.0),
                scale,
                theme.foreground,
            );
            if self.expanded {
                text.draw(
                    scene,
                    page.label(),
                    Point::new(52.0, rect.y0 + 10.0),
                    scale,
                    theme.foreground,
                );
            }
        }
    }

    fn tooltip(&self) -> Option<(&'static str, Rect)> {
        match self.hovered()? {
            Control::Toggle => Some((
                if self.expanded {
                    "Collapse sidebar"
                } else {
                    "Expand sidebar"
                },
                Self::TOGGLE,
            )),
            Control::Page(page) if !self.expanded => Some((page.label(), page.rect(self.width()))),
            _ => None,
        }
    }

    pub(super) fn draw_tooltip(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        size: LogicalSize<f64>,
        scale: f64,
    ) {
        let Some((label, control)) = self.tooltip() else {
            return;
        };
        let width = if matches!(self.hovered(), Some(Control::Toggle)) {
            152.0
        } else {
            88.0
        };
        let x = (self.width() + 8.0).min((size.width - width).max(0.0));
        let y = control.y0.min((size.height - 36.0).max(0.0));
        Tooltip(label).draw(
            scene,
            text,
            theme,
            Rect::new(x, y, x + width, y + 36.0),
            scale,
        );
    }

    pub(super) fn cursor(&self) -> Option<Point> {
        self.cursor
    }

    pub(super) fn set_cursor(&mut self, cursor: Point) {
        self.cursor = Some(cursor);
    }

    pub(super) fn hovered(&self) -> Option<Control> {
        let point = self.cursor?;
        if Self::TOGGLE.contains(point) {
            return Some(Control::Toggle);
        }
        [Page::Tracks, Page::Settings]
            .into_iter()
            .find(|page| page.rect(self.width()).contains(point))
            .map(Control::Page)
    }

    pub(super) fn mouse_input(&mut self, state: ElementState) -> Option<Control> {
        match state {
            ElementState::Pressed => {
                self.pressed = self.hovered();
                None
            }
            ElementState::Released => {
                let clicked = self
                    .pressed
                    .take()
                    .filter(|control| Some(*control) == self.hovered());
                if clicked == Some(Control::Toggle) {
                    self.expanded = !self.expanded;
                }
                clicked
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_clicks_toggle_and_navigate_in_both_widths() {
        let mut sidebar = Sidebar::default();
        assert_eq!(sidebar.width(), 160.0);
        for width in [64.0, 160.0] {
            sidebar.set_cursor(Sidebar::TOGGLE.center());
            assert_eq!(sidebar.mouse_input(ElementState::Released), None);
            sidebar.mouse_input(ElementState::Pressed);
            assert_eq!(
                sidebar.mouse_input(ElementState::Released),
                Some(Control::Toggle)
            );
            assert_eq!(sidebar.width(), width);
            for page in [Page::Tracks, Page::Settings] {
                sidebar.set_cursor(page.rect(width).center());
                sidebar.mouse_input(ElementState::Pressed);
                assert_eq!(
                    sidebar.mouse_input(ElementState::Released),
                    Some(Control::Page(page))
                );
            }
            sidebar.reset_input();
            assert_eq!(sidebar.width(), width);
            assert_eq!(sidebar.hovered(), None);
            assert_eq!(sidebar.mouse_input(ElementState::Released), None);
        }
    }

    #[test]
    fn mismatched_and_reset_clicks_are_rejected() {
        let mut sidebar = Sidebar::default();
        sidebar.set_cursor(Sidebar::TOGGLE.center());
        sidebar.mouse_input(ElementState::Pressed);
        sidebar.set_cursor(Page::Tracks.rect(sidebar.width()).center());
        assert_eq!(sidebar.mouse_input(ElementState::Released), None);
        sidebar.mouse_input(ElementState::Pressed);
        sidebar.set_cursor(Page::Settings.rect(sidebar.width()).center());
        assert_eq!(sidebar.mouse_input(ElementState::Released), None);
        sidebar.mouse_input(ElementState::Pressed);
        sidebar.reset_input();
        sidebar.set_cursor(Page::Settings.rect(sidebar.width()).center());
        assert_eq!(sidebar.mouse_input(ElementState::Released), None);
        assert_eq!(sidebar.width(), 160.0);
    }

    #[test]
    fn sidebar_and_tooltips_render_at_display_scales() {
        let mut sidebar = Sidebar::default();
        let mut text = TextRenderer::new();
        let theme = Theme::default();
        for width in [160.0, 64.0] {
            assert_eq!(sidebar.width(), width);
            for scale in [1.0, 1.5, 2.0] {
                for size in [
                    LogicalSize::new(400.0, 160.0),
                    LogicalSize::new(800.0, 410.0),
                ] {
                    let mut scene = Scene::new();
                    sidebar.draw(&mut scene, &mut text, &theme, Page::Tracks, size, scale);
                    sidebar.set_cursor(Sidebar::TOGGLE.center());
                    sidebar.draw_tooltip(&mut scene, &mut text, &theme, size, scale);
                    assert!(!scene.encoding().is_empty());
                }
            }
            sidebar.mouse_input(ElementState::Pressed);
            sidebar.mouse_input(ElementState::Released);
        }
    }

    #[test]
    fn tooltips_follow_expansion_and_hover() {
        let mut sidebar = Sidebar::default();
        sidebar.set_cursor(Sidebar::TOGGLE.center());
        assert_eq!(sidebar.tooltip().unwrap().0, "Collapse sidebar");
        sidebar.mouse_input(ElementState::Pressed);
        sidebar.mouse_input(ElementState::Released);
        assert_eq!(sidebar.tooltip().unwrap().0, "Expand sidebar");
        sidebar.set_cursor(Page::Tracks.rect(sidebar.width()).center());
        assert_eq!(sidebar.tooltip().unwrap().0, "Tracks");
        sidebar.reset_input();
        assert!(sidebar.tooltip().is_none());
    }
}
