use super::{AppState, BoxView, UiAction, UiTrack, caption, theme, traced};
use tinex_widgets::{FONT_STACK, keyed};
use xilem::WidgetView;
use xilem::masonry::properties::{LineBreaking, Padding, types::Length};
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, FlexExt, button, flex_col, flex_row, label, portal, sized_box,
};

pub(super) fn track_view(state: &AppState, track: &UiTrack) -> BoxView {
    let header = flex_row((
        traced(
            button(
                label("Back to Tracks").font(FONT_STACK).text_size(12.0),
                |_: &mut AppState| UiAction::BackToMain,
            )
            .background_color(theme::BACKGROUND)
            .border_color(theme::BACKGROUND),
        ),
        sized_box(
            label(track.name.clone())
                .font(FONT_STACK)
                .text_size(24.0)
                .color(theme::TEXT)
                .line_break_mode(LineBreaking::Clip),
        )
        .expand_width()
        .flex(1.0),
    ))
    .gap(Length::px(12.0))
    .padding(Padding::from_vh(12.0, 16.0));

    // Track controls stay visible while the confirmed plugin chain scrolls.
    let controls = flex_row((
        flex_col((
            label("MIDI input")
                .font(FONT_STACK)
                .text_size(12.0)
                .color(theme::MUTED),
            super::track_arm_control(state, track),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::px(6.0)),
        sized_box(
            flex_col((
                label("Volume / output")
                    .font(FONT_STACK)
                    .text_size(12.0)
                    .color(theme::MUTED),
                super::track_volume_control(state, track, 240.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .gap(Length::px(6.0)),
        )
        .expand_width()
        .flex(1.0),
    ))
    .gap(Length::px(12.0))
    .padding(8.0)
    .background_color(theme::SURFACE)
    .corner_radius(8.0);

    let chain_header = flex_row((
        sized_box(
            label(format!("Plugin chain · {}", track.plugins.len()))
                .font(FONT_STACK)
                .text_size(20.0)
                .color(theme::TEXT)
                .line_break_mode(LineBreaking::Clip),
        )
        .expand_width()
        .flex(1.0),
        traced(
            button(label("Add Plugin").font(FONT_STACK).text_size(16.0), {
                let track_id = track.id;
                move |_: &mut AppState| UiAction::OpenPlugins {
                    track_id,
                    index: u16::MAX,
                }
            })
            .disabled(!state.connected || track.deleting)
            .background_color(xilem::Color::from_rgb8(88, 71, 122)),
        ),
    ))
    .gap(Length::px(12.0))
    .padding(Padding::from_vh(8.0, 16.0));

    let plugins = keyed(track.plugins.iter().enumerate().map(|(index, plugin)| {
        let row = flex_row((
            sized_box(
                label(format!("{:02}", index + 1))
                    .font(FONT_STACK)
                    .text_size(12.0)
                    .color(theme::MUTED),
            )
            .width(Length::px(24.0)),
            sized_box(
                label(plugin.name)
                    .font(FONT_STACK)
                    .text_size(16.0)
                    .color(theme::TEXT)
                    .line_break_mode(LineBreaking::Clip),
            )
            .expand_width()
            .flex(1.0),
            chain_button(
                "Insert Before",
                state.connected && !track.deleting && u16::try_from(index).is_ok(),
                UiAction::OpenPlugins {
                    track_id: track.id,
                    index: u16::try_from(index).unwrap_or(u16::MAX),
                },
                100.0,
                theme::BACKGROUND,
            ),
            chain_button(
                if plugin.deleting {
                    "Deleting…"
                } else {
                    "Delete"
                },
                state.connected && !track.deleting && !plugin.deleting,
                UiAction::DeletePlugin {
                    track_id: track.id,
                    plugin_id: plugin.id,
                },
                80.0,
                theme::SURFACE,
            ),
        ))
        .gap(Length::px(8.0))
        .padding(10.0)
        .background_color(theme::SURFACE)
        .corner_radius(8.0)
        .boxed();
        (plugin.id, row)
    }));

    let content = flex_col((
        (!state.connected).then(|| {
            caption(
                "Audio engine disconnected. Track controls are unavailable.",
                theme::ERROR,
            )
        }),
        (state.connected && track.deleting).then(|| caption("Deleting track…", theme::MUTED)),
        track.plugins.is_empty().then(|| {
            flex_col((
                label("No plugins yet")
                    .font(FONT_STACK)
                    .text_size(20.0)
                    .color(theme::TEXT),
                caption(
                    "Add an instrument or effect to start building this track.",
                    theme::MUTED,
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .gap(Length::px(8.0))
            .padding(20.0)
            .background_color(theme::SURFACE)
            .corner_radius(8.0)
        }),
        plugins,
        (!track.pending_plugins.is_empty()).then(|| caption("Adding plugin…", theme::MUTED)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(Length::px(8.0))
    .padding(Padding::from_vh(0.0, 16.0));

    flex_col((
        header,
        sized_box(controls)
            .expand_width()
            .padding(Padding::from_vh(0.0, 16.0)),
        chain_header,
        portal(content).flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(Length::ZERO)
    .boxed()
}

fn chain_button(
    text: &str,
    enabled: bool,
    action: UiAction,
    width: f64,
    background: xilem::Color,
) -> BoxView {
    // Reserve deletion feedback space without shifting names or action columns.
    sized_box(traced(
        button(
            label(text.to_owned())
                .font(FONT_STACK)
                .text_size(12.0)
                .color(theme::MUTED)
                .line_break_mode(LineBreaking::Clip),
            move |_: &mut AppState| action,
        )
        .disabled(!enabled)
        .background_color(background)
        .border_color(theme::SURFACE)
        .padding(Padding::from_vh(4.0, 8.0)),
    ))
    .width(Length::px(width))
    .height(Length::px(32.0))
    .boxed()
}
