use std::collections::{HashMap, HashSet};

use crate::{
    Beat, Transport,
    plugin::{PluginInstance, PluginInstanceId},
    track::{Track, TrackId},
};

pub enum TinexRequest {
    PollStatus {
        // The tracks to request volume for. Only tracks in this set will receive levels. Any tracks
        // that are missing will not have any update.
        requested_tracks: HashMap<TrackId, f32>,
    },
    SetTransport(TransportChange),
    SetTrackVolume(TrackId, f32),
    NewTrack(Track),
    DeleteTrack(TrackId),
    ArmTracks(HashSet<TrackId>),
    AddPlugin {
        track_id: TrackId,
        plugin: PluginInstance,
        // If out of bounds (like when using u16::MAX), then we append at the end.
        index: u16,
    },
    DeletePlugin(TrackId, PluginInstanceId),
}

/// Changes to apply to the transport. `None` leaves a field as it is.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub struct TransportChange {
    pub bpm: Option<f32>,
    pub playing: Option<bool>,
    /// Positions past the end of the arrangement wrap on the next block.
    pub beat: Option<Beat>,
}

#[derive(Debug)]
pub enum TinexNotification {
    Status {
        /// The output level of the final mix.
        output_level: f32,
        /// A map from `TrackId` to its level.
        tracks: HashMap<TrackId, f32>,
        /// The status of the transport.
        transport: Transport,
    },
    TrackCreated(TrackId),
    TrackCreationFailed(Track),
    TrackDeleted(Track),
    ArmTracks(HashSet<TrackId>),
    /// Track, plugin instance, and actual insertion index in the audio chain.
    PluginAdded {
        track_id: TrackId,
        plugin_id: PluginInstanceId,
        index: u16,
    },
    PluginDeleted(PluginInstance),
}
