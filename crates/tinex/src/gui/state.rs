use std::collections::{HashMap, VecDeque};

use tinex_core::track::TrackId;
use tinex_widgets::WidgetId;

pub(super) struct TrackRow {
    pub id: TrackId,
    pub name: String,
    pub number: u64,
    pub row: WidgetId,
    pub open_button: WidgetId,
    pub delete_button: WidgetId,
    pub deleting: bool,
    /// Confirmed plugin names in audio-chain order (including duplicates).
    pub plugins: Vec<&'static str>,
    /// Names awaiting engine confirmation, in request order.
    pub pending_plugins: VecDeque<&'static str>,
    /// Linear, exponentially weighted block peak reported by the engine.
    pub output_level: f32,
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
    pub tracks: Vec<TrackRow>,
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
            level_request_buffer: HashMap::new(),
        }
    }
}
