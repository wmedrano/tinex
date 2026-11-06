use std::f32::consts::TAU;

use crate::{Beat, TimeSignature, Transport};

const DOWNBEAT_FREQUENCY_HZ: f32 = 1200.0;
const BEAT_FREQUENCY_HZ: f32 = 800.0;
const DOWNBEAT_AMPLITUDE: f32 = 1.0;
const BEAT_AMPLITUDE: f32 = 0.7;
/// Exponential decay time constant in seconds (~45 ms to fall by 95%).
const CLICK_TIME_CONSTANT_SECONDS: f32 = 0.015;
const ENVELOPE_FLOOR: f32 = 1e-5;

#[derive(Default)]
pub(crate) struct Metronome {
    phase: f32,
    frequency: f32,
    envelope: f32,
}

impl Metronome {
    pub(crate) fn process(
        &mut self,
        beats: &[Beat],
        transport: Transport,
        sample_rate: u32,
        output: &mut [&mut [f32]; 2],
    ) {
        if !transport.playing || sample_rate == 0 {
            self.envelope = 0.0;
            self.phase = 0.0;
            return;
        }
        let volume = if transport.metronome_volume.is_finite() && transport.metronome_volume > 0.0 {
            transport.metronome_volume
        } else {
            0.0
        };
        let samples = beats.len().saturating_sub(1);
        if samples == 0 || beats[0] == beats[1] {
            return;
        }
        let inv_sample_rate = 1.0 / (sample_rate as f32);
        let decay = (-inv_sample_rate / CLICK_TIME_CONSTANT_SECONDS).exp();
        for frame in 0..samples {
            if let Some(beat_index) =
                crossed_beat(beats[frame].as_f64(), beats[frame + 1].as_f64(), transport)
            {
                self.trigger(beat_index, transport.time_signature);
            }
            if self.envelope <= 0.0 {
                continue;
            }
            self.phase = (self.phase + self.frequency * inv_sample_rate).fract();
            let sample = self.envelope * (self.phase * TAU).sin() * volume;
            self.envelope *= decay;
            if self.envelope < ENVELOPE_FLOOR {
                self.envelope = 0.0;
            }
            if volume > 0.0 {
                for channel in output.iter_mut() {
                    if let Some(dst) = channel.get_mut(frame) {
                        *dst += sample;
                    }
                }
            }
        }
    }

    fn trigger(&mut self, beat_index: u64, time_signature: TimeSignature) {
        let beats_per_bar = u64::from(time_signature.numerator.max(1));
        self.phase = 0.0;
        if beat_index.is_multiple_of(beats_per_bar) {
            self.frequency = DOWNBEAT_FREQUENCY_HZ;
            self.envelope = DOWNBEAT_AMPLITUDE;
        } else {
            self.frequency = BEAT_FREQUENCY_HZ;
            self.envelope = BEAT_AMPLITUDE;
        }
    }
}

/// Returns the integer beat crossed during `[start, end)`, accounting for loop wraparound.
fn crossed_beat(start: f64, end: f64, transport: Transport) -> Option<u64> {
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end < 0.0 {
        return None;
    }
    if start < end {
        let candidate = start.ceil();
        (candidate < end).then_some(candidate as u64)
    } else if end < start && transport.bpm > 0.0 {
        if end > 0.0 {
            Some((end.ceil() - 1.0).max(0.0) as u64)
        } else {
            let length = f64::from(transport.measures) * f64::from(transport.time_signature.numerator);
            let candidate = start.ceil();
            (candidate < length).then_some(candidate as u64)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_accented_downbeat_and_regular_beats_across_loop_wraps() {
        let arena = bumpalo::Bump::new();
        // 1 bar of 4/4 at 60 bpm and 8_000 Hz: use a small sample rate where 1200 Hz and 800 Hz
        // are well below Nyquist (e.g. 8 Hz with 4 frames per beat at 120 bpm -> 2 beats/sec -> 4 frames/beat).
        let mut transport = Transport {
            bpm: 120.0,
            playing: true,
            metronome_volume: 1.0,
            measures: 1,
            ..Transport::default()
        };
        let beats = transport.compute_beat_at_frame(20, 8, &arena);
        let mut metronome = Metronome::default();
        let mut left = [0.0; 20];
        let mut right = [0.0; 20];
        metronome.process(beats, transport, 8_000, &mut [&mut left, &mut right]);

        assert_eq!(left, right);
        assert!(left[0].abs() > left[4].abs(), "downbeat is louder than beat 2");
        assert!((left[4].abs() - left[8].abs()).abs() < 1e-5);
        assert!(
            (left[0].abs() - left[16].abs()).abs() < 1e-5,
            "wrapped bar 1 beat 1 is a downbeat"
        );
    }
}
