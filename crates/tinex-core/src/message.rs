use std::collections::{HashMap, HashSet};

use crate::{
    plugin::{PluginInstance, PluginInstanceId},
    track::{Track, TrackId},
};

pub enum TinexRequest {
    /// Updates the supplied track entries with exponentially weighted block peak levels.
    /// Entries for unknown tracks are left untouched; the response returns the same map
    /// allocation.
    OutputLevel(HashMap<TrackId, f32>),
    SetTrackVolume(TrackId, f32),
    NewTrack(Track),
    DeleteTrack(TrackId),
    ArmTracks(HashSet<TrackId>),
    AddPlugin {
        track_id: TrackId,
        plugin: PluginInstance,
    },
}

#[derive(Debug)]
pub enum TinexNotification {
    OutputLevel {
        output_level: f32,
        tracks: HashMap<TrackId, f32>,
    },
    TrackCreated(TrackId),
    TrackCreationFailed(Track),
    TrackDeleted(Track),
    ArmTracks(HashSet<TrackId>),
    PluginAdded(TrackId, PluginInstanceId),
}
