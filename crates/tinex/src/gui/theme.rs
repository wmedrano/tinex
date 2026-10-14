use tinex_widgets::MeterStyle;
use xilem::Color;
use xilem::masonry::{
    core::DefaultProperties,
    properties::{
        ActiveBackground, Background, BorderColor, ContentColor, DisabledBackground,
        DisabledContentColor, HoveredBorderColor,
    },
    widgets::Button,
};

pub(super) const BACKGROUND: Color = Color::from_rgb8(40, 42, 54);
pub(super) const TEXT: Color = Color::from_rgb8(248, 248, 242);
pub(super) const MUTED: Color = Color::from_rgb8(160, 166, 190);
pub(super) const ERROR: Color = Color::from_rgb8(255, 85, 85);
pub(super) const SURFACE: Color = Color::from_rgb8(48, 50, 64);
pub(super) const SELECTED: Color = Color::from_rgb8(150, 45, 60);

pub(super) fn meter_style() -> MeterStyle {
    MeterStyle {
        trough: BACKGROUND,
        normal: Color::from_rgb8(80, 250, 123),
        warning: Color::from_rgb8(241, 250, 140),
        near_ceiling: Color::from_rgb8(255, 184, 108),
        over: ERROR,
        reference: TEXT,
    }
}

pub(super) fn properties() -> DefaultProperties {
    let mut props = xilem::masonry::theme::default_property_set();
    props.insert::<Button, _>(Background::Color(Color::from_rgb8(68, 71, 90)));
    props.insert::<Button, _>(HoveredBorderColor(BorderColor {
        color: Color::from_rgb8(88, 91, 116),
    }));
    props.insert::<Button, _>(ActiveBackground(Background::Color(Color::from_rgb8(
        98, 85, 130,
    ))));
    props.insert::<Button, _>(DisabledBackground(Background::Color(Color::from_rgb8(
        55, 57, 72,
    ))));
    props.insert::<xilem::masonry::widgets::Label, _>(ContentColor { color: TEXT });
    props.insert::<xilem::masonry::widgets::Label, _>(DisabledContentColor(ContentColor {
        color: MUTED,
    }));
    props
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meter_palette_uses_conventional_severity_colors() {
        let meter = meter_style();
        assert_eq!(meter.normal, Color::from_rgb8(80, 250, 123));
        assert_eq!(meter.warning, Color::from_rgb8(241, 250, 140));
        assert_eq!(meter.near_ceiling, Color::from_rgb8(255, 184, 108));
        assert_eq!(meter.over, ERROR);
        assert_ne!(meter.reference, meter.trough);
    }
    #[test]
    fn default_background_is_dracula_dark() {
        assert_eq!(BACKGROUND, Color::from_rgb8(40, 42, 54));
    }
}
