//! A noninteractive, accessible segmented level meter, managed by Masonry.
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NoAction, PaintCtx, PropertiesRef, Widget,
};
use xilem::masonry::kurbo::Size;
use xilem::masonry::vello::Scene;
use xilem::{Pod, ViewCtx};

use crate::{LevelMeter, MeterStyle};

/// Create a meter in logical coordinates. Masonry applies display scaling.
pub fn level_meter(amplitude: f32, width: f64, height: f64, style: MeterStyle) -> Meter {
    Meter {
        reading: Reading::Dbfs(LevelMeter::new(amplitude)),
        size: Size::new(width, height),
        style,
    }
}

/// A noninteractive percentage bar, suitable for CPU load. Compose a Xilem
/// label above it for the readout; it owns no independent text renderer.
pub fn percentage_meter(value: f64, width: f64, height: f64, style: MeterStyle) -> Meter {
    let value = if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    };
    Meter {
        reading: Reading::Percentage(value),
        size: Size::new(width, height),
        style,
    }
}

#[derive(Clone, Copy)]
enum Reading {
    Dbfs(LevelMeter),
    Percentage(f64),
}
impl Reading {
    fn text(self) -> String {
        match self {
            Self::Dbfs(level) if level.amplitude == 0.0 => "Silence".into(),
            Self::Dbfs(level) => format!("{:+.1} dBFS", level.dbfs()),
            Self::Percentage(value) => format!("{value:.1}%"),
        }
    }
    fn value(self) -> f64 {
        match self {
            Self::Dbfs(level) => f64::from(level.amplitude),
            Self::Percentage(value) => value,
        }
    }
}

/// Xilem view for a dBFS meter.
pub struct Meter {
    reading: Reading,
    size: Size,
    style: MeterStyle,
}

/// Masonry leaf widget. It never polls the engine or participates in focus.
pub struct MeterWidget {
    reading: Reading,
    size: Size,
    style: MeterStyle,
}

impl Widget for MeterWidget {
    type Action = NoAction;

