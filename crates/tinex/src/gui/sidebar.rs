use vello::Scene;
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::Fill;
use winit::dpi::LogicalSize;
use winit::event::ElementState;

use super::text::TextRenderer;
use super::theme::Theme;

pub(super) const SIDEBAR_WIDTH: f64 = 160.0;

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
    fn rect(self) -> Rect {
        let y = match self {
            Self::Tracks => 24.0,
            Self::Settings => 76.0,
        };
        Rect::new(12.0, y, SIDEBAR_WIDTH - 12.0, y + 44.0)
    }
}

#[derive(Default)]
pub(super) struct Sidebar {
    cursor: Option<Point>,
    pressed: Option<Page>,
}

impl Sidebar {
    pub(super) fn draw(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        selected: Page,
        size: LogicalSize<f64>,
        scale: f64,
    ) {
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            theme.surface,
            None,
            &Rect::new(0.0, 0.0, SIDEBAR_WIDTH, size.height),
        );
        for page in [Page::Tracks, Page::Settings] {
            let rect = page.rect();
            let color = if selected == page {
                theme.selected_background
            } else if self.hovered() == Some(page) {
                theme.button_background
            } else {
                theme.surface
            };
            scene.fill(
                Fill::NonZero,
                Affine::scale(scale),
                color,
                None,
                &rect.to_rounded_rect(6.0),
            );
            text.draw(
                scene,
                page.label(),
                Point::new(rect.x0 + 12.0, rect.y0 + 10.0),
                scale,
                theme.foreground,
            );
        }
    }

    pub(super) fn set_cursor(&mut self, cursor: Point) {
        self.cursor = Some(cursor);
    }

    pub(super) fn hovered(&self) -> Option<Page> {
        self.cursor.and_then(|point| {
            [Page::Tracks, Page::Settings]
                .into_iter()
                .find(|page| page.rect().contains(point))
        })
    }
    pub(super) fn mouse_input(&mut self, state: ElementState) -> Option<Page> {
        match state {
            ElementState::Pressed => {
                self.pressed = self.hovered();
                None
            }
            ElementState::Released => self
                .pressed
                .take()
                .filter(|page| Some(*page) == self.hovered()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_requires_press_and_release_on_the_same_page() {
        let mut sidebar = Sidebar {
            cursor: Some(Point::new(40.0, 40.0)),
            ..Default::default()
        };
        assert_eq!(sidebar.mouse_input(ElementState::Released), None);
        sidebar.mouse_input(ElementState::Pressed);
        sidebar.cursor = Some(Point::new(40.0, 90.0));
        assert_eq!(sidebar.mouse_input(ElementState::Released), None);
        sidebar.mouse_input(ElementState::Pressed);
        assert_eq!(
            sidebar.mouse_input(ElementState::Released),
            Some(Page::Settings)
        );
    }
}
