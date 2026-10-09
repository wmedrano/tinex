use vello::peniko::Color;

/// Colors shared by all GUI rendering.
pub(super) struct Theme {
    pub(super) background: Color,
    pub(super) surface: Color,
    pub(super) foreground: Color,
    pub(super) button_background: Color,
    pub(super) meter_normal: Color,
    pub(super) meter_over: Color,
    pub(super) meter_reference: Color,
    pub(super) selected_background: Color,
    pub(super) create_button_hovered: Color,
    pub(super) create_button_pressed: Color,
    pub(super) remove_button_hovered: Color,
    pub(super) remove_button_pressed: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::from_rgb8(20, 24, 30),
            surface: Color::from_rgb8(28, 33, 41),
            foreground: Color::from_rgb8(236, 240, 244),
            button_background: Color::from_rgb8(48, 54, 64),
            meter_normal: Color::from_rgb8(62, 164, 106),
            meter_over: Color::from_rgb8(230, 76, 76),
            meter_reference: Color::from_rgb8(236, 240, 244),
            selected_background: Color::from_rgb8(48, 75, 100),
            create_button_hovered: Color::from_rgb8(62, 164, 106),
            create_button_pressed: Color::from_rgb8(42, 126, 80),
            remove_button_hovered: Color::from_rgb8(170, 70, 70),
            remove_button_pressed: Color::from_rgb8(140, 56, 56),
        }
    }
}
