//! Tinex instrument and effect implementations.
mod compressor;
mod delay;
mod epiano;
mod motown_bass;
mod reverb;
mod tremolo;

pub use compressor::Compressor;
pub use delay::Delay;
pub use epiano::EPiano;
pub use motown_bass::MotownBass;
pub use reverb::Reverb;
pub use tremolo::Tremolo;

use tinex_core::plugin::Plugin;

/// A named constructor for an available plugin.
#[derive(Clone, Copy)]
pub struct PluginBuilder {
    name: &'static str,
    build: fn(f32) -> Box<dyn Plugin>,
}

impl PluginBuilder {
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Constructs a fresh plugin at the given sample rate.
    ///
    /// Panics if the sample rate is not finite and positive.
    pub fn build(&self, sample_rate: f32) -> Box<dyn Plugin> {
        (self.build)(sample_rate)
    }
}

/// Available plugin constructors.
pub const FACTORY: [PluginBuilder; 6] = [
    PluginBuilder {
        name: "EPiano",
        build: |sample_rate| Box::new(EPiano::new(sample_rate)),
    },
    PluginBuilder {
        name: "Bass",
        build: |sample_rate| Box::new(MotownBass::new(sample_rate)),
    },
    PluginBuilder {
        name: "Tremolo",
        build: |sample_rate| Box::new(Tremolo::new(sample_rate)),
    },
    PluginBuilder {
        name: "Compressor",
        build: |sample_rate| Box::new(Compressor::new(sample_rate)),
    },
    PluginBuilder {
        name: "Delay",
        build: |sample_rate| Box::new(Delay::new(sample_rate)),
    },
    PluginBuilder {
        name: "Reverb",
        build: |sample_rate| Box::new(Reverb::new(sample_rate)),
    },
];
