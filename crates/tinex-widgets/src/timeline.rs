//! A noninteractive arrangement ruler with bar lines, beat ticks, and a playhead.
use xilem::Color;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NoAction, PaintCtx, PropertiesRef, Widget,
};
use xilem::masonry::kurbo::{Affine, Line, Rect, Size, Stroke};
use xilem::masonry::peniko::Fill;
use xilem::masonry::vello::Scene;
use xilem::{Pod, ViewCtx};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineStyle {
    pub trough: Color,
    /// Fill behind the playhead, showing how much of the arrangement has played.
    pub elapsed: Color,
    pub bar_line: Color,
    pub beat_tick: Color,
    pub playhead: Color,
}

/// The ruler's reading: a position within `bars` bars of `beats_per_bar` beats.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Position {
    beat: f64,
    beats_per_bar: u32,
    bars: u32,
}

impl Position {
    fn total_beats(self) -> f64 {
        f64::from(self.beats_per_bar) * f64::from(self.bars)
    }

    /// Fraction of the arrangement that has played, in `0.0..=1.0`.
    fn progress(self) -> f64 {
        let total = self.total_beats();
        if total > 0.0 {
            (self.beat / total).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn text(self) -> String {
        let beats_per_bar = u64::from(self.beats_per_bar.max(1));
        let beat = self.beat as u64;
        format!(
            "Bar {} of {}, beat {}",
            beat / beats_per_bar + 1,
            self.bars,
            beat % beats_per_bar + 1
        )
    }
}

/// Create a timeline that fills the available width at a fixed logical height.
/// Non-finite or negative beats are shown at the start of the arrangement.
pub fn timeline(
    beat: f64,
    beats_per_bar: u32,
    bars: u32,
    height: f64,
    style: TimelineStyle,
) -> Timeline {
    Timeline {
        position: Position {
            beat: if beat.is_finite() { beat.max(0.0) } else { 0.0 },
            beats_per_bar,
            bars,
        },
        height,
        style,
    }
}

/// Xilem view for a [`TimelineWidget`].
pub struct Timeline {
    position: Position,
    height: f64,
    style: TimelineStyle,
}

/// Masonry leaf widget. It never polls the engine or participates in focus.
pub struct TimelineWidget {
    position: Position,
    height: f64,
    style: TimelineStyle,
}

impl Widget for TimelineWidget {
    type Action = NoAction;

    fn register_children(&mut self, _: &mut xilem::masonry::core::RegisterCtx<'_>) {}

    fn layout(
        &mut self,
        _: &mut LayoutCtx<'_>,
        _: &mut xilem::masonry::core::PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let width = if bc.is_width_bounded() {
            bc.max().width
        } else {
            bc.min().width
        };
        bc.constrain(Size::new(width, self.height))
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, scene: &mut Scene) {
        let rect = ctx.size().to_rect();
        let style = &self.style;
        scene.fill(Fill::NonZero, Affine::IDENTITY, style.trough, None, &rect);
        let playhead = rect.width() * self.position.progress();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            style.elapsed,
            None,
            &Rect::new(0.0, 0.0, playhead, rect.height()),
        );
        let total = self.position.total_beats();
        if total > 0.0 {
            let thin = Stroke::new(1.0);
            let beats_per_bar = self.position.beats_per_bar;
            for beat in 0..beats_per_bar * self.position.bars {
                // Lines sit on half-pixels so 1px strokes stay crisp at 1x.
                let x = (rect.width() * f64::from(beat) / total).floor() + 0.5;
                if beat % beats_per_bar == 0 {
                    let line = Line::new((x, 0.0), (x, rect.height()));
                    scene.stroke(&thin, Affine::IDENTITY, style.bar_line, None, &line);
                } else {
                    let top = rect.height() * 0.7;
                    let line = Line::new((x, top), (x, rect.height()));
                    scene.stroke(&thin, Affine::IDENTITY, style.beat_tick, None, &line);
                }
            }
        }
        let line = Line::new((playhead, 0.0), (playhead, rect.height()));
        scene.stroke(
            &Stroke::new(2.0),
            Affine::IDENTITY,
            style.playhead,
            None,
            &line,
        );
    }

    fn accessibility_role(&self) -> Role {
        Role::ProgressIndicator
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label("Playback position");
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(self.position.total_beats());
        node.set_numeric_value(self.position.beat.min(self.position.total_beats()));
        node.set_value(self.position.text());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.position.text())
    }
}

impl ViewMarker for Timeline {}
impl<State, Action> View<State, Action, ViewCtx> for Timeline {
    type Element = Pod<TimelineWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (
            ctx.create_pod(TimelineWidget {
                position: self.position,
                height: self.height,
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
        if self.position != prev.position || self.style != prev.style {
            element.widget.position = self.position;
            element.widget.style = self.style;
            element.ctx.request_render();
        }
        if self.height != prev.height {
            element.widget.height = self.height;
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

    fn style() -> TimelineStyle {
        TimelineStyle {
            trough: Color::BLACK,
            elapsed: Color::from_rgb8(32, 32, 32),
            bar_line: Color::WHITE,
            beat_tick: Color::from_rgb8(128, 128, 128),
            playhead: Color::from_rgb8(255, 0, 0),
        }
    }

    fn widget(beat: f64, beats_per_bar: u32, bars: u32) -> TimelineWidget {
        let Timeline {
            position,
            height,
            style,
        } = timeline(beat, beats_per_bar, bars, 28.0, style());
        TimelineWidget {
            position,
            height,
            style,
        }
    }

    #[test]
    fn fills_available_width_and_reports_the_position_accessibly() {
        let mut harness = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            widget(6.5, 4, 4).with_auto_id(),
            Size::new(320.0, 28.0),
        );
        let id = harness.root_id();
        assert_eq!(harness.root_widget().ctx().size(), Size::new(320.0, 28.0));
        let node = harness.access_node(id).unwrap();
        assert_eq!(node.role(), Role::ProgressIndicator);
        assert_eq!(node.numeric_value(), Some(6.5));
        assert_eq!(node.max_numeric_value(), Some(16.0));
        assert_eq!(node.value().as_deref(), Some("Bar 2 of 4, beat 3"));
        harness.press_tab_key(false);
        assert!(harness.focused_widget_id().is_none());
        harness.mouse_move((40.0, 12.0));
        harness.mouse_button_press(PointerButton::Primary);
        harness.mouse_button_release(PointerButton::Primary);
        assert!(harness.pop_action_erased().is_none());
    }

    #[test]
    fn progress_is_clamped_and_empty_arrangements_stay_at_the_start() {
        assert_eq!(widget(4.0, 4, 2).position.progress(), 0.5);
        assert_eq!(widget(20.0, 4, 2).position.progress(), 1.0);
        for beat in [f64::NAN, f64::INFINITY, -3.0] {
            assert_eq!(widget(beat, 4, 2).position.beat, 0.0);
        }
        assert_eq!(widget(3.0, 4, 0).position.progress(), 0.0);
        assert_eq!(widget(3.0, 0, 4).position.progress(), 0.0);
    }
}
