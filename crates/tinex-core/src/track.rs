use wmidi::MidiMessage;

use crate::{
    Beat,
    plugin::{Plugin, PluginInstance, PluginInstanceId},
    sequence::MidiSequence,
};

use super::{id::Id, output_level_weight};

const MIDI_SCRATCH_CAPACITY: usize = 256;

pub type TrackId = Id<Track>;

pub struct Track {
    pub volume: f32,
    pub armed: bool,
    pub sequence: MidiSequence,
    id: TrackId,
    output_level: f64,
    plugins: Vec<PluginInstance>,
    midi_scratch: Vec<(usize, MidiMessage<'static>)>,
}

impl std::fmt::Debug for Track {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let plugins_count = self.plugins.len();
        f.debug_struct("Track")
            .field("id", &self.id)
            .field("plugins_count", &plugins_count)
            .finish_non_exhaustive()
    }
}

impl Track {
    pub fn new() -> Self {
        Self {
            volume: 1.0,
            armed: false,
            sequence: MidiSequence::new(),
            id: Id::new(),
            output_level: 0.0,
            plugins: Vec::with_capacity(8),
            midi_scratch: Vec::with_capacity(MIDI_SCRATCH_CAPACITY),
        }
    }

    pub fn with_plugin(plugin: impl Plugin) -> Self {
        let mut track = Self::new();
        track.push_plugin(PluginInstance::new(Box::new(plugin)));
        track
    }

    pub fn push_plugin(&mut self, plugin: PluginInstance) {
        self.plugins.push(plugin);
    }

    /// Inserts before `index`, appending when it exceeds the chain length.
    /// Returns the actual position for the engine's confirmation.
    pub fn insert_plugin(&mut self, index: u16, plugin: PluginInstance) -> u16 {
        let index = usize::from(index).min(self.plugins.len());
        self.plugins.insert(index, plugin);
        index as u16
    }

    pub fn remove_plugin(
        &mut self,
        plugin_instance_id: PluginInstanceId,
    ) -> Option<PluginInstance> {
        let idx = self
            .plugins
            .iter()
            .position(|p| p.id() == plugin_instance_id)?;
        Some(self.plugins.remove(idx))
    }

    pub fn id(&self) -> TrackId {
        self.id
    }

    /// The exponentially weighted peak absolute level of completed, nonempty blocks.
    /// Smoothing uses a 100 ms time constant, independent of sample rate and block size.
    pub fn output_level(&self) -> f32 {
        self.output_level as f32
    }

    /// Buffers are two stereo pairs in left/right order. Returns the rendered pair.
    /// Plugins run in chain order, each receiving the previous plugin's output.
    /// Plugins must write every output sample; buffer contents are otherwise unspecified.
    /// A track without plugins writes silence to the returned pair.
    pub fn process<'buffers>(
        &mut self,
        midi: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        buffers: [&'buffers mut [f32]; 4],
        sample_rate: u32,
    ) -> [&'buffers mut [f32]; 2] {
        Self::process_plugins(
            &mut self.plugins,
            &mut self.output_level,
            midi,
            input,
            buffers,
            sample_rate,
        )
    }

