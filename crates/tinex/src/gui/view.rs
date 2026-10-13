//! Page routing and the shared, fixed status footer.
use std::sync::Arc;
use tinex_widgets::{LevelMeter, Slider, Tone, Widget};

use super::action::UiAction;
use super::state::{AppState, Page, UiTrack};

mod main;
mod plugins;
mod track;

pub(super) fn view(state: &AppState) -> Widget<UiAction> {
    let mut children = match state.page {
        Page::Main => main::main_view(state),
        Page::Track(id) => state
            .tracks
            .iter()
            .find(|track| track.id == id)
            .map_or_else(
                || main::main_view(state),
                |row| track::track_view(state, row),
            ),
        Page::Plugins(id) => state
            .tracks
            .iter()
            .find(|track| track.id == id)
            .map_or_else(
                || main::main_view(state),
                |row| plugins::plugin_view(state, row),
            ),
    };
    let (status, tone) = status(state);
    // The scroll region fills the remaining height on every page; this row
    // stays fixed at the bottom regardless of content or loading/error messages.
    children.push(Widget::StatusBar(vec![
        Widget::label(status, tone),
        Widget::fixed_label("Master", Tone::Muted, 56.0),
        Widget::compact_level_meter(state.master_output_level, 176.0),
        Widget::fixed_label("CPU", Tone::Muted, 32.0),
        Widget::percentage_meter(state.cpu_load.map_or(0.0, f64::from), 88.0),
    ]));
    Widget::Column(children)
}

/// Metering is reported before track gain; present the estimated post-gain level.
fn track_volume_control(state: &AppState, track: &UiTrack, width: f64) -> Widget<UiAction> {
    let track_id = track.id;
    let gain = if track.volume_db == -61.0 {
        0.0
    } else {
        10.0_f32.powf(track.volume_db / 20.0)
    };
    let level = if gain == 0.0 {
        0.0
    } else {
        tinex_widgets::sanitize_level(track.output_level) * gain
    };
    Widget::MeterSlider {
        slider: Slider {
            id: track.volume_slider,
            value: track.volume_db,
            min: -61.0,
            max: 12.0,
            step: 1.0,
            width,
            enabled: state.connected && !track.deleting,
            on_change: Arc::new(move |db| UiAction::SetTrackVolume { track_id, db }),
        },
        meter: LevelMeter::new(level),
        readout: if track.volume_db == -61.0 {
            "−∞ dB".into()
        } else {
            format!("{:+.0} dB", track.volume_db)
        },
    }
}

fn track_arm_control(state: &AppState, track: &UiTrack) -> Widget<UiAction> {
    Widget::toggle_button(
        track.arm_button,
        "Arm",
        88.0,
        state.connected && !track.deleting && !state.arm_request_in_flight,
        track.armed,
        UiAction::ToggleTrackArm(track.id),
    )
}

fn status(state: &AppState) -> (String, Tone) {
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
        let text = if activity.is_empty() {
            message.clone()
        } else {
            format!("{message} · {activity}")
        };
        (text, Tone::Error)
    } else if activity.is_empty() {
        ("Ready".into(), Tone::Muted)
    } else {
        (activity, Tone::Muted)
    }
}

#[cfg(test)]
mod tests;
