use tinex_core::plugin::Plugin;
use wmidi::MidiMessage;

/// A 5 Hz, 50% depth tremolo with synchronized stereo modulation.
///
/// Gain varies smoothly between 0.5 and 1.0, starting at unity. MIDI is ignored.
/// Processing is allocation-free and preserves oscillator phase across blocks.
pub struct Tremolo {
    amount: f64,
    phase: f64,
    phase_step: f64,
}

impl Tremolo {
    /// Constructs a fresh tremolo at the given sample rate.
    ///
    /// Panics if the sample rate is not finite and positive.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            sample_rate.is_finite() && sample_rate > 0.0,
            "sample rate must be finite and positive"
        );
        Self {
            amount: 0.25,
            phase: 0.0,
            phase_step: (std::f64::consts::TAU * 5.0 / f64::from(sample_rate))
                % std::f64::consts::TAU,
        }
    }
}

impl Plugin for Tremolo {
    fn process(
        &mut self,
        _: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        outputs: [&mut [f32]; 2],
    ) {
        let [left, right] = outputs;
        let (a, b) = (self.amount, (1.0 - self.amount) as f32);
        for frame in 0..left.len().max(right.len()) {
            let gain = (a * self.phase.cos()) as f32 + b;
            if let Some(output) = left.get_mut(frame) {
                *output = input[0].get(frame).copied().unwrap_or(0.0) * gain;
            }
            if let Some(output) = right.get_mut(frame) {
                *output = input[1].get(frame).copied().unwrap_or(0.0) * gain;
            }
            self.phase = (self.phase + self.phase_step) % std::f64::consts::TAU;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tremolo_modulates_stereo_at_five_hz_with_half_depth() {
        for sample_rate in [20, 44_100, 48_000, 96_000] {
            let frames = sample_rate / 5;
            let left_input = vec![1.0; frames];
            let right_input = vec![-2.0; frames];
            let mut left = vec![99.0; frames];
            let mut right = vec![99.0; frames];
            Tremolo::new(sample_rate as f32).process(
                &[],
                [&left_input, &right_input],
                [&mut left, &mut right],
            );
            for (frame, (&left, &right)) in left.iter().zip(&right).enumerate() {
                let expected = (0.75
                    + 0.25 * (std::f64::consts::TAU * frame as f64 / frames as f64).cos())
                    as f32;
                assert!((left - expected).abs() < 1e-6);
                assert_eq!(right, -2.0 * left);
                assert!((0.5..=1.0).contains(&left));
            }
        }
    }

    #[test]
    fn tremolo_preserves_phase_across_blocks_and_ignores_midi() {
        let input = vec![1.0; 20_000];
        let mut expected = vec![0.0; input.len()];
        Tremolo::new(48_000.0).process(&[], [&input, &[]], [&mut expected, &mut []]);

        let mut tremolo = Tremolo::new(48_000.0);
        let mut actual = vec![99.0; input.len()];
        for (source, output) in input.chunks(127).zip(actual.chunks_mut(127)) {
            tremolo.process(&[(0, MidiMessage::Start)], [source, &[]], [output, &mut []]);
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn tremolo_handles_missing_inputs_and_unequal_output_lengths() {
        for longer_channel in 0..2 {
            let mut tremolo = Tremolo::new(20.0);
            let mut short = [99.0; 1];
            let mut long = [99.0; 3];
            let outputs = if longer_channel == 0 {
                [&mut long[..], &mut short[..]]
            } else {
                [&mut short[..], &mut long[..]]
            };
            tremolo.process(&[], [&[2.0, 0.0], &[]], outputs);
            if longer_channel == 0 {
                assert_eq!(long, [2.0, 0.0, 0.0]);
                assert_eq!(short, [0.0]);
            } else {
                assert_eq!(short, [2.0]);
                assert_eq!(long, [0.0; 3]);
            }

            // The longer output advances three frames; an empty block advances none.
            tremolo.process(&[], [&[1.0], &[1.0]], [&mut [], &mut []]);
            let mut next = [99.0];
            tremolo.process(&[], [&[1.0], &[]], [&mut next, &mut []]);
            assert!((next[0] - 0.75).abs() < 1e-6);
        }
    }

    #[test]
    fn tremolo_rejects_invalid_sample_rates() {
        for sample_rate in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(std::panic::catch_unwind(|| Tremolo::new(sample_rate)).is_err());
        }
    }
}
