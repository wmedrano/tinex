use wmidi::MidiMessage;

use super::{id::Id, output_level_weight, plugin::Plugin};

pub type TrackId = Id<Track>;

pub struct Track {
    id: TrackId,
    output_level: f64,
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
            output_level: 0.0,
            plugin: Box::new(plugin),
        }
    }

    pub fn id(&self) -> TrackId {
        self.id
    }

    /// The exponentially weighted peak absolute level of completed, nonempty blocks.
    /// Smoothing uses a 100 ms time constant, independent of sample rate and block size.
    pub fn output_level(&self) -> f32 {
        self.output_level as f32
    }

    pub fn process(
        &mut self,
        midi: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        mut output: [&mut [f32]; 2],
        sample_rate: u32,
    ) {
        let [left, right] = output.each_mut();
        self.plugin.process(midi, input, [left, right]);
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
            self.output_level +=
                (f64::from(peak) - self.output_level) * output_level_weight(frames, sample_rate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        track.process(&[], [&[level], &[]], [&mut left, &mut right], sample_rate);
    }

    #[test]
    fn recent_peaks_respond_and_decay_independently_of_track_age() {
        let mut decayed_levels = Vec::new();
        for history in [100, 1000] {
            let mut track = Track::new(Level);
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
                let mut track = Track::new(Level);
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
