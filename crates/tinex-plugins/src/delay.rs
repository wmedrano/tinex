use tinex_core::plugin::Plugin;
use wmidi::MidiMessage;

/// A 250 ms stereo delay with 35% feedback and a 30% wet mix.
///
/// Channels have independent echo tails. MIDI is ignored, missing input samples
/// are treated as silence, and processing is allocation-free across blocks.
pub struct Delay {
    buffer: Box<[[f32; 2]]>,
    position: usize,
}

impl Delay {
    /// Constructs a fresh delay at the given sample rate.
    ///
    /// The delay length is rounded to the nearest sample, with a one-sample minimum.
    /// Panics if the sample rate is not finite and positive.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            sample_rate.is_finite() && sample_rate > 0.0,
            "sample rate must be finite and positive"
        );
        let frames = (f64::from(sample_rate) * 0.25).round().max(1.0) as usize;
        Self {
            buffer: vec![[0.0; 2]; frames].into_boxed_slice(),
            position: 0,
        }
    }
}

impl Plugin for Delay {
    fn process(
        &mut self,
        _: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        mut outputs: [&mut [f32]; 2],
    ) {
        for frame in 0..outputs[0].len().max(outputs[1].len()) {
            for (channel, output) in outputs.iter_mut().enumerate() {
                let dry = input[channel].get(frame).copied().unwrap_or(0.0);
                let wet = self.buffer[self.position][channel];
                self.buffer[self.position][channel] = dry + wet * 0.35;
                if let Some(sample) = output.get_mut(frame) {
                    *sample = dry * 0.7 + wet * 0.3;
                }
            }
            self.position += 1;
            if self.position == self.buffer.len() {
                self.position = 0;
            }
        }
    }
}
