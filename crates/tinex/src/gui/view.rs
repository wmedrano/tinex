//! Pure Xilem page construction. Semantic actions are dispatched above this tree.
use tinex_widgets::{
    FONT_STACK, TransportGlyph, level_meter, percentage_meter, sanitize_level, timeline,
    transport_icon,
};
use xilem::masonry::properties::{LineBreaking, Padding, types::Length};
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, Flex, FlexExt, FlexSequence, button as xilem_button, flex_col, flex_row,
    label, sized_box, slider, zstack,
};
use xilem::{AnyWidgetView, WidgetView};

use super::{
    action::UiAction,
    state::{AppState, Page, UiTrack},
    theme,
    update::{MAX_BPM, MIN_BPM},
};
mod main;
mod plugins;
mod track;

type BoxView = Box<AnyWidgetView<AppState, UiAction>>;

const TIMELINE_HEIGHT: f64 = 28.0;
const TRANSPORT_BUTTON_WIDTH: f64 = 32.0;

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
        // Captions stay close to their controls; groups get wider separation.
        flex_row((
            label(status)
                .font(FONT_STACK)
                .text_size(12.0)
                .color(color)
                .line_break_mode(LineBreaking::Clip)
                .flex(0.5),
            footer_group((
                footer_caption("BPM"),
                // Dragging covers large tempo jumps; arrow keys step by exactly 1 BPM.
                sized_box(traced(
                    slider(
                        f64::from(MIN_BPM),
                        f64::from(MAX_BPM),
                        f64::from(state.bpm),
                        |_: &mut AppState, bpm| UiAction::SetBpm(bpm as f32),
                    )
                    .step(1.0)
                    .disabled(!state.connected),
                ))
                // Masonry sliders are never narrower than 100px, so a flex share
                // would overflow the footer at the minimum window width.
                .width(Length::px(100.0)),
                // Fixed width keeps the meters from shifting as the digit count changes.
                sized_box(
                    label(format!("{:.0}", state.bpm))
                        .font(FONT_STACK)
                        .text_size(12.0)
                        .color(theme::TEXT),
                )
                .width(Length::px(24.0)),
            )),
            footer_group((
                footer_caption(format!(
                    "{}/{}",
                    state.time_signature.numerator, state.time_signature.denominator
                )),
                // Fixed width keeps the meters from shifting as the bar number grows.
                sized_box(
                    label(position(state.beat, state.time_signature))
                        .font(FONT_STACK)
                        .text_size(12.0)
                        .color(theme::TEXT),
                )
                .width(Length::px(40.0)),
            )),
            footer_group((
                footer_caption("Master"),
                level_meter(state.master_output_level, 176.0, 24.0, theme::meter_style()).flex(1.0),
            ))
            .flex(1.0),
            footer_group((
                footer_caption("CPU"),
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
                .flex(1.0),
            ))
            .flex(0.8),
        ))
        .gap(Length::px(12.0))
        .padding(Padding::from_vh(0.0, 8.0))
        .background_color(theme::SURFACE)
        .corner_radius(0.0),
    )
    .height(Length::px(32.0));
    flex_col((page.flex(1.0), timeline_row(state), footer))
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(Length::ZERO)
        .background_color(theme::BACKGROUND)
}

/// Transport buttons beside a ruler of the arrangement's bars with a moving playhead.
fn timeline_row(state: &AppState) -> impl WidgetView<AppState, UiAction> + use<> {
    let play = if state.playing {
        (TransportGlyph::Pause, UiAction::TogglePlaying)
    } else {
        (TransportGlyph::Play, UiAction::TogglePlaying)
    };
    let buttons = [play, (TransportGlyph::Stop, UiAction::Stop)].map(|(glyph, action)| {
        // Only play/pause lights up; stop is an action, not a state.
        let lit = state.playing && action == UiAction::TogglePlaying;
        let icon_color = if lit {
            theme::BACKGROUND
        } else if state.connected {
            theme::TEXT
        } else {
            theme::MUTED
        };
        sized_box(traced(
            xilem_button(
                transport_icon(glyph, 10.0, icon_color),
                move |_: &mut AppState| action,
            )
            .disabled(!state.connected)
            .background_color(if lit {
                theme::PLAYING
            } else {
                xilem::Color::from_rgb8(68, 71, 90)
            })
            .active_background_color(if lit {
                theme::PLAYING_ACTIVE
            } else {
                xilem::Color::from_rgb8(98, 85, 130)
            })
            // The default 16px side padding would leave no room for the icon.
            .padding(0.0)
            .corner_radius(0.0),
        ))
        .width(Length::px(TRANSPORT_BUTTON_WIDTH))
        .height(Length::px(TIMELINE_HEIGHT))
    });
    // One equal-width cell per bar lines each number up with its bar line.
    let numbers = (1..=state.measures)
        .map(|bar| {
            sized_box(
                label(bar.to_string())
                    .font(FONT_STACK)
                    .text_size(11.0)
                    .color(theme::MUTED)
                    .line_break_mode(LineBreaking::Clip),
            )
            .expand_width()
            .padding(Padding::from_vh(1.0, 4.0))
            .flex(1.0)
        })
        .collect::<Vec<_>>();
    let ruler = zstack((
        timeline(
            state.beat,
            u32::from(state.time_signature.numerator),
            state.measures,
            TIMELINE_HEIGHT,
            theme::timeline_style(),
        ),
        flex_row(numbers).gap(Length::ZERO),
    ));
    sized_box(flex_row((buttons, ruler.flex(1.0))).gap(Length::ZERO))
        .height(Length::px(TIMELINE_HEIGHT))
}

/// Formats a beat count as a one-based `bar.beat` position.
fn position(beat: f64, time_signature: tinex_core::TimeSignature) -> String {
    let beats_per_bar = u64::from(time_signature.numerator.max(1));
    let beat = beat.max(0.0) as u64;
    format!("{}.{}", beat / beats_per_bar + 1, beat % beats_per_bar + 1)
}

fn footer_caption<T: Into<String>>(text: T) -> impl WidgetView<AppState, UiAction> + use<T> {
    label(text.into())
        .font(FONT_STACK)
        .text_size(12.0)
        .color(theme::MUTED)
}

fn footer_group<Seq: FlexSequence<AppState, UiAction>>(
    children: Seq,
) -> Flex<Seq, AppState, UiAction> {
    flex_row(children).gap(Length::px(6.0))
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
