//! Bundled fonts loaded into Xilem/Masonry, not an independent text renderer.
/// Font stack shared by labels and control content.
// System sans-serif supplies mathematical symbols (such as infinity) absent
// from the bundled Noto Sans subset; text and emoji prefer the bundled fonts.
pub const FONT_STACK: &str = "Noto Sans, Noto Color Emoji, sans-serif";
/// Bundled Noto Sans variable font data.
pub const FONT_SANS: &[u8] = include_bytes!("../../../resources/notosans/NotoSans.ttf");
/// Bundled Noto Color Emoji font data.
pub const FONT_EMOJI: &[u8] =
    include_bytes!("../../../resources/noto-color-emoji/NotoColorEmoji.ttf");

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use xilem::{
        Blob,
        masonry::parley::{
            FontContext, Layout, LayoutContext, PositionedLayoutItem, StyleProperty,
        },
    };

    #[test]
    fn bundled_fonts_shape_labels_and_complete_emoji_sequences() {
        let mut fonts = FontContext::new();
        fonts
            .collection
            .register_fonts(Blob::new(Arc::new(FONT_SANS.to_vec())), None);
        fonts
            .collection
            .register_fonts(Blob::new(Arc::new(FONT_EMOJI.to_vec())), None);
        let mut context = LayoutContext::<()>::new();
        for text in [
            "Tracks −60 dB · CPU 12.3%",
            "😀",
            "👍🏽",
            "🇺🇸",
            "❤️",
            "👩‍💻",
            "🎛️",
            "⚙️",
            "🖥️",
            "🔌",
            "🎹",
        ] {
            let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
            builder.push_default(StyleProperty::FontStack(FONT_STACK.into()));
            let mut layout: Layout<()> = builder.build(text);
            layout.break_all_lines(None);
            let mut visible_glyphs = 0;
            for line in layout.lines() {
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(run) = item else {
                        continue;
                    };
                    if text != "Tracks −60 dB · CPU 12.3%" {
                        assert_eq!(
                            run.run().font().data.as_ref(),
                            FONT_EMOJI,
                            "wrong font for {text}"
                        );
                    }
                    for glyph in run.positioned_glyphs() {
                        assert_ne!(glyph.id, 0, "missing glyph for {text}");
                        if glyph.advance > 0.0 {
                            visible_glyphs += 1;
                        }
                    }
                }
            }
            assert!(visible_glyphs > 0);
            if text != "Tracks −60 dB · CPU 12.3%" {
                assert_eq!(visible_glyphs, 1, "emoji not shaped as a sequence: {text}");
            }
        }
    }
}
