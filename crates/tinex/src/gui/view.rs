//! Page routing and the shared, fixed status footer.
use tinex_widgets::{Tone, Widget};

use super::action::UiAction;
use super::state::{AppState, Page};

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
