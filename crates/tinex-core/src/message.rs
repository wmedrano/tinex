use std::collections::HashMap;

use crate::{
    plugin::{PluginInstance, PluginInstanceId},
    track::{Track, TrackId},
};

pub enum TinexRequest {
    /// Updates the supplied track entries with exponentially weighted block peak levels.
    /// Entries for unknown tracks are left untouched; the response returns the same map
    /// allocation.
    OutputLevel(HashMap<TrackId, f32>),
    NewTrack(Track),
    DeleteTrack(TrackId),
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
    PluginAdded(TrackId, PluginInstanceId),
}
