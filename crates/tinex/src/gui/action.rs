use tinex_core::plugin::PluginInstanceId;
use tinex_core::track::TrackId;
use tinex_core::{TinexNotification, TinexRequest};

/// Only semantic, cloneable application actions are attached to widgets.
#[derive(Clone, Copy, Debug)]
pub(super) enum UiAction {
    SetTrackVolume {
        track_id: TrackId,
        db: f32,
    },
    CreateTrack,
    OpenTrack(TrackId),
    OpenPlugins(TrackId),
    AddPlugin {
        track_id: TrackId,
        index: usize,
    },
    DeletePlugin {
        track_id: TrackId,
        plugin_id: PluginInstanceId,
    },
    BackToMain,
    DeleteTrack(TrackId),
    ToggleTrackArm(TrackId),
}

pub(super) enum Action {
    Ui(UiAction),
    SampleLevels,
    CpuLoad(f32),
    Notification(TinexNotification),
    RequestFailed(Operation),
    Disconnected,
}

#[derive(Clone, Copy)]
pub(super) enum Operation {
    SetTrackVolume {
        track_id: TrackId,
        previous_db: f32,
    },
    Create(TrackId),
    Delete(TrackId),
    ArmTracks,
    AddPlugin,
    DeletePlugin {
        track_id: TrackId,
        plugin_id: PluginInstanceId,
    },
    SampleLevels,
}

pub(super) struct Effect {
    pub request: TinexRequest,
    pub operation: Operation,
}
