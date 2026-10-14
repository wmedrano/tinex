use super::{AppState, BoxView, UiAction, UiTrack, caption, theme, traced};
use tinex_plugins::PluginKind;
use tinex_widgets::FONT_STACK;
use xilem::WidgetView;
use xilem::masonry::properties::{LineBreaking, Padding, types::Length};
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, FlexExt, button, flex_col, flex_row, label, portal, sized_box,
};

pub(super) fn plugin_view(state: &AppState, track: &UiTrack, insertion_index: u16) -> BoxView {
    let (title, destination) = match track.plugins.get(usize::from(insertion_index)) {
        Some(plugin) => (
            "Insert Plugin",
            format!(
                "Before {:02} · {}",
                usize::from(insertion_index) + 1,
                plugin.name
            ),
        ),
        None => (
            "Add Plugin",
            format!(
                "At the end of the chain · position {:02}",
                track.plugins.len() + 1
            ),
        ),
    };
    let header = flex_col((
        flex_row((
            traced(
                button(label("Back to Track").font(FONT_STACK).text_size(12.0), {
                    let id = track.id;
                    move |_: &mut AppState| UiAction::OpenTrack(id)
                })
                .background_color(theme::BACKGROUND)
                .border_color(theme::BACKGROUND),
            ),
            sized_box(
                label(title)
                    .font(FONT_STACK)
                    .text_size(24.0)
                    .color(theme::TEXT),
            )
            .expand_width()
            .flex(1.0),
        ))
        .gap(Length::px(12.0)),
        label(format!("Track · {}", track.name))
            .font(FONT_STACK)
            .text_size(14.0)
            .color(theme::TEXT)
            .line_break_mode(LineBreaking::Clip),
        label(destination)
            .font(FONT_STACK)
            .text_size(12.0)
            .color(theme::MUTED)
            .line_break_mode(LineBreaking::Clip),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(Length::px(6.0))
    .padding(Padding::from_vh(12.0, 16.0));

    let sections = [
        (PluginKind::Instrument, "Instruments"),
        (PluginKind::Effect, "Effects"),
    ]
    .into_iter()
    .map(|(kind, name)| {
        // Group by catalog metadata without changing the original factory index
        // carried by AddPlugin. View construction never builds a plugin.
        let choices = tinex_plugins::FACTORY
            .iter()
            .enumerate()
            .filter(|(_, builder)| builder.kind() == kind)
            .map(|(index, builder)| {
                flex_row((
                    sized_box(traced(
                        button(
                            label(builder.name())
                                .font(FONT_STACK)
                                .text_size(16.0)
                                .line_break_mode(LineBreaking::Clip),
                            {
                                let track_id = track.id;
                                move |_: &mut AppState| UiAction::AddPlugin { track_id, index }
                            },
                        )
                        .disabled(!state.connected || track.deleting)
                        .background_color(theme::BACKGROUND)
                        .border_color(theme::SURFACE)
                        .padding(Padding::from_vh(4.0, 8.0)),
                    ))
                    .width(Length::px(132.0))
                    .height(Length::px(32.0)),
                    sized_box(
                        label(builder.description())
                            .font(FONT_STACK)
                            .text_size(12.0)
                            .color(theme::MUTED)
                            .line_break_mode(LineBreaking::WordWrap),
                    )
                    .expand_width()
                    .flex(1.0),
                ))
                .gap(Length::px(12.0))
                .padding(10.0)
                .background_color(theme::SURFACE)
                .corner_radius(8.0)
                .boxed()
            })
            .collect::<Vec<_>>();
        flex_col((
            label(format!("{name} · {}", choices.len()))
                .font(FONT_STACK)
                .text_size(20.0)
                .color(theme::TEXT),
            choices,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::px(8.0))
        .boxed()
    })
    .collect::<Vec<_>>();
    let guidance = if !state.connected {
        "Audio engine disconnected. Adding plugins is unavailable."
    } else if track.deleting {
        "This track is being deleted. Adding plugins is unavailable."
    } else {
        "Choose a plugin to add it to this track."
    };
    let content = flex_col((
        caption(
            guidance,
            if state.connected {
                theme::MUTED
            } else {
                theme::ERROR
            },
        ),
        sections,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(Length::px(16.0))
    .padding(Padding::from_vh(8.0, 16.0));

    flex_col((header, portal(content).flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::ZERO)
        .boxed()
}
