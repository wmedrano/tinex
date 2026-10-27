//! Application-specific Xilem/Masonry views and pure audio-meter helpers.
//!
//! Xilem owns interaction, focus, text, and layout. The only custom painting is
//! the segmented dBFS meter, the timeline ruler, and transport symbols; [`keyed`] gives ordered domain
//! collections stable identities when items are inserted or deleted.
mod fonts;
mod keyed;
mod level_meter;
mod meter;
mod timeline;
mod transport_icon;

pub use fonts::{FONT_EMOJI, FONT_SANS, FONT_STACK};
pub use keyed::keyed;
pub use level_meter::{
    LevelMeter, METER_MAX_DB, METER_MIN_DB, MeterStyle, amplitude_to_dbfs, sanitize_level,
};
pub use meter::{Meter, MeterWidget, level_meter, percentage_meter};
pub use timeline::{Timeline, TimelineStyle, TimelineWidget, timeline};
pub use transport_icon::{TransportGlyph, TransportIcon, TransportIconWidget, transport_icon};
