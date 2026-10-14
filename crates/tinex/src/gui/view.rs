//! Pure Xilem page construction. Semantic actions are dispatched above this tree.
use tinex_widgets::{FONT_STACK, level_meter, percentage_meter, sanitize_level};
use xilem::masonry::properties::{LineBreaking, Padding, types::Length};
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, FlexExt, button as xilem_button, flex_col, flex_row, label, sized_box,
    slider, zstack,
};
use xilem::{AnyWidgetView, WidgetView};

use super::{
    action::UiAction,
    state::{AppState, Page, UiTrack},
    theme,
};
mod main;
mod plugins;
mod track;

type BoxView = Box<AnyWidgetView<AppState, UiAction>>;

pub(super) fn view(state: &AppState) -> impl WidgetView<AppState, UiAction> + use<> {
    let page = match state.page {
        Page::Main => main::main_view(state),
        Page::Track(id) => state
            .tracks
            .iter()
            .find(|track| track.id == id)
            .map_or_else(
                || main::main_view(state),
                |row| track::track_view(state, row),
            ),
        Page::Plugins(id, index) => state
            .tracks
            .iter()
            .find(|track| track.id == id)
            .map_or_else(
                || main::main_view(state),
                |row| plugins::plugin_view(state, row, index),
            ),
    };
    let (status, color) = status(state);
    // Flex children may shrink at the minimum window size; the footer itself
    // never enters the Portal, and its square background fills the whole width.
    let footer = sized_box(
        flex_row((
            label(status)
                .font(FONT_STACK)
                .text_size(12.0)
                .color(color)
                .line_break_mode(LineBreaking::Clip)
                .flex(1.0),
            label("Master")
                .font(FONT_STACK)
                .text_size(12.0)
                .color(theme::MUTED),
            level_meter(state.master_output_level, 176.0, 24.0, theme::meter_style()).flex(1.0),
            label("CPU")
                .font(FONT_STACK)
                .text_size(12.0)
                .color(theme::MUTED),
            zstack((
                percentage_meter(
                    f64::from(state.cpu_load.unwrap_or(0.0)),
                    88.0,
                    24.0,
                    theme::meter_style(),
                ),
                label(format!("{:.1}%", state.cpu_load.unwrap_or(0.0)))
                    .font(FONT_STACK)
                    .text_size(12.0)
                    .color(theme::TEXT),
            ))
            .flex(0.6),
        ))
        .gap(Length::px(6.0))
        .padding(Padding::from_vh(0.0, 8.0))
        .background_color(theme::SURFACE)
        .corner_radius(0.0),
    )
    .height(Length::px(32.0));
    flex_col((page.flex(1.0), footer))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::ZERO)
        .background_color(theme::BACKGROUND)
}

fn caption(text: impl Into<String>, color: xilem::Color) -> BoxView {
    label(text.into())
        .font(FONT_STACK)
        .text_size(16.0)
        .color(color)
        .line_break_mode(LineBreaking::WordWrap)
        .boxed()
}

/// Metering is pre-gain; display the estimated post-gain level.
fn track_volume_control(state: &AppState, track: &UiTrack, width: f64) -> BoxView {
    let id = track.id;
    let gain = if track.volume_db == -61.0 {
        0.0
    } else {
        10.0_f32.powf(track.volume_db / 20.0)
    };
    let level = if gain == 0.0 {
        0.0
    } else {
        sanitize_level(track.output_level) * gain
    };
    let readout = if track.volume_db == -61.0 {
        "−∞ dB".into()
    } else {
        format!("{:+.0} dB", track.volume_db)
    };
    sized_box(
        flex_col((
            flex_row((
                traced(
                    slider(
                        -61.0,
                        12.0,
                        f64::from(track.volume_db),
                        move |_: &mut AppState, db| UiAction::SetTrackVolume {
                            track_id: id,
                            db: db as f32,
                        },
                    )
                    .step(1.0)
                    .disabled(!state.connected || track.deleting),
                )
                .flex(1.0),
                label(readout)
                    .font(FONT_STACK)
                    .text_size(12.0)
                    .color(theme::TEXT),
            )),
            level_meter(level, width, 24.0, theme::meter_style()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::ZERO),
    )
    .width(Length::px(width))
    .boxed()
}

const TRACK_ARM_WIDTH: f64 = 96.0;

fn track_arm_control(state: &AppState, track: &UiTrack) -> BoxView {
    let id = track.id;
    // Reserve space for the armed indicator so confirmation never shifts layout.
    sized_box(traced(
        xilem_button(
            label(if track.armed { "Arm ●" } else { "Arm" })
                .font(FONT_STACK)
                .text_size(16.0)
                .line_break_mode(LineBreaking::Clip),
            move |_: &mut AppState| UiAction::ToggleTrackArm(id),
        )
        .disabled(!state.connected || track.deleting || state.arm_request_in_flight)
        .background_color(if track.armed {
            theme::SELECTED
        } else {
            xilem::Color::from_rgb8(68, 71, 90)
        })
        .active_background_color(if track.armed {
            xilem::Color::from_rgb8(120, 35, 48)
        } else {
            xilem::Color::from_rgb8(98, 85, 130)
        })
        .disabled_background_color(if track.armed {
            xilem::Color::from_rgb8(90, 45, 52)
        } else {
            xilem::Color::from_rgb8(55, 57, 72)
        }),
    ))
    .width(Length::px(TRACK_ARM_WIDTH))
    .height(Length::px(32.0))
    .boxed()
}

fn status(state: &AppState) -> (String, xilem::Color) {
    let mut activity = Vec::new();
    for (verb, count) in [
        ("Creating", state.pending.len()),
        (
            "Deleting",
            state.tracks.iter().filter(|track| track.deleting).count(),
        ),
    ] {
        match count {
            0 => {}
            1 => activity.push(format!("{verb} Track…")),
            _ => activity.push(format!("{verb} {count} Tracks…")),
        }
    }
    let activity = activity.join(" · ");
    if let Some(message) = &state.message {
        (
            if activity.is_empty() {
                message.clone()
            } else {
                format!("{message} · {activity}")
            },
            theme::ERROR,
        )
    } else if activity.is_empty() {
        ("Ready".into(), theme::MUTED)
    } else {
        (activity, theme::MUTED)
    }
}

#[cfg(not(test))]
fn traced<V: WidgetView<AppState, UiAction>>(view: V) -> V {
    view
}
#[cfg(test)]
fn traced<V: WidgetView<AppState, UiAction>>(view: V) -> testing::Trace<V> {
    testing::Trace(view)
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
