use std::collections::HashMap;

use tinex_core::{plugin::PluginInstanceId, track::TrackId};

pub(super) struct UiTrack {
    pub id: TrackId,
    pub name: String,
    pub number: u64,
    /// Confirmed membership in the engine's armed-track set.
    pub armed: bool,
    /// Requested dB setting; -61 represents silence.
    pub volume_db: f32,
    pub deleting: bool,
    /// Confirmed plugins in audio-chain order (including duplicates), each carrying
    /// the id the engine uses to address it.
    pub plugins: Vec<UiPluginInstance>,
    /// Plugins awaiting engine confirmation, keyed by the id the engine echoes back.
    pub pending_plugins: HashMap<PluginInstanceId, UiPluginInstance>,
    /// Linear, exponentially weighted block peak reported by the engine.
    pub output_level: f32,
}

/// A plugin on a track, addressed by the id the engine echoes back in `PluginAdded`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UiPluginInstance {
    pub id: PluginInstanceId,
    pub name: &'static str,
    pub deleting: bool,
}

pub(super) struct PendingTrack {
    pub name: String,
    pub number: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    Main,
    Track(TrackId),
    Plugins(TrackId, u16),
}

pub(super) struct AppState {
    pub page: Page,
    pub sample_rate: f32,
    pub tracks: Vec<UiTrack>,
    pub pending: HashMap<TrackId, PendingTrack>,
    pub next_number: u64,
    pub connected: bool,
    pub message: Option<String>,
    /// Linear, exponentially weighted peak of the engine's combined output.
    pub master_output_level: f32,
    /// Requested tempo; engine reports overwrite it once they reflect the request.
    pub bpm: f32,
    /// Requested play state; engine reports overwrite it once they reflect the request.
    pub playing: bool,
    /// A status poll sent before the last transport change reports the old transport.
    pub transport_stale: bool,
    /// Last reported transport position in beats.
    pub beat: f64,
    pub time_signature: tinex_core::TimeSignature,
    /// Arrangement length in bars.
    pub measures: u32,
    /// JACK CPU utilization percentage, rounded to one decimal for display.
    pub cpu_load: Option<f32>,
    pub level_request_in_flight: bool,
    /// Arming replaces the whole set, so serialize requests across all tracks.
    pub arm_request_in_flight: bool,
    /// Returned by the engine and reused for the next level request.
    pub level_request_buffer: HashMap<TrackId, f32>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            page: Page::Main,
            sample_rate: 48_000.0,
            tracks: Vec::new(),
            pending: HashMap::new(),
            next_number: 1,
            connected: true,
            message: None,
            master_output_level: 0.0,
            bpm: 120.0,
            playing: false,
            transport_stale: false,
            beat: 0.0,
            time_signature: tinex_core::TimeSignature::default(),
            measures: tinex_core::Transport::default().measures,
            cpu_load: None,
            level_request_in_flight: false,
            arm_request_in_flight: false,
            level_request_buffer: HashMap::new(),
        }
    }
}
