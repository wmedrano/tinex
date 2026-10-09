use tinex_core::plugin::Plugin;
use wmidi::MidiMessage;

const COMB_LENGTHS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const DIFFUSER_LENGTHS: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const COMB_FEEDBACK: f32 = 0.84;
const COMB_DAMPING: f32 = 0.30;
const DIFFUSER_FEEDBACK: f32 = 0.5;
const WET_MIX: f32 = 0.25;

fn delay_frames(reference_frames: usize, sample_rate: f32) -> usize {
    (reference_frames as f64 * f64::from(sample_rate) / 44_100.0)
        .round()
        .max(1.0) as usize
}

fn without_denormals(sample: f32) -> f32 {
    if sample.abs() < 1e-20 { 0.0 } else { sample }
}

struct Comb {
    buffer: Box<[f32]>,
    position: usize,
    lowpass: f32,
}

impl Comb {
    fn new(frames: usize) -> Self {
        Self {
            buffer: vec![0.0; frames].into_boxed_slice(),
            position: 0,
            lowpass: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = without_denormals(self.buffer[self.position]);
        self.lowpass =
            without_denormals(delayed * (1.0 - COMB_DAMPING) + self.lowpass * COMB_DAMPING);
        self.buffer[self.position] = without_denormals(input + self.lowpass * COMB_FEEDBACK);
        self.position += 1;
        if self.position == self.buffer.len() {
            self.position = 0;
        }
        delayed
    }
}

struct AllPass {
    buffer: Box<[f32]>,
    position: usize,
}

impl AllPass {
    fn new(frames: usize) -> Self {
        Self {
            buffer: vec![0.0; frames].into_boxed_slice(),
            position: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = without_denormals(self.buffer[self.position]);
        self.buffer[self.position] = without_denormals(input + delayed * DIFFUSER_FEEDBACK);
        self.position += 1;
        if self.position == self.buffer.len() {
            self.position = 0;
        }
        delayed - input * DIFFUSER_FEEDBACK
    }
}

/// A stereo reverb with a roughly one-second, damped tail and a 25% wet mix.
///
/// The wet signal has an 80 Hz low cut and slight channel crossfeed. Left and
/// right delay lengths differ to spread mono sounds across the stereo field.
/// MIDI is ignored; processing allocates no memory and preserves tails across blocks.
pub struct Reverb {
    combs: [[Comb; COMB_LENGTHS.len()]; 2],
    diffusers: [[AllPass; DIFFUSER_LENGTHS.len()]; 2],
    lowpass: [f64; 2],
    highpass_coefficient: f64,
}

impl Reverb {
    /// Constructs a fresh reverb at the given sample rate.
    ///
    /// Panics if the sample rate is not finite and positive.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            sample_rate.is_finite() && sample_rate > 0.0,
            "sample rate must be finite and positive"
        );
        Self {
            combs: std::array::from_fn(|channel| {
                std::array::from_fn(|index| {
                    Comb::new(delay_frames(
                        COMB_LENGTHS[index] + channel * STEREO_SPREAD,
                        sample_rate,
                    ))
                })
            }),
            diffusers: std::array::from_fn(|channel| {
                std::array::from_fn(|index| {
                    AllPass::new(delay_frames(
                        DIFFUSER_LENGTHS[index] + channel * STEREO_SPREAD,
                        sample_rate,
                    ))
                })
            }),
            lowpass: [0.0; 2],
            highpass_coefficient: (-std::f64::consts::TAU * 80.0 / f64::from(sample_rate)).exp(),
        }
    }

    fn wet_sample(&mut self, channel: usize, input: f32) -> f32 {
        let mut combined = 0.0;
        for comb in &mut self.combs[channel] {
            combined += comb.process(input);
        }
        let mut wet = combined / COMB_LENGTHS.len() as f32;
        for diffuser in &mut self.diffusers[channel] {
            wet = diffuser.process(wet);
        }
        wet
    }
}

