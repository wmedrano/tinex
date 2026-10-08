use super::Plugin;
use wmidi::{Channel, MidiMessage, Note};

const PIANO_VOICES: usize = 32;
const SILENCE_THRESHOLD: f64 = 1e-5;

/// `fm-piano`: a velocity-sensitive FM electric piano with a bright strike
/// and a warm body. Modulation decreases with pitch to soften the treble.
///
/// MIDI events take effect at their frame offset within a block. Audio input is ignored;
/// synthesized mono audio is written to both output channels. Only note-on and
/// note-off messages are supported (including zero-velocity note-on).
pub struct FmPiano {
    voices: [Option<PianoVoice>; PIANO_VOICES],
    sample_rate: f64,
    strike_attack_step: f64,
    body_attack_step: f64,
    strike_decay: f64,
    body_decay: f64,
    body_modulation_decay: f64,
    release_frames: f64,
}

struct PianoVoice {
    channel: Channel,
    note: Note,
    age: u64,
    phase: f64,
    phase_step: f64,
    velocity: f64,
    strike: Envelope,
    body: Envelope,
    strike_modulation: f64,
    body_modulation: f64,
    body_modulation_floor: f64,
    release_remaining: Option<f64>,
}

struct Envelope {
    amplitude: f64,
    attacking: bool,
}

impl Envelope {
    fn new() -> Self {
        Self {
            amplitude: 0.0,
            attacking: true,
        }
    }

    fn advance(&mut self, attack_step: f64, decay: f64) {
        if self.attacking {
            self.amplitude = (self.amplitude + attack_step).min(1.0);
            self.attacking = self.amplitude < 1.0;
        } else {
            self.amplitude *= decay;
        }
    }

    fn is_silent(&self) -> bool {
        !self.attacking && self.amplitude < SILENCE_THRESHOLD
    }
}

impl FmPiano {
    /// Creates the fixed electric piano preset with 32 voices.
    ///
    /// # Panics
    /// Panics if `sample_rate` is not finite and positive.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            sample_rate.is_finite() && sample_rate > 0.0,
            "sample rate must be finite and positive"
        );
        let sample_rate = f64::from(sample_rate);
        Self {
            voices: std::array::from_fn(|_| None),
            sample_rate,
            strike_attack_step: 1.0 / (0.001 * sample_rate),
            body_attack_step: 1.0 / (0.003 * sample_rate),
            strike_decay: (-1.0 / (0.120 * sample_rate)).exp(),
            body_decay: (-1.0 / (3.0 * sample_rate)).exp(),
            body_modulation_decay: (-1.0 / (0.250 * sample_rate)).exp(),
            release_frames: (0.200 * sample_rate).ceil().max(1.0),
        }
    }

    fn note_on(&mut self, channel: Channel, note: Note, velocity: u8) {
        for voice in self.voices.iter_mut().flatten() {
            voice.age = voice.age.saturating_add(1);
        }
        let index = self
            .voices
            .iter()
            .position(|voice| {
                voice
                    .as_ref()
                    .is_some_and(|voice| voice.channel == channel && voice.note == note)
            })
            .or_else(|| self.voices.iter().position(Option::is_none))
            .unwrap_or_else(|| {
                // Prefer releasing voices, then the oldest voice in that group.
                self.voices
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, voice)| {
                        let voice = voice.as_ref().unwrap();
                        (voice.release_remaining.is_some(), voice.age)
                    })
                    .unwrap()
                    .0
            });
        let frequency = 440.0 * 2.0_f64.powf((f64::from(u8::from(note)) - 69.0) / 12.0);
        let velocity = f64::from(velocity) / 127.0;
        // Richer bass, gentler treble, with bounded indices at either extreme.
        let modulation_scale = (440.0 / frequency).sqrt().clamp(0.35, 2.0);
        self.voices[index] = Some(PianoVoice {
            channel,
            note,
            age: 0,
            phase: 0.0,
            phase_step: (frequency / self.sample_rate).fract(),
            velocity,
            strike: Envelope::new(),
            body: Envelope::new(),
            strike_modulation: 4.0 * velocity * modulation_scale,
            body_modulation: 0.6 * velocity * modulation_scale,
            body_modulation_floor: 0.15 * velocity * modulation_scale,
            release_remaining: None,
        });
    }

    fn note_off(&mut self, channel: Channel, note: Note) {
        for voice in self.voices.iter_mut().flatten() {
            if voice.channel == channel && voice.note == note && voice.release_remaining.is_none() {
                voice.release_remaining = Some(self.release_frames);
            }
        }
    }

    fn next_sample(&mut self) -> f32 {
        let mut sample = 0.0;
        for slot in &mut self.voices {
            let Some(voice) = slot else { continue };
            voice
                .strike
                .advance(self.strike_attack_step, self.strike_decay);
            voice.body.advance(self.body_attack_step, self.body_decay);
            let release_gain = if let Some(remaining) = &mut voice.release_remaining {
                *remaining = (*remaining - 1.0).max(0.0);
                *remaining / self.release_frames
            } else {
                1.0
            };
            if release_gain == 0.0 || (voice.strike.is_silent() && voice.body.is_silent()) {
                *slot = None;
                continue;
            }
            let phase = std::f64::consts::TAU * voice.phase;
            let modulator = phase.sin();
            let strike =
                (phase + voice.strike_modulation * modulator).sin() * voice.strike.amplitude;
            let body = (phase + voice.body_modulation * modulator).sin() * voice.body.amplitude;
            sample += (0.3 * strike + 0.7 * body) * voice.velocity * release_gain;
            voice.phase = (voice.phase + voice.phase_step).fract();
            voice.strike_modulation *= self.strike_decay;
            // Retain mild harmonics in the body after the strike has faded.
            voice.body_modulation = voice.body_modulation_floor
                + (voice.body_modulation - voice.body_modulation_floor)
                    * self.body_modulation_decay;
        }
        (sample * 0.12).tanh() as f32
    }
}

