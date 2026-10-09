use wmidi::MidiMessage;

use super::{id::Id, plugin::Plugin};

pub struct Track {
    id: Id<Track>,
    plugin: Box<dyn Plugin>,
}

impl std::fmt::Debug for Track {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Track")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Track {
    pub fn new(plugin: impl Plugin + 'static) -> Self {
        Self {
            id: Id::new(),
            plugin: Box::new(plugin),
        }
    }

    pub fn id(&self) -> Id<Track> {
        self.id
    }

    pub fn process(
        &mut self,
        midi: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        output: [&mut [f32]; 2],
    ) {
        self.plugin.process(midi, input, output);
    }
}
