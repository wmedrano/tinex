use wmidi::MidiMessage;

pub trait Plugin: 'static + Send {
    /// MIDI frame offsets are relative to this block and sorted in ascending order.
    /// Events at the same frame are applied in slice order, before rendering that frame.
    /// An offset equal to the block length updates state after its last sample.
    /// Every output sample must be written; output buffers may contain earlier audio.
    fn process(
        &mut self,
        midi: &[(usize, MidiMessage)],
        input: [&[f32]; 2],
        outputs: [&mut [f32]; 2],
    );
}

pub struct Silence;

impl Plugin for Silence {
    fn process(&mut self, _: &[(usize, MidiMessage)], _: [&[f32]; 2], outputs: [&mut [f32]; 2]) {
        for output in outputs {
            output.fill(0.0);
        }
    }
}
