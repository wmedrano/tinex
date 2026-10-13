use tinex_widgets::{MeterStyle, WidgetStyle};
use vello::peniko::Color;

/// Colors used by the GUI.
pub(super) struct Theme {
    pub background: Color,
    pub widgets: WidgetStyle,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::from_rgb8(40, 42, 54),
            widgets: WidgetStyle {
                text: Color::from_rgb8(248, 248, 242),
                muted: Color::from_rgb8(160, 166, 190),
                error: Color::from_rgb8(255, 85, 85),
                surface: Color::from_rgb8(48, 50, 64),
                button: Color::from_rgb8(68, 71, 90),
                selected_button: Color::from_rgb8(150, 45, 60),
                selected_hovered: Color::from_rgb8(180, 55, 70),
                selected_pressed: Color::from_rgb8(120, 35, 48),
                selected_disabled: Color::from_rgb8(90, 45, 52),
                hovered: Color::from_rgb8(88, 91, 116),
                pressed: Color::from_rgb8(98, 85, 130),
                disabled: Color::from_rgb8(55, 57, 72),
                focus: Color::from_rgb8(189, 147, 249),
                meter: MeterStyle {
                    trough: Color::from_rgb8(40, 42, 54),
                    normal: Color::from_rgb8(80, 250, 123),
                    warning: Color::from_rgb8(241, 250, 140),
                    near_ceiling: Color::from_rgb8(255, 184, 108),
                    over: Color::from_rgb8(255, 85, 85),
                    reference: Color::from_rgb8(248, 248, 242),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_palette_uses_conventional_severity_colors() {
        let theme = Theme::default();
        assert_eq!(theme.widgets.meter.normal, Color::from_rgb8(80, 250, 123));
        assert_eq!(theme.widgets.meter.warning, Color::from_rgb8(241, 250, 140));
        assert_eq!(
            theme.widgets.meter.near_ceiling,
            Color::from_rgb8(255, 184, 108)
        );
        assert_eq!(theme.widgets.meter.over, Color::from_rgb8(255, 85, 85));
        assert_ne!(theme.widgets.meter.reference, theme.widgets.meter.trough);
    }

    #[test]
    fn default_background_is_dracula_dark() {
        assert_eq!(Theme::default().background, Color::from_rgb8(40, 42, 54));
    }
}
