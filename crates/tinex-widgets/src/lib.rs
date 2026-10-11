//! Declarative Tinex widgets and text rendering with bundled Noto fonts.
//!
//! Build a [`Widget`] tree from application state, retaining stable [`WidgetId`]s
//! across rebuilds. [`Ui`] owns layout, focus, pointer capture, and scroll offsets;
//! it never owns or updates domain state. Dispatch the actions returned by
//! [`Ui::input`] after traversal, then replace the view with [`Ui::set_view`].
//! Input and layout use logical pixels; [`Ui::render`] applies the display scale.
//!
//! ```
//! use tinex_widgets::{Input, Tone, Ui, Widget, WidgetId};
//! use vello::kurbo::Size;
//!
//! #[derive(Clone)]
//! enum Action { Create }
//! let create_id = WidgetId::new();
//! let view = Widget::Row {
//!     children: vec![
//!         Widget::button(create_id, "Create", 120.0, true, Action::Create),
//!         Widget::label("Ready", Tone::Muted),
//!     ],
//!     surface: false,
//! };
//! let mut ui = Ui::new(view, Size::new(800.0, 450.0));
//! ui.input(Input::Tab { backwards: false });
//! // The caller supplies a TextRenderer, WidgetStyle, Scene, and scale to render.
//! ```
mod framework;
mod level_meter;
mod text;

pub use framework::{ActivationKey, Input, Tone, Ui, Widget, WidgetId, WidgetStyle};
pub use level_meter::{
    LevelMeter, METER_MAX_DB, METER_MIN_DB, MeterStyle, amplitude_to_dbfs, sanitize_level,
};
pub use text::TextRenderer;
