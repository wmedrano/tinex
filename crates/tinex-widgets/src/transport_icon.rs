//! Painted play, pause, and stop symbols, which the bundled fonts lack.
use xilem::Color;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NoAction, PaintCtx, PropertiesRef, Widget,
};
use xilem::masonry::kurbo::{Affine, BezPath, Rect, Shape, Size};
use xilem::masonry::peniko::Fill;
use xilem::masonry::vello::Scene;
use xilem::{Pod, ViewCtx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportGlyph {
    Play,
    Pause,
    Stop,
}

impl TransportGlyph {
    /// Accessible name, also used as the debug text.
    pub fn name(self) -> &'static str {
        match self {
            TransportGlyph::Play => "Play",
            TransportGlyph::Pause => "Pause",
            TransportGlyph::Stop => "Stop",
        }
    }
}

/// Create a square transport symbol `size` logical pixels wide.
pub fn transport_icon(glyph: TransportGlyph, size: f64, color: Color) -> TransportIcon {
    TransportIcon { glyph, size, color }
}

/// Xilem view for a [`TransportIconWidget`].
pub struct TransportIcon {
    glyph: TransportGlyph,
    size: f64,
    color: Color,
}

/// Masonry leaf widget. Interaction belongs to the button that contains it.
pub struct TransportIconWidget {
    glyph: TransportGlyph,
    size: f64,
    color: Color,
}

impl Widget for TransportIconWidget {
    type Action = NoAction;

    fn register_children(&mut self, _: &mut xilem::masonry::core::RegisterCtx<'_>) {}

    fn layout(
        &mut self,
        _: &mut LayoutCtx<'_>,
        _: &mut xilem::masonry::core::PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        bc.constrain(Size::new(self.size, self.size))
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, scene: &mut Scene) {
        let Size { width, height } = ctx.size();
        let side = width.min(height);
        let (x, y) = ((width - side) / 2.0, (height - side) / 2.0);
        let path = match self.glyph {
            TransportGlyph::Play => {
                // Nudged right so the triangle's visual center sits in the middle.
                let mut path = BezPath::new();
                path.move_to((x + side * 0.2, y));
                path.line_to((x + side, y + side / 2.0));
                path.line_to((x + side * 0.2, y + side));
                path.close_path();
                path
            }
            TransportGlyph::Pause => {
                let bar = side * 0.35;
                let mut path =
                    Rect::new(x + side * 0.1, y, x + side * 0.1 + bar, y + side).to_path(0.1);
                path.extend(
                    Rect::new(x + side * 0.9 - bar, y, x + side * 0.9, y + side).to_path(0.1),
                );
                path
            }
            TransportGlyph::Stop => {
                let inset = side * 0.1;
                Rect::new(x + inset, y + inset, x + side - inset, y + side - inset).to_path(0.1)
            }
        };
        scene.fill(Fill::NonZero, Affine::IDENTITY, self.color, None, &path);
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.glyph.name());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.glyph.name().into())
    }
}

impl TransportIconWidget {
    pub fn glyph(&self) -> TransportGlyph {
        self.glyph
    }
}

impl ViewMarker for TransportIcon {}
impl<State, Action> View<State, Action, ViewCtx> for TransportIcon {
    type Element = Pod<TransportIconWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (
            ctx.create_pod(TransportIconWidget {
                glyph: self.glyph,
                size: self.size,
                color: self.color,
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
        if self.glyph != prev.glyph || self.color != prev.color {
            element.widget.glyph = self.glyph;
            element.widget.color = self.color;
            element.ctx.request_render();
            element.ctx.request_accessibility_update();
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
    use masonry::testing::TestHarness;

    #[test]
    fn keeps_its_size_and_names_the_glyph_accessibly() {
        for glyph in [
            TransportGlyph::Play,
            TransportGlyph::Pause,
            TransportGlyph::Stop,
        ] {
            let harness = TestHarness::create_with_size(
                masonry::theme::default_property_set(),
                TransportIconWidget {
                    glyph,
                    size: 12.0,
                    color: Color::WHITE,
                }
                .with_auto_id(),
                Size::new(12.0, 12.0),
            );
            let id = harness.root_id();
            assert_eq!(harness.root_widget().ctx().size(), Size::new(12.0, 12.0));
            let node = harness.access_node(id).unwrap();
            assert_eq!(node.role(), Role::Image);
            assert_eq!(node.label().as_deref(), Some(glyph.name()));
        }
    }
}
