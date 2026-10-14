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

use tinex_core::plugin::{Plugin, PluginInstance};

/// Whether a plugin generates sound from MIDI or processes incoming audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginKind {
    Instrument,
    Effect,
}

/// A named constructor and display metadata for an available plugin.
#[derive(Clone, Copy)]
pub struct PluginBuilder {
    name: &'static str,
    kind: PluginKind,
    description: &'static str,
    build: fn(f32) -> Box<dyn Plugin>,
}

impl PluginBuilder {
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The plugin's role in an audio chain.
    pub fn kind(&self) -> PluginKind {
        self.kind
    }

    /// A short description for plugin browsers.
    pub fn description(&self) -> &'static str {
        self.description
    }

    /// Constructs a fresh plugin at the given sample rate.
    ///
    /// Panics if the sample rate is not finite and positive.
    pub fn build(&self, sample_rate: f32) -> PluginInstance {
        let plugin = (self.build)(sample_rate);
        PluginInstance::new(plugin)
    }
}

/// Available plugin constructors.
pub const FACTORY: [PluginBuilder; 6] = [
    PluginBuilder {
        name: "EPiano",
        kind: PluginKind::Instrument,
        description: "Physical-model electric piano.",
        build: |sample_rate| Box::new(EPiano::new(sample_rate)),
    },
    PluginBuilder {
        name: "Bass",
        kind: PluginKind::Instrument,
        description: "Warm, muted electric bass.",
        build: |sample_rate| Box::new(MotownBass::new(sample_rate)),
    },
    PluginBuilder {
        name: "Tremolo",
        kind: PluginKind::Effect,
        description: "Rhythmic volume modulation.",
        build: |sample_rate| Box::new(Tremolo::new(sample_rate)),
    },
    PluginBuilder {
        name: "Compressor",
        kind: PluginKind::Effect,
        description: "Stereo-linked peak compression.",
        build: |sample_rate| Box::new(Compressor::new(sample_rate)),
    },
    PluginBuilder {
        name: "Delay",
        kind: PluginKind::Effect,
        description: "Stereo echoes with feedback.",
        build: |sample_rate| Box::new(Delay::new(sample_rate)),
    },
    PluginBuilder {
        name: "Reverb",
        kind: PluginKind::Effect,
        description: "Stereo ambience with a damped tail.",
        build: |sample_rate| Box::new(Reverb::new(sample_rate)),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_unique_names_descriptions_and_correct_roles() {
        let roles = FACTORY.map(|builder| (builder.name(), builder.kind()));
        assert_eq!(
            roles,
            [
                ("EPiano", PluginKind::Instrument),
                ("Bass", PluginKind::Instrument),
                ("Tremolo", PluginKind::Effect),
                ("Compressor", PluginKind::Effect),
                ("Delay", PluginKind::Effect),
                ("Reverb", PluginKind::Effect),
            ]
        );
        let mut names = std::collections::HashSet::new();
        for builder in &FACTORY {
            assert!(names.insert(builder.name()));
            assert!(!builder.description().trim().is_empty());
        }
    }
}