impl Plugin for FmPiano {
    fn process(&mut self, midi: &[(usize, MidiMessage)], _: [&[f32]; 2], outputs: [&mut [f32]; 2]) {
        let [left, right] = outputs;
        let frames = left.len().max(right.len());
        let mut events = midi.iter().peekable();
        for index in 0..=frames {
            while let Some((frame, message)) = events.peek() {
                if *frame != index {
                    break;
                }
                match *message {
                    MidiMessage::NoteOn(channel, note, velocity) if u8::from(velocity) > 0 => {
                        self.note_on(channel, note, u8::from(velocity));
                    }
                    MidiMessage::NoteOn(channel, note, _)
                    | MidiMessage::NoteOff(channel, note, _) => {
                        self.note_off(channel, note);
                    }
                    _ => {}
                }
                events.next();
            }
            if index == frames {
                break;
            }
            let sample = self.next_sample();
            if let Some(output) = left.get_mut(index) {
                *output = sample;
            }
            if let Some(output) = right.get_mut(index) {
                *output = sample;
            }
        }
    }
}

#[cfg(test)]
mod piano_tests {
    use super::*;
    use wmidi::{Channel, Note, U7};

    fn note_on(channel: Channel, note: u8, velocity: u8) -> MidiMessage<'static> {
        MidiMessage::NoteOn(
            channel,
            Note::try_from(note).unwrap(),
            U7::try_from(velocity).unwrap(),
        )
    }

    fn render(piano: &mut FmPiano, midi: &[MidiMessage], frames: usize) -> Vec<f32> {
        let midi: Vec<_> = midi.iter().cloned().map(|message| (0, message)).collect();
        render_timed(piano, &midi, frames)
    }

    fn render_timed(piano: &mut FmPiano, midi: &[(usize, MidiMessage)], frames: usize) -> Vec<f32> {
        let mut left = vec![f32::NAN; frames];
        let mut right = vec![f32::NAN; frames];
        piano.process(midi, [&[], &[]], [&mut left, &mut right]);
        assert_eq!(left, right);
        assert!(
            left.iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
        );
        left
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|sample| sample * sample).sum()
    }

    #[test]
    fn silence_and_velocity_sensitive_notes() {
        let mut quiet = FmPiano::new(48_000.0);
        assert_eq!(render(&mut quiet, &[], 128), vec![0.0; 128]);
        let quiet_audio = render(&mut quiet, &[note_on(Channel::Ch1, 69, 32)], 4800);
        let mut loud = FmPiano::new(48_000.0);
        let loud_audio = render(&mut loud, &[note_on(Channel::Ch1, 69, 127)], 4800);
        assert!(energy(&quiet_audio) > 0.0);
        assert!(energy(&loud_audio) > energy(&quiet_audio));
    }

    #[test]
    fn release_and_zero_velocity_note_on_end_notes() {
        for zero_velocity in [false, true] {
            let mut piano = FmPiano::new(48_000.0);
            render(&mut piano, &[note_on(Channel::Ch1, 60, 100)], 480);
            let message = if zero_velocity {
                note_on(Channel::Ch1, 60, 0)
            } else {
                MidiMessage::NoteOff(Channel::Ch1, Note::try_from(60).unwrap(), U7::MIN)
            };
            let tail = render(&mut piano, &[message], 9601);
            assert!(energy(&tail[..480]) > 0.0);
            assert!(energy(&tail[..480]) > energy(&tail[9000..9480]));
            assert_eq!(tail[9600], 0.0);
            assert!(piano.voices.iter().all(Option::is_none));
        }
    }

    #[test]
    fn notes_are_isolated_by_channel_and_retriggered() {
        let mut piano = FmPiano::new(48_000.0);
        render(
            &mut piano,
            &[
                note_on(Channel::Ch1, 60, 100),
                note_on(Channel::Ch2, 60, 100),
            ],
            480,
        );
        render(&mut piano, &[note_on(Channel::Ch1, 60, 0)], 9601);
        let remaining: Vec<_> = piano.voices.iter().flatten().collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].channel, Channel::Ch2);
        render(&mut piano, &[note_on(Channel::Ch2, 60, 80)], 0);
        let remaining: Vec<_> = piano.voices.iter().flatten().collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].strike.amplitude, 0.0);
        assert_eq!(remaining[0].body.amplitude, 0.0);
        assert!(remaining[0].strike.attacking && remaining[0].body.attacking);
        assert_eq!(remaining[0].phase, 0.0);
        assert!(remaining[0].release_remaining.is_none());
        let mut fresh = FmPiano::new(48_000.0);
        assert_eq!(
            render(&mut piano, &[], 512),
            render(&mut fresh, &[note_on(Channel::Ch2, 60, 80)], 512)
        );
    }

    #[test]
    fn strike_fades_before_the_warm_body_at_each_sample_rate() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            let mut piano = FmPiano::new(sample_rate);
            render(
                &mut piano,
                &[note_on(Channel::Ch1, 69, 127)],
                (sample_rate * 0.6) as usize,
            );
            let voice = piano.voices.iter().flatten().next().unwrap();
            assert!(!voice.strike.attacking && !voice.body.attacking);
            assert!(voice.strike.amplitude < 0.01);
            assert!(voice.body.amplitude > 0.8);
            assert!(voice.strike_modulation < 0.03);
            assert!(voice.body_modulation > voice.body_modulation_floor);
            assert!(voice.body_modulation < 0.2);

            render(&mut piano, &[], (sample_rate * 1.4) as usize);
            let voice = piano.voices.iter().flatten().next().unwrap();
            assert!(voice.strike.is_silent());
            assert!(!voice.body.is_silent());
            assert!(voice.body_modulation_floor > 0.0);
            assert!(energy(&render(&mut piano, &[], 1024)) > 0.0);
        }
    }

    #[test]
    fn modulation_softens_with_pitch_and_responds_to_velocity() {
        let mut piano = FmPiano::new(48_000.0);
        let mut previous = f64::INFINITY;
        for note in 0..=127 {
            render(&mut piano, &[note_on(Channel::Ch1, note, 127)], 0);
            let voice = piano
                .voices
                .iter()
                .flatten()
                .find(|voice| u8::from(voice.note) == note)
                .unwrap();
            assert!((1.4..=8.0).contains(&voice.strike_modulation));
            assert!((0.21..=1.2).contains(&voice.body_modulation));
            assert!((0.0525..=0.3).contains(&voice.body_modulation_floor));
            assert!(voice.strike_modulation <= previous);
            previous = voice.strike_modulation;
        }

        for note in [36, 69, 96] {
            render(&mut piano, &[note_on(Channel::Ch1, note, 127)], 0);
            let loud = piano
                .voices
                .iter()
                .flatten()
                .find(|voice| u8::from(voice.note) == note)
                .unwrap();
            let indices = (
                loud.strike_modulation,
                loud.body_modulation,
                loud.body_modulation_floor,
            );
            render(&mut piano, &[note_on(Channel::Ch1, note, 32)], 0);
            let quiet = piano
                .voices
                .iter()
                .flatten()
                .find(|voice| u8::from(voice.note) == note)
                .unwrap();
            assert!(quiet.strike_modulation < indices.0);
            assert!(quiet.body_modulation < indices.1);
            assert!(quiet.body_modulation_floor < indices.2);
        }
    }

    #[test]
    fn release_during_attack_ends_both_layers_on_time() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            for attack_frames in [0, 1, (sample_rate * 0.002) as usize] {
                let mut piano = FmPiano::new(sample_rate);
                render(&mut piano, &[note_on(Channel::Ch1, 60, 127)], attack_frames);
                let release_frames = piano.release_frames as usize;
                let tail = render(
                    &mut piano,
                    &[note_on(Channel::Ch1, 60, 0)],
                    release_frames - 1,
                );
                assert!(energy(&tail) > 0.0);
                assert_eq!(piano.voices.iter().flatten().count(), 1);
                assert_eq!(render(&mut piano, &[], 1), vec![0.0]);
                assert!(piano.voices.iter().all(Option::is_none));
            }
        }
    }

    #[test]
    fn voice_overflow_prefers_releasing_then_oldest_held() {
        let mut piano = FmPiano::new(48_000.0);
        let notes: Vec<_> = (40..72)
            .map(|note| note_on(Channel::Ch1, note, 127))
            .collect();
        render(&mut piano, &notes, 480);
        let chord = render(&mut piano, &[], 480);
        assert!(energy(&chord) > 0.0);
        render(
            &mut piano,
            &[note_on(Channel::Ch1, 50, 0), note_on(Channel::Ch1, 72, 127)],
            0,
        );
        assert_eq!(piano.voices.iter().flatten().count(), PIANO_VOICES);
        assert!(
            !piano
                .voices
                .iter()
                .flatten()
                .any(|voice| u8::from(voice.note) == 50)
        );
        render(&mut piano, &[note_on(Channel::Ch1, 73, 127)], 0);
        assert!(
            !piano
                .voices
                .iter()
                .flatten()
                .any(|voice| u8::from(voice.note) == 40)
        );
        render(&mut piano, &[], 4800);
    }

    #[test]
    fn rendering_is_independent_of_block_size() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            let mut whole = FmPiano::new(sample_rate);
            let mut split = FmPiano::new(sample_rate);
            let expected = render(&mut whole, &[note_on(Channel::Ch1, 69, 100)], 2048);
            let mut actual = render(&mut split, &[note_on(Channel::Ch1, 69, 100)], 127);
            actual.extend(render(&mut split, &[], 1921));
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn midi_offsets_match_rendering_split_at_event_frames() {
        for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
            let mut whole = FmPiano::new(sample_rate);
            let mut split = FmPiano::new(sample_rate);
            let on = note_on(Channel::Ch1, 69, 100);
            let off = MidiMessage::NoteOff(Channel::Ch1, Note::A4, U7::MIN);
            let retrigger = note_on(Channel::Ch1, 69, 80);
            let zero_velocity = note_on(Channel::Ch1, 69, 0);
            let expected = render_timed(
                &mut whole,
                &[
                    (127, on.clone()),
                    (480, off.clone()),
                    (480, retrigger.clone()),
                    (800, zero_velocity.clone()),
                ],
                1200,
            );
            let mut actual = render(&mut split, &[], 127);
            actual.extend(render(&mut split, &[on], 353));
            actual.extend(render(&mut split, &[off, retrigger], 320));
            actual.extend(render(&mut split, &[zero_velocity], 400));
            assert_eq!(&expected[..127], &[0.0; 127]);
            assert!(energy(&expected[127..480]) > 0.0);
            assert_eq!(actual, expected);
            assert_eq!(render(&mut whole, &[], 1024), render(&mut split, &[], 1024));
        }
    }

    #[test]
    fn empty_and_unequal_outputs_advance_once_per_frame() {
        let mut piano = FmPiano::new(48_000.0);
        let mut reference = FmPiano::new(48_000.0);
        render(&mut piano, &[note_on(Channel::Ch1, 69, 100)], 0);
        let expected = render(&mut reference, &[note_on(Channel::Ch1, 69, 100)], 256);
        let mut left = [f32::NAN; 64];
        let mut right = [f32::NAN; 256];
        piano.process(&[], [&[1.0; 64], &[1.0; 256]], [&mut left, &mut right]);
        assert_eq!(left.as_slice(), &expected[..64]);
        assert_eq!(right.as_slice(), expected);
        assert_eq!(
            render(&mut piano, &[], 128),
            render(&mut reference, &[], 128)
        );
        let expected = render(&mut reference, &[], 128);
        let mut left = [f32::NAN; 128];
        piano.process(&[], [&[], &[]], [&mut left, &mut []]);
        assert_eq!(left.as_slice(), expected);
    }

    #[test]
    fn held_notes_decay_to_silence() {
        let mut piano = FmPiano::new(1000.0);
        let strike = render(&mut piano, &[note_on(Channel::Ch1, 48, 100)], 1000);
        let tail = render(&mut piano, &[], 1000);
        assert!(energy(&strike) > energy(&tail));
        render(&mut piano, &[], 35_000);
        assert!(piano.voices.iter().all(Option::is_none));
    }

    #[test]
    fn invalid_sample_rates_panic() {
        for rate in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(std::panic::catch_unwind(|| FmPiano::new(rate)).is_err());
        }
    }
}
