use std::sync::Arc;

use parley::{FontContext, Layout, LayoutContext, PositionedLayoutItem, StyleProperty};
use vello::Scene;
use vello::kurbo::{Affine, Point};
use vello::peniko::{Color, Fill};

pub struct TextRenderer {
    fonts: FontContext,
    context: LayoutContext<()>,
}

impl TextRenderer {
    pub fn new() -> Self {
        let mut fonts = FontContext::new();
        fonts.collection.register_fonts(
            vello::peniko::Blob::new(Arc::new(
                include_bytes!("../../../resources/notosans/NotoSans.ttf").to_vec(),
            )),
            None,
        );
        fonts.collection.register_fonts(
            vello::peniko::Blob::new(Arc::new(
                include_bytes!("../../../resources/openmoji/OpenMoji.ttf").to_vec(),
            )),
            None,
        );
        Self {
            fonts,
            context: LayoutContext::new(),
        }
    }

    pub fn draw(&mut self, scene: &mut Scene, text: &str, origin: Point, scale: f64, color: Color) {
        self.draw_aligned(scene, text, origin, scale, color, false);
    }

    pub fn draw_right(
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
        builder.push_default(StyleProperty::FontFamily("Noto Sans, OpenMoji".into()));
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
    use crate::Theme;

    #[test]
    fn emoji_sequences_use_bundled_openmoji() {
        let mut renderer = TextRenderer::new();
        for text in ["😀", "👍🏽", "🇺🇸", "❤️", "👩‍💻", "🎛️", "⚙️", "🖥️"]
        {
            let mut builder =
                renderer
                    .context
                    .ranged_builder(&mut renderer.fonts, text, 1.0, false);
            builder.push_default(StyleProperty::FontFamily("Noto Sans, OpenMoji".into()));
            let mut layout: Layout<()> = builder.build(text);
            layout.break_all_lines(None);
            let mut glyph_count = 0;
            for line in layout.lines() {
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                        continue;
                    };
                    assert_eq!(
                        glyph_run.run().font().data.as_ref(),
                        include_bytes!("../../../resources/openmoji/OpenMoji.ttf").as_slice(),
                        "wrong font for {text}",
                    );
                    for glyph in glyph_run.positioned_glyphs() {
                        assert_ne!(glyph.id, 0, "missing glyph for {text}");
                        // Shaping may retain an invisible variation selector with zero advance.
                        if glyph.advance > 0.0 {
                            glyph_count += 1;
                        }
                    }
                }
            }
            assert_eq!(
                glyph_count, 1,
                "sequence should shape into one emoji: {text}"
            );
            let mut scene = Scene::new();
            renderer.draw(
                &mut scene,
                text,
                Point::ZERO,
                2.0,
                Theme::default().foreground,
            );
            assert!(!scene.encoding().is_empty(), "empty scene for {text}");
        }
    }

    #[test]
    fn bundled_font_can_render_gui_labels() {
        let mut renderer = TextRenderer::new();
        let mut scene = Scene::new();
        renderer.draw(
            &mut scene,
            "Tracks Settings 🖥️ CPU 12.3%",
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
