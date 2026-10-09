use std::sync::Arc;

use parley::{FontContext, Layout, LayoutContext, PositionedLayoutItem, StyleProperty};
use vello::Scene;
use vello::kurbo::{Affine, Point};
use vello::peniko::{Color, Fill};

pub(super) struct TextRenderer {
    fonts: FontContext,
    context: LayoutContext<()>,
}

impl TextRenderer {
    pub(super) fn new() -> Self {
        let mut fonts = FontContext::new();
        fonts.collection.register_fonts(
            vello::peniko::Blob::new(Arc::new(
                include_bytes!("../../../../resources/notosans/NotoSans.ttf").to_vec(),
            )),
            None,
        );
        Self {
            fonts,
            context: LayoutContext::new(),
        }
    }

    pub(super) fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        origin: Point,
        scale: f64,
        color: Color,
    ) {
        self.draw_aligned(scene, text, origin, scale, color, false);
    }

    pub(super) fn draw_right(
        &mut self,
        scene: &mut Scene,
        text: &str,
        origin: Point,
        scale: f64,
        color: Color,
    ) {
        self.draw_aligned(scene, text, origin, scale, color, true);
    }

    fn draw_aligned(
        &mut self,
        scene: &mut Scene,
        text: &str,
        mut origin: Point,
        scale: f64,
        color: Color,
        right_aligned: bool,
    ) {
        let mut builder = self
            .context
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily("Noto Sans".into()));
        builder.push_default(StyleProperty::FontSize(16.0));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        if right_aligned {
            origin.x -= f64::from(layout.width());
        }
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                scene
                    .draw_glyphs(run.font())
                    .font_size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .transform(Affine::scale(scale) * Affine::translate((origin.x, origin.y)))
                    .brush(color)
                    .draw(
                        Fill::NonZero,
                        glyph_run.positioned_glyphs().map(|glyph| vello::Glyph {
                            id: glyph.id,
                            x: glyph.x,
                            y: glyph.y,
                        }),
                    );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::Theme;

    #[test]
    fn bundled_font_can_render_gui_labels() {
        let mut renderer = TextRenderer::new();
        let mut scene = Scene::new();
        renderer.draw(
            &mut scene,
            "Tracks Settings CPU load: 12.3%",
            Point::ZERO,
            2.0,
            Theme::default().foreground,
        );
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
