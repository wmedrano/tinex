use std::collections::HashMap;

use tinex_core::{plugin::PluginInstanceId, track::TrackId};
use tinex_widgets::WidgetId;

pub(super) struct UiTrack {
    pub id: TrackId,
    pub name: String,
    pub number: u64,
    pub row: WidgetId,
    pub delete_button: WidgetId,
    pub arm_button: WidgetId,
    /// Confirmed membership in the engine's armed-track set.
    pub armed: bool,
    pub volume_slider: WidgetId,
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
    pub delete_button: WidgetId,
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
    Plugins(TrackId),
}

pub(super) struct AppState {
    pub page: Page,
    pub back_button: WidgetId,
    pub track_page: WidgetId,
    pub add_plugin_button: WidgetId,
    pub plugin_page: WidgetId,
    pub plugin_back_button: WidgetId,
    pub plugin_buttons: [WidgetId; tinex_plugins::FACTORY.len()],
    pub sample_rate: f32,
    pub tracks: Vec<UiTrack>,
    pub pending: HashMap<TrackId, PendingTrack>,
    pub next_number: u64,
    pub connected: bool,
    pub message: Option<String>,
    pub create_button: WidgetId,
    pub track_list: WidgetId,
    /// Linear, exponentially weighted peak of the engine's combined output.
    pub master_output_level: f32,
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
            back_button: WidgetId::new(),
            track_page: WidgetId::new(),
            add_plugin_button: WidgetId::new(),
            plugin_page: WidgetId::new(),
            plugin_back_button: WidgetId::new(),
            plugin_buttons: std::array::from_fn(|_| WidgetId::new()),
            sample_rate: 48_000.0,
            tracks: Vec::new(),
            pending: HashMap::new(),
            next_number: 1,
            connected: true,
            message: None,
            create_button: WidgetId::new(),
            track_list: WidgetId::new(),
            master_output_level: 0.0,
            cpu_load: None,
            level_request_in_flight: false,
            arm_request_in_flight: false,
            level_request_buffer: HashMap::new(),
        }
    }
}
