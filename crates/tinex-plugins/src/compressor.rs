use tinex_core::plugin::Plugin;
use wmidi::MidiMessage;

/// A stereo-linked peak compressor with a -18 dB threshold, 4:1 ratio,
/// 6 dB soft knee, 10 ms attack, and 100 ms release. MIDI is ignored.
///
/// Both channels share one detector and gain, so stereo balance is preserved.
/// Processing is allocation-free and the detector continues across blocks.
pub struct Compressor {
    envelope: f64,
    attack_coefficient: f64,
    release_coefficient: f64,
}

impl Compressor {
    /// Constructs a fresh compressor at the given sample rate.
    ///
    /// Panics if the sample rate is not finite and positive.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            sample_rate.is_finite() && sample_rate > 0.0,
            "sample rate must be finite and positive"
        );
        let sample_rate = f64::from(sample_rate);
        Self {
            envelope: 0.0,
            attack_coefficient: (-1.0 / (0.010 * sample_rate)).exp(),
            release_coefficient: (-1.0 / (0.100 * sample_rate)).exp(),
        }
    }

    fn gain(&self) -> f32 {
        // Below the knee, the signal passes through unchanged. This also avoids
        // taking the logarithm of silence.
        if self.envelope <= 10.0_f64.powf(-21.0 / 20.0) {
            return 1.0;
        }

        let above_threshold_db = 20.0 * self.envelope.log10() + 18.0;
        let reduction_db = if above_threshold_db < 3.0 {
            // Quadratic soft knee, continuous in both gain and slope.
            0.75 * (above_threshold_db + 3.0).powi(2) / 12.0
        } else {
            0.75 * above_threshold_db
        };
        10.0_f64.powf(-reduction_db / 20.0) as f32
    }
}

impl Plugin for Compressor {
    fn process(
        &mut self,
        _: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        outputs: [&mut [f32]; 2],
    ) {
        let [left, right] = outputs;
        for frame in 0..left.len().max(right.len()) {
            let left_input = input[0].get(frame).copied().unwrap_or(0.0);
            let right_input = input[1].get(frame).copied().unwrap_or(0.0);
            let peak = f64::from(left_input.abs().max(right_input.abs()));
            let coefficient = if peak > self.envelope {
                self.attack_coefficient
            } else {
                self.release_coefficient
            };
            self.envelope = coefficient * self.envelope + (1.0 - coefficient) * peak;
            if self.envelope < 1e-15 {
                self.envelope = 0.0;
            }

            let gain = self.gain();
            if let Some(output) = left.get_mut(frame) {
                *output = left_input * gain;
            }
            if let Some(output) = right.get_mut(frame) {
                *output = right_input * gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_audio_passes_through_and_loud_audio_reaches_four_to_one_ratio() {
        let mut compressor = Compressor::new(1_000.0);
        let quiet = [0.05; 200];
        let loud = [1.0; 1_000];
        let mut output = [f32::NAN; 1_000];
        compressor.process(&[], [&quiet, &[]], [&mut output[..200], &mut []]);
        assert_eq!(&output[..200], &quiet);

        compressor.process(&[], [&loud, &[]], [&mut output, &mut []]);
        assert!(output[0] > 0.8); // Attack lets the initial transient through.
        let expected_gain = 10.0_f32.powf(-13.5 / 20.0);
        assert!((output[999] - expected_gain).abs() < 1e-5);
        assert!(output[10] > output[100]);
    }

    #[test]
    fn one_gain_controls_both_channels_and_recovers_after_release() {
        let mut compressor = Compressor::new(1_000.0);
        let left_input = [1.0; 1_000];
        let right_input = [-0.5; 1_000];
        let mut left = [0.0; 1_000];
        let mut right = [0.0; 1_000];
        compressor.process(&[], [&left_input, &right_input], [&mut left, &mut right]);
        for (&l, &r) in left.iter().zip(&right) {
            assert_eq!(r, -0.5 * l);
        }

        let silence = [0.0; 1_000];
        compressor.process(&[], [&silence, &silence], [&mut left, &mut right]);
        let quiet = [0.05];
        compressor.process(&[], [&quiet, &quiet], [&mut left[..1], &mut right[..1]]);
        assert_eq!(left[0], quiet[0]);
        assert_eq!(right[0], quiet[0]);
    }

    #[test]
    fn block_boundaries_midi_and_missing_inputs_do_not_change_the_result() {
        let input = [0.5; 1_001];
        let mut whole = [0.0; 1_001];
        Compressor::new(48_000.0).process(&[], [&input, &[]], [&mut whole, &mut []]);

        let mut split = [0.0; 1_001];
        let mut compressor = Compressor::new(48_000.0);
        for (source, target) in input.chunks(73).zip(split.chunks_mut(73)) {
            compressor.process(&[(0, MidiMessage::Start)], [source, &[]], [target, &mut []]);
        }
        assert_eq!(split, whole);

        let mut left = [99.0; 2];
        let mut right = [99.0; 3];
        Compressor::new(48_000.0).process(&[], [&[0.05], &[]], [&mut left, &mut right]);
        assert_eq!(left, [0.05, 0.0]);
        assert_eq!(right, [0.0; 3]);
    }

    #[test]
    fn factory_builds_compressor_and_invalid_rates_are_rejected() {
        let builder = crate::FACTORY
            .iter()
            .find(|builder| builder.name() == "Compressor")
            .unwrap();
        let mut plugin = builder.build(48_000.0);
        let mut output = [f32::NAN; 2];
        plugin.process(&[], [&[0.05; 2], &[]], [&mut output, &mut []]);
        assert_eq!(output, [0.05; 2]);

        for rate in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(std::panic::catch_unwind(|| Compressor::new(rate)).is_err());
        }
    }
}
