use vello::Scene;
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::Fill;

use super::{TextRenderer, Theme};

/// A stateless tooltip. Callers own visibility, placement, and bounds.
pub struct Tooltip<'a>(pub &'a str);

impl Tooltip<'_> {
    pub fn draw(
        &self,
        scene: &mut Scene,
        text: &mut TextRenderer,
        theme: &Theme,
        bounds: Rect,
        scale: f64,
    ) {
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            theme.button_background,
            None,
            &bounds.to_rounded_rect(6.0),
        );
        text.draw(
            scene,
            self.0,
            Point::new(bounds.x0 + 8.0, bounds.y0 + 6.0),
            scale,
            theme.foreground,
        );
    }
}
