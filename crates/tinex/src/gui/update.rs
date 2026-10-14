use tinex_core::track::Track;
use tinex_core::{TinexNotification, TinexRequest};
use tinex_widgets::{WidgetId, sanitize_level};

use crate::gui::state::UiPluginInstance;

use super::action::{Action, Effect, Operation, UiAction};
use super::state::{AppState, Page, PendingTrack, UiTrack};

#[derive(Default)]
pub(super) struct Update {
    pub changed: bool,
    pub effects: Vec<Effect>,
}

/// All domain-state transitions happen here. Tracks returned by notifications
/// are disposed of by this GUI-thread caller, never by a widget or audio callback.
pub(super) fn update(state: &mut AppState, action: Action) -> Update {
    let mut result = Update::default();
    match action {
        Action::Ui(UiAction::ToggleTrackArm(id))
            if state.connected && !state.arm_request_in_flight =>
        {
            if let Some(track) = state
                .tracks
                .iter()
                .find(|track| track.id == id && !track.deleting)
            {
                let tracks = if track.armed {
                    Default::default()
                } else {
                    [id].into()
                };
                result.effects.push(Effect {
                    request: TinexRequest::ArmTracks(tracks),
                    operation: Operation::ArmTracks,
                });
                state.arm_request_in_flight = true;
                state.message = None;
                result.changed = true;
            }
        }
        Action::Notification(TinexNotification::ArmTracks(tracks)) if state.connected => {
            result.changed = state.arm_request_in_flight;
            state.arm_request_in_flight = false;
            for track in &mut state.tracks {
                let armed = tracks.contains(&track.id);
                result.changed |= track.armed != armed;
                track.armed = armed;
            }
        }
        Action::Ui(UiAction::SetTrackVolume { track_id, db })
            if state.connected && db.is_finite() =>
        {
            if let Some(track) = state
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id && !track.deleting)
            {
                let db = db.clamp(-61.0, 12.0).round();
                if track.volume_db != db {
                    let previous_db = track.volume_db;
                    track.volume_db = db;
                    let gain = if db == -61.0 {
                        0.0
                    } else {
                        10.0_f32.powf(db / 20.0)
                    };
                    result.effects.push(Effect {
                        request: TinexRequest::SetTrackVolume(track_id, gain),
                        operation: Operation::SetTrackVolume {
                            track_id,
                            previous_db,
                        },
                    });
                    state.message = None;
                    result.changed = true;
                }
            }
        }
        Action::CpuLoad(load) => {
            let load = (state.connected && load.is_finite() && load >= 0.0)
                .then(|| (load.min(100.0) * 10.0).round() / 10.0);
            result.changed = state.cpu_load != load;
            state.cpu_load = load;
        }
        Action::Ui(UiAction::OpenTrack(id)) => {
            // Navigation is local and remains available when the engine is gone.
            // Pending or stale IDs must never open a nonexistent track page.
            if state.tracks.iter().any(|row| row.id == id) {
                result.changed = state.page != Page::Track(id);
                state.page = Page::Track(id);
            }
        }
        Action::Ui(UiAction::OpenPlugins { track_id, index }) if state.connected => {
            if state
                .tracks
                .iter()
                .any(|row| row.id == track_id && !row.deleting)
            {
                result.changed = state.page != Page::Plugins(track_id, index);
                state.page = Page::Plugins(track_id, index);
            }
        }
        Action::Ui(UiAction::AddPlugin { track_id, index }) if state.connected => {
            if let Page::Plugins(id, insertion_index) = state.page
                && id == track_id
                && state
                    .tracks
                    .iter()
                    .any(|row| row.id == track_id && !row.deleting)
                && let Some(builder) = tinex_plugins::FACTORY.get(index)
            {
                let plugin = builder.build(state.sample_rate);
                let plugin_id = plugin.id();
                result.effects.push(Effect {
                    request: TinexRequest::AddPlugin {
                        track_id,
                        plugin,
                        index: insertion_index,
                    },
                    operation: Operation::AddPlugin,
                });
                state
                    .tracks
                    .iter_mut()
                    .find(|row| row.id == track_id)
                    .expect("validated track")
                    .pending_plugins
                    .insert(
                        plugin_id,
                        UiPluginInstance {
                            id: plugin_id,
                            name: builder.name(),
                            delete_button: WidgetId::new(),
                            insert_before_button: WidgetId::new(),
                            deleting: false,
                        },
                    );
                state.page = Page::Track(track_id);
                state.message = None;
                result.changed = true;
            }
        }
        Action::Ui(UiAction::DeletePlugin {
            track_id,
            plugin_id,
        }) if state.connected => {
            if let Some(track) = state
                .tracks
                .iter_mut()
                .find(|track| track.id == track_id && !track.deleting)
                && let Some(plugin) = track
                    .plugins
                    .iter_mut()
                    .find(|plugin| plugin.id == plugin_id && !plugin.deleting)
            {
                plugin.deleting = true;
                state.message = None;
                result.effects.push(Effect {
                    request: TinexRequest::DeletePlugin(track_id, plugin_id),
                    operation: Operation::DeletePlugin {
                        track_id,
                        plugin_id,
                    },
                });
                result.changed = true;
            }
        }
        Action::Ui(UiAction::BackToMain) => {
            result.changed = state.page != Page::Main;
            state.page = Page::Main;
        }
        Action::Ui(UiAction::CreateTrack) if state.connected => {
            let track = Track::new();
            let id = track.id();
            let number = state.next_number;
            state.next_number += 1;
            state.pending.insert(
                id,
                PendingTrack {
                    name: format!("Track {number}"),
                    number,
                },
            );
            state.message = None;
            result.effects.push(Effect {
                request: TinexRequest::NewTrack(track),
                operation: Operation::Create(id),
            });
            result.changed = true;
        }
        Action::SampleLevels if state.connected && !state.level_request_in_flight => {
            let mut tracks = std::mem::take(&mut state.level_request_buffer);
            tracks.clear();
            for track in state.tracks.iter() {
                tracks.insert(track.id, 0.0);
            }
            state.level_request_in_flight = true;
            result.effects.push(Effect {
                request: TinexRequest::OutputLevel(tracks),
                operation: Operation::SampleLevels,
            });
        }
        Action::Ui(UiAction::DeleteTrack(id)) if state.connected => {
            if let Some(row) = state
                .tracks
                .iter_mut()
                .find(|row| row.id == id && !row.deleting)
            {
                row.deleting = true;
                state.message = None;
                result.effects.push(Effect {
                    request: TinexRequest::DeleteTrack(id),
                    operation: Operation::Delete(id),
                });
                result.changed = true;
            }
        }
        Action::Notification(TinexNotification::OutputLevel {
            output_level,
            mut tracks,
        }) if state.connected => {
            state.level_request_in_flight = false;
            let level = sanitize_level(output_level);
            result.changed |= state.master_output_level != level;
            state.master_output_level = level;
            for row in &mut state.tracks {
                let level = sanitize_level(tracks.get(&row.id).copied().unwrap_or(0.0));
                result.changed |= row.output_level != level;
                row.output_level = level;
            }
            tracks.retain(|id, _| state.tracks.iter().any(|row| row.id == *id));
            state.level_request_buffer = tracks;
        }
        Action::Notification(TinexNotification::TrackCreated(id)) => {
            if let Some(pending) = state.pending.remove(&id) {
                state.tracks.push(UiTrack {
                    id,
                    name: pending.name,
                    number: pending.number,
                    row: WidgetId::new(),
                    delete_button: WidgetId::new(),
                    arm_button: WidgetId::new(),
                    armed: false,
                    volume_slider: WidgetId::new(),
                    volume_db: 0.0,
                    deleting: false,
                    plugins: Vec::new(),
                    pending_plugins: Default::default(),
                    output_level: 0.0,
                });
                state.tracks.sort_by_key(|row| row.number);
                result.changed = true;
            }
        }
        Action::Notification(TinexNotification::TrackCreationFailed(track)) => {
            if let Some(pending) = state.pending.remove(&track.id()) {
                state.message = Some(format!(
                    "Could not create {} (track capacity reached).",
                    pending.name
                ));
                result.changed = true;
            }
        }
        Action::Notification(TinexNotification::TrackDeleted(track)) => {
            let count = state.tracks.len();
            state.tracks.retain(|row| row.id != track.id());
            state.level_request_buffer.remove(&track.id());
            result.changed = state.tracks.len() != count;
            if matches!(state.page, Page::Track(id) | Page::Plugins(id, _) if id == track.id()) {
                state.page = Page::Main;
                result.changed = true;
            }
        }
        Action::Notification(TinexNotification::PluginAdded {
            track_id,
            plugin_id,
            index,
        }) => {
            if let Some(row) = state.tracks.iter_mut().find(|row| row.id == track_id)
                && let Some(plugin) = row.pending_plugins.remove(&plugin_id)
            {
                row.plugins
                    .insert(usize::from(index).min(row.plugins.len()), plugin);
                result.changed = true;
            }
        }
        Action::Notification(TinexNotification::PluginDeleted(plugin)) => {
            for track in &mut state.tracks {
                let count = track.plugins.len();
                track.plugins.retain(|entry| entry.id != plugin.id());
                result.changed |= track.plugins.len() != count;
            }
            // The returned engine instance is dropped here on the GUI thread.
        }
        Action::RequestFailed(operation) => {
            match operation {
                Operation::SetTrackVolume {
                    track_id,
                    previous_db,
                } => {
                    if let Some(track) = state.tracks.iter_mut().find(|track| track.id == track_id)
                    {
                        track.volume_db = previous_db;
                    }
                }
                Operation::Create(id) => {
                    state.pending.remove(&id);
                }
                Operation::SampleLevels | Operation::AddPlugin | Operation::ArmTracks => {}
                Operation::DeletePlugin {
                    track_id,
                    plugin_id,
                } => {
                    if let Some(track) = state.tracks.iter_mut().find(|track| track.id == track_id)
                        && let Some(plugin) = track
                            .plugins
                            .iter_mut()
                            .find(|plugin| plugin.id == plugin_id)
                    {
                        plugin.deleting = false;
                    }
                }
                Operation::Delete(id) => {
                    if let Some(row) = state.tracks.iter_mut().find(|row| row.id == id) {
                        row.deleting = false;
                    }
                }
            }
            // A failed mpsc send means the engine is gone, not a transient failure.
            disconnect(state);
            result.changed = true;
        }
        Action::Disconnected if state.connected => {
            disconnect(state);
            result.changed = true;
        }
        Action::Notification(TinexNotification::OutputLevel { .. })
        | Action::Notification(TinexNotification::ArmTracks(_))
        | Action::SampleLevels
        | Action::Ui(_)
        | Action::Disconnected => {}
    }
    result
}

fn disconnect(state: &mut AppState) {
    state.connected = false;
    state.pending.clear();
    state.level_request_in_flight = false;
    state.arm_request_in_flight = false;
    state.level_request_buffer.clear();
    state.master_output_level = 0.0;
    state.cpu_load = None;
    for row in &mut state.tracks {
        row.deleting = false;
        row.pending_plugins.clear();
        for plugin in &mut row.plugins {
            plugin.deleting = false;
        }
        row.output_level = 0.0;
    }
    state.message = Some("Audio engine disconnected. Track controls are unavailable.".into());
}

#[cfg(test)]
mod meter_tests;
#[cfg(test)]
mod tests;
