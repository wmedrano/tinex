//! Tinex instrument and effect implementations.
mod delay;
mod epiano;
mod tremolo;

pub use delay::Delay;
pub use epiano::EPiano;
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
pub const FACTORY: [PluginBuilder; 3] = [
    PluginBuilder {
        name: "EPiano",
        build: |sample_rate| Box::new(EPiano::new(sample_rate)),
    },
    PluginBuilder {
        name: "Tremolo",
        build: |sample_rate| Box::new(Tremolo::new(sample_rate)),
    },
    PluginBuilder {
        name: "Delay",
        build: |sample_rate| Box::new(Delay::new(sample_rate)),
    },
];