    fn register_children(&mut self, _: &mut xilem::masonry::core::RegisterCtx<'_>) {}

    fn layout(
        &mut self,
        _: &mut LayoutCtx<'_>,
        _: &mut xilem::masonry::core::PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        bc.constrain(self.size)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, scene: &mut Scene) {
        let rect = ctx.size().to_rect();
        match self.reading {
            Reading::Dbfs(level) => level.render(scene, rect, 1.0, &self.style),
            Reading::Percentage(value) => {
                use xilem::masonry::{kurbo::Affine, peniko::Fill};
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    self.style.trough,
                    None,
                    &rect,
                );
                let filled = xilem::masonry::kurbo::Rect::new(
                    0.0,
                    0.0,
                    rect.width() * value / 100.0,
                    rect.height(),
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    self.style.normal,
                    None,
                    &filled,
                );
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        match self.reading {
            Reading::Dbfs(_) => Role::Meter,
            Reading::Percentage(_) => Role::ProgressIndicator,
        }
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        match self.reading {
            Reading::Dbfs(level) => {
                node.set_label("Output level");
                // AccessKit numeric values must be finite. Preserve overload/
                // silence in the text; limit only the accessible numeric scale.
                node.set_min_numeric_value(crate::METER_MIN_DB);
                node.set_max_numeric_value(crate::METER_MAX_DB);
                node.set_numeric_value(
                    level.dbfs().clamp(crate::METER_MIN_DB, crate::METER_MAX_DB),
                );
            }
            Reading::Percentage(value) => {
                node.set_label("CPU load");
                node.set_min_numeric_value(0.0);
                node.set_max_numeric_value(100.0);
                node.set_numeric_value(value);
            }
        }
        node.set_value(self.reading.text());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.reading.text())
    }
}

impl ViewMarker for Meter {}
impl<State, Action> View<State, Action, ViewCtx> for Meter {
    type Element = Pod<MeterWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (
            ctx.create_pod(MeterWidget {
                reading: self.reading,
                size: self.size,
                style: self.style,
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.reading.value() != prev.reading.value()
            || std::mem::discriminant(&self.reading) != std::mem::discriminant(&prev.reading)
            || self.style != prev.style
        {
            element.widget.reading = self.reading;
            element.widget.style = self.style;
            element.ctx.request_render();
        }
        if self.size != prev.size {
            element.widget.size = self.size;
            element.ctx.request_layout();
        }
    }

    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_leaf(element);
    }

    fn message(
        &self,
        _: &mut (),
        _: &mut MessageContext,
        _: Mut<'_, Self::Element>,
        _: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::{core::PointerButton, testing::TestHarness};
    use xilem::Color;

    fn style() -> MeterStyle {
        MeterStyle {
            trough: Color::BLACK,
            normal: Color::from_rgb8(0, 255, 0),
            warning: Color::from_rgb8(255, 255, 0),
            near_ceiling: Color::from_rgb8(255, 128, 0),
            over: Color::from_rgb8(255, 0, 0),
            reference: Color::WHITE,
        }
    }

    #[test]
    fn meter_is_noninteractive_and_accessibility_values_are_finite() {
        for amplitude in [0.0, f32::NAN, -1.0, 1.0, 2.0, f32::INFINITY] {
            let widget = MeterWidget {
                reading: Reading::Dbfs(LevelMeter::new(amplitude)),
                size: Size::new(176.0, 24.0),
                style: style(),
            };
            let mut harness = TestHarness::create_with_size(
                masonry::theme::default_property_set(),
                widget.with_auto_id(),
                Size::new(176.0, 24.0),
            );
            let id = harness.root_id();
            assert_eq!(harness.root_widget().ctx().size(), Size::new(176.0, 24.0));
            let node = harness.access_node(id).unwrap();
            assert_eq!(node.role(), Role::Meter);
            assert!(node.numeric_value().unwrap().is_finite());
            assert_eq!(node.min_numeric_value(), Some(-60.0));
            assert_eq!(node.max_numeric_value(), Some(12.0));
            harness.press_tab_key(false);
            assert!(harness.focused_widget_id().is_none());
            harness.mouse_move((40.0, 12.0));
            harness.mouse_button_press(PointerButton::Primary);
            harness.mouse_button_release(PointerButton::Primary);
            assert!(harness.pop_action_erased().is_none());
        }
    }

    #[test]
    fn percentage_bars_keep_decimal_readouts_and_sanitize_invalid_values() {
        for (input, expected) in [
            (42.5, 42.5),
            (-1.0, 0.0),
            (120.0, 100.0),
            (f64::NAN, 0.0),
            (f64::INFINITY, 0.0),
        ] {
            let view = percentage_meter(input, 88.0, 24.0, style());
            let widget = MeterWidget {
                reading: view.reading,
                size: view.size,
                style: view.style,
            };
            let harness = TestHarness::create_with_size(
                masonry::theme::default_property_set(),
                widget.with_auto_id(),
                Size::new(88.0, 24.0),
            );
            let node = harness.access_node(harness.root_id()).unwrap();
            assert_eq!(node.role(), Role::ProgressIndicator);
            assert_eq!(node.numeric_value(), Some(expected));
            assert_eq!(node.value(), Some(format!("{expected:.1}%")));
        }
    }

    #[test]
    fn meter_paint_is_clipped_and_handles_display_scaling_and_overloads() {
        for scale in [1.0, 1.5, 2.0] {
            for amplitude in [0.0, 1.0, 2.0, f32::INFINITY, f32::NAN] {
                let mut scene = Scene::new();
                LevelMeter::new(amplitude).render(
                    &mut scene,
                    Size::new(176.0, 24.0).to_rect(),
                    scale,
                    &style(),
                );
                assert!(!scene.encoding().is_empty());
                assert!(!scene.encoding().path_data.is_empty());
            }
        }
        let mut scene = Scene::new();
        LevelMeter::new(1.0).render(
            &mut scene,
            Size::new(176.0, 24.0).to_rect(),
            f64::NAN,
            &style(),
        );
        assert!(scene.encoding().is_empty());
    }
}