impl Plugin for Reverb {
    fn process(
        &mut self,
        _: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        outputs: [&mut [f32]; 2],
    ) {
        let [left, right] = outputs;
        for frame in 0..left.len().max(right.len()) {
            let dry_left = input[0].get(frame).copied().unwrap_or(0.0);
            let dry_right = input[1].get(frame).copied().unwrap_or(0.0);
            let dry = [dry_left, dry_right];
            let mut highpassed = [0.0; 2];
            for channel in 0..2 {
                self.lowpass[channel] = self.highpass_coefficient * self.lowpass[channel]
                    + (1.0 - self.highpass_coefficient) * f64::from(dry[channel]);
                if self.lowpass[channel].abs() < 1e-20 {
                    self.lowpass[channel] = 0.0;
                }
                highpassed[channel] = (f64::from(dry[channel]) - self.lowpass[channel]) as f32;
            }

            let wet_left = self.wet_sample(0, highpassed[0] * 0.75 + highpassed[1] * 0.25);
            let wet_right = self.wet_sample(1, highpassed[1] * 0.75 + highpassed[0] * 0.25);
            if let Some(output) = left.get_mut(frame) {
                *output = dry_left * (1.0 - WET_MIX) + wet_left * WET_MIX;
            }
            if let Some(output) = right.get_mut(frame) {
                *output = dry_right * (1.0 - WET_MIX) + wet_right * WET_MIX;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_produces_a_decaying_stereo_tail() {
        let mut reverb = Reverb::new(8_000.0);
        let mut left = vec![f32::NAN; 16_000];
        let mut right = vec![f32::NAN; 16_000];
        reverb.process(&[], [&[1.0], &[]], [&mut left, &mut right]);
        assert_eq!(left[0], 0.75);
        assert_eq!(right[0], 0.0);
        assert!(left[200..8_000].iter().any(|sample| sample.abs() > 1e-5));
        assert!(right[200..8_000].iter().any(|sample| sample.abs() > 1e-5));
        assert_ne!(left[200..8_000], right[200..8_000]);

        let early_energy: f32 = left[200..4_000].iter().map(|sample| sample * sample).sum();
        let late_energy: f32 = left[12_000..].iter().map(|sample| sample * sample).sum();
        assert!(early_energy > late_energy * 10.0);
        assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
    }

    #[test]
    fn block_boundaries_and_midi_do_not_change_the_tail() {
        let input: Vec<f32> = (0..4_000)
            .map(|frame| if frame % 101 == 0 { 0.5 } else { 0.0 })
            .collect();
        let mut whole_left = vec![0.0; input.len()];
        let mut whole_right = vec![0.0; input.len()];
        Reverb::new(8_000.0).process(&[], [&input, &[]], [&mut whole_left, &mut whole_right]);

        let mut split_left = vec![0.0; input.len()];
        let mut split_right = vec![0.0; input.len()];
        let mut reverb = Reverb::new(8_000.0);
        for ((source, left), right) in input
            .chunks(73)
            .zip(split_left.chunks_mut(73))
            .zip(split_right.chunks_mut(73))
        {
            reverb.process(&[(0, MidiMessage::Start)], [source, &[]], [left, right]);
        }
        assert_eq!(split_left, whole_left);
        assert_eq!(split_right, whole_right);
    }

    #[test]
    fn sustained_dc_does_not_build_up_in_the_wet_signal() {
        let input = vec![1.0; 24_000];
        let mut output = vec![0.0; input.len()];
        Reverb::new(8_000.0).process(&[], [&input, &[]], [&mut output, &mut []]);
        assert!((output[23_999] - 0.75).abs() < 0.01);
    }

    #[test]
    fn missing_input_and_unequal_outputs_are_written() {
        for longer_channel in 0..2 {
            let mut reverb = Reverb::new(8_000.0);
            let mut short = [f32::NAN; 2];
            let mut long = [f32::NAN; 3];
            let outputs = if longer_channel == 0 {
                [&mut long[..], &mut short[..]]
            } else {
                [&mut short[..], &mut long[..]]
            };
            reverb.process(&[], [&[1.0], &[]], outputs);
            if longer_channel == 0 {
                assert_eq!(long, [0.75, 0.0, 0.0]);
                assert_eq!(short, [0.0; 2]);
            } else {
                assert_eq!(short, [0.75, 0.0]);
                assert_eq!(long, [0.0; 3]);
            }
        }
    }

    #[test]
    fn factory_builds_reverb_and_invalid_rates_are_rejected() {
        let builder = crate::FACTORY
            .iter()
            .find(|builder| builder.name() == "Reverb")
            .unwrap();
        let mut plugin = builder.build(48_000.0);
        let mut left = [f32::NAN; 1];
        let mut right = [f32::NAN; 1];
        plugin.process(&[], [&[1.0], &[]], [&mut left, &mut right]);
        assert_eq!(left, [0.75]);
        assert_eq!(right, [0.0]);

        for rate in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(std::panic::catch_unwind(|| Reverb::new(rate)).is_err());
        }
    }
}
