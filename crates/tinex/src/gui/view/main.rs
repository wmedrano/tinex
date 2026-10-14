use super::{AppState, BoxView, UiAction, UiTrack, caption, theme, traced};
use tinex_widgets::{FONT_STACK, keyed};
use xilem::WidgetView;
use xilem::masonry::properties::{LineBreaking, Padding, types::Length};
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, FlexExt, button, flex_col, flex_row, label, portal, sized_box,
};

pub(super) fn main_view(state: &AppState) -> BoxView {
    let rows = keyed(state.tracks.iter().map(|track| {
        // Navigation is explicit: Arm, Delete, and volume never open a track.
        let details = flex_col((
            flex_row(
                traced(
                    button(
                        label(track.name.clone())
                            .font(FONT_STACK)
                            .text_size(18.0)
                            .line_break_mode(LineBreaking::Clip),
                        {
                            let id = track.id;
                            move |_: &mut AppState| UiAction::OpenTrack(id)
                        },
                    )
                    .background_color(theme::BACKGROUND)
                    .border_color(theme::SURFACE)
                    .padding(Padding::from_vh(4.0, 8.0)),
                )
                .flex(1.0),
            ),
            label(plugin_summary(track))
                .font(FONT_STACK)
                .text_size(12.0)
                .color(theme::MUTED)
                .line_break_mode(LineBreaking::Clip),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::px(6.0));
        let actions = sized_box(
            flex_col((
                super::track_arm_control(state, track),
                traced(
                    button(
                        label(if track.deleting {
                            "Deleting…"
                        } else {
                            "Delete"
                        })
                        .font(FONT_STACK)
                        .text_size(12.0)
                        .color(theme::MUTED),
                        {
                            let id = track.id;
                            move |_: &mut AppState| UiAction::DeleteTrack(id)
                        },
                    )
                    .disabled(!state.connected || track.deleting)
                    .background_color(theme::SURFACE)
                    .border_color(theme::SURFACE),
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .gap(Length::px(6.0)),
        )
        .width(Length::px(super::TRACK_ARM_WIDTH));
        let output = flex_col((
            label("Volume / output")
                .font(FONT_STACK)
                .text_size(12.0)
                .color(theme::MUTED),
            super::track_volume_control(state, track, 144.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::px(6.0));
        let row = flex_row((sized_box(details).expand_width().flex(1.0), actions, output))
            .gap(Length::px(12.0))
            .padding(12.0)
            .background_color(theme::SURFACE)
            .corner_radius(8.0)
            .boxed();
        (track.id, row)
    }));
    let content = flex_col((
        state.tracks.is_empty().then(|| {
            flex_col((
                label(if state.pending.is_empty() {
                    "Your session starts here"
                } else {
                    "Creating your first track…"
                })
                .font(FONT_STACK)
                .text_size(22.0)
                .color(theme::TEXT)
                .line_break_mode(LineBreaking::WordWrap),
                caption(
                    "Create a track, then open it to add an instrument or effect.\nArm tracks to play them from MIDI input.",
                    theme::MUTED,
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .gap(Length::px(12.0))
            .padding(24.0)
            .background_color(theme::SURFACE)
            .corner_radius(8.0)
        }),
        rows,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(Length::px(8.0))
    .padding(Padding::from_vh(8.0, 16.0));

    let description = if !state.connected {
        "Audio engine disconnected. Track settings remain available.".into()
    } else if !state.pending.is_empty() {
        match state.pending.len() {
            1 => "Creating 1 track…".into(),
            count => format!("Creating {count} tracks…"),
        }
    } else {
        "Open a track to edit its instruments and effects.".into()
    };
    let header = flex_col((
        flex_row((
            sized_box(
                label(format!("Tracks · {}", state.tracks.len()))
                    .font(FONT_STACK)
                    .text_size(24.0)
                    .color(theme::TEXT),
            )
            .expand_width()
            .flex(1.0),
            traced(
                button(
                    label("Create Track").font(FONT_STACK).text_size(16.0),
                    |_: &mut AppState| UiAction::CreateTrack,
                )
                .disabled(!state.connected)
                .background_color(xilem::Color::from_rgb8(88, 71, 122))
                .padding(Padding::from_vh(8.0, 12.0)),
            ),
        )),
        label(description)
            .font(FONT_STACK)
            .text_size(12.0)
            .color(if state.connected {
                theme::MUTED
            } else {
                theme::ERROR
            })
            .line_break_mode(LineBreaking::WordWrap),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(Length::px(6.0))
    .padding(Padding::from_vh(12.0, 16.0));

    flex_col((header, portal(content).flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::ZERO)
        .boxed()
}

fn plugin_summary(track: &UiTrack) -> String {
    let mut summary = if track.plugins.is_empty() {
        "No plugins".into()
    } else {
        track
            .plugins
            .iter()
            .map(|plugin| plugin.name)
            .collect::<Vec<_>>()
            .join(" · ")
    };
    if !track.pending_plugins.is_empty() {
        summary.push_str(" · Adding plugin…");
    }
    summary
}