    /// Plays back recorded sequence events along `beats`, merges live `midi_input` when
    /// armed, renders the plugin chain, and overdubs live `midi_input` when `recording`.
    pub(crate) fn process_block<'buffers>(
        &mut self,
        beats: &[Beat],
        recording: bool,
        midi_input: &[(usize, MidiMessage<'static>)],
        input: [&[f32]; 2],
        buffers: [&'buffers mut [f32]; 4],
        sample_rate: u32,
    ) -> [&'buffers mut [f32]; 2] {
        let live_midi = if self.armed { midi_input } else { &[] };
        let stationary = beats.len() < 2 || beats[0] == beats[1];
        let output = if self.sequence.is_empty() || stationary {
            Self::process_plugins(
                &mut self.plugins,
                &mut self.output_level,
                live_midi,
                input,
                buffers,
                sample_rate,
            )
        } else {
            self.sequence
                .collect_events(beats, live_midi, &mut self.midi_scratch);
            Self::process_plugins(
                &mut self.plugins,
                &mut self.output_level,
                &self.midi_scratch,
                input,
                buffers,
                sample_rate,
            )
        };
        if recording && self.armed {
            self.sequence.record_events(beats, live_midi);
        }
        output
    }

    fn process_plugins<'buffers>(
        plugins: &mut [PluginInstance],
        output_level: &mut f64,
        midi: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        buffers: [&'buffers mut [f32]; 4],
        sample_rate: u32,
    ) -> [&'buffers mut [f32]; 2] {
        let [left, right, scratch_left, scratch_right] = buffers;
        let mut output = [left, right];
        let mut scratch = [scratch_left, scratch_right];
        if let Some((first, remaining)) = plugins.split_first_mut() {
            let [left, right] = output.each_mut();
            first.process(midi, input, [left, right]);
            for plugin in remaining {
                std::mem::swap(&mut output, &mut scratch);
                let [left, right] = output.each_mut();
                plugin.process(midi, [&*scratch[0], &*scratch[1]], [left, right]);
            }
        } else {
            for channel in &mut output {
                channel.fill(0.0);
            }
        }
        if output.iter().any(|channel| !channel.is_empty()) {
            let peak = output
                .iter()
                .flat_map(|channel| channel.iter())
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            let frames = output
                .iter()
                .map(|channel| channel.len())
                .max()
                .unwrap_or(0);
            *output_level +=
                (f64::from(peak) - *output_level) * output_level_weight(frames, sample_rate);
        }
        output
    }
}

impl Default for Track {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::plugin::Plugin;

    use super::*;

    struct Transform {
        gain: f32,
        offset: f32,
    }

    impl Plugin for Transform {
        fn process(
            &mut self,
            midi: &[(usize, MidiMessage)],
            input: [&[f32]; 2],
            output: [&mut [f32]; 2],
        ) {
            assert_eq!(midi, &[(0, MidiMessage::Start)]);
            for (input, output) in input.into_iter().zip(output) {
                for (input, output) in input.iter().zip(output) {
                    *output = input * self.gain + self.offset;
                }
            }
        }
    }

    #[test]
    fn chains_plugins_in_order_with_reused_stereo_buffers_and_shared_midi() {
        for count in 0..=3 {
            let mut track = Track::new();
            let input = [[1.0, -2.0], [3.0, -4.0]];
            let mut expected = input;
            for index in 0..count {
                let offset = index as f32 + 1.0;
                let plugin = PluginInstance::new(Box::new(Transform { gain: 2.0, offset }));
                track.push_plugin(plugin);
                for channel in &mut expected {
                    for sample in channel {
                        *sample = *sample * 2.0 + offset;
                    }
                }
            }
            if count == 0 {
                expected = [[0.0; 2]; 2];
            }
            let mut buffers = [[99.0; 2]; 4];
            let pair_index = if count == 2 { 2 } else { 0 };
            let expected_pointer = buffers[pair_index].as_ptr();
            let output = track.process(
                &[(0, MidiMessage::Start)],
                [&input[0], &input[1]],
                buffers.each_mut().map(|channel| channel.as_mut_slice()),
                48_000,
            );
            assert_eq!(output[0].as_ptr(), expected_pointer);
            assert_eq!(output[0], expected[0]);
            assert_eq!(output[1], expected[1]);
            let peak = expected
                .into_iter()
                .flatten()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            let expected_level = f64::from(peak) * output_level_weight(2, 48_000);
            assert!((track.output_level() - expected_level as f32).abs() < 1e-6);
        }
    }

    struct Level;
    impl Plugin for Level {
        fn process(
            &mut self,
            _: &[(usize, MidiMessage)],
            input: [&[f32]; 2],
            output: [&mut [f32]; 2],
        ) {
            for channel in output {
                channel.fill(-input[0].first().copied().unwrap_or(0.0));
            }
        }
    }

    fn process_frames(track: &mut Track, level: f32, frames: usize, sample_rate: u32) {
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        let mut scratch_left = vec![0.0; frames];
        let mut scratch_right = vec![0.0; frames];
        track.process(
            &[],
            [&[level], &[]],
            [&mut left, &mut right, &mut scratch_left, &mut scratch_right],
            sample_rate,
        );
    }

    #[test]
    fn recent_peaks_respond_and_decay_independently_of_track_age() {
        let mut decayed_levels = Vec::new();
        for history in [100, 1000] {
            let mut track = Track::new();
            track.push_plugin(PluginInstance::new(Box::new(Level)));
            assert_eq!(track.output_level(), 0.0);
            for _ in 0..history {
                process_frames(&mut track, 1.0, 480, 48_000);
            }
            assert!(track.output_level() > 0.99);
            for _ in 0..24 {
                process_frames(&mut track, 0.0, 480, 48_000);
            }
            assert!(track.output_level() < 0.091);
            for _ in 0..26 {
                process_frames(&mut track, 0.0, 480, 48_000);
            }
            let decayed = track.output_level();
            assert!(decayed < 0.007);
            decayed_levels.push(decayed);
            process_frames(&mut track, 0.0, 0, 48_000);
            assert_eq!(track.output_level(), decayed);
            process_frames(&mut track, 1.0, 480, 48_000);
            assert!(track.output_level() > 0.095);
        }
        assert!((decayed_levels[0] - decayed_levels[1]).abs() < 1e-6);
    }

    #[test]
    fn smoothing_depends_on_elapsed_audio_time_across_sample_rates_and_block_sizes() {
        for sample_rate in [44_100, 48_000, 96_000] {
            for block_size in [1, 128, 1024] {
                let mut track = Track::new();
                track.push_plugin(PluginInstance::new(Box::new(Level)));
                for (level, frames) in [
                    (1.0, sample_rate as usize),
                    (0.0, sample_rate as usize / 10),
                ] {
                    let mut remaining = frames;
                    while remaining > 0 {
                        let count = remaining.min(block_size);
                        process_frames(&mut track, level, count, sample_rate);
                        remaining -= count;
                    }
                    let expected = if level == 1.0 {
                        1.0 - (-10.0_f32).exp()
                    } else {
                        (1.0 - (-10.0_f32).exp()) * (-1.0_f32).exp()
                    };
                    assert!(
                        (track.output_level() - expected).abs() < 1e-6,
                        "sample rate {sample_rate}, block size {block_size}"
                    );
                }
            }
        }
    }
}
