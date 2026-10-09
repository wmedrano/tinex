//! A warm, muted electric bass voiced for a smooth Motown-style sound.

use tinex_core::plugin::Plugin;
use wmidi::{MidiMessage, Note};

const VOICES: usize = 16;
const HARMONICS: usize = 8;
const SPECTRUM: [f64; HARMONICS] = [0.82, 0.32, 0.21, 0.12, 0.07, 0.04, 0.025, 0.015];
const DECAY_SECONDS: [f64; HARMONICS] = [1.1, 0.48, 0.30, 0.20, 0.15, 0.12, 0.10, 0.08];

/// A polyphonic electric bass with a soft pluck and quickly fading overtones.
///
/// It writes the same dry signal to both outputs and ignores input audio and MIDI
/// channels. CC64 controls sustain; CC120, CC121, and CC123 provide the usual
/// all-sound-off, reset-controllers, and all-notes-off behavior. Processing uses
/// fixed-size voice storage and does not allocate.
pub struct MotownBass {
    voices: [Option<Voice>; VOICES],
    tails: [Option<Tail>; VOICES],
    decay: [f64; HARMONICS],
    sample_rate: f64,
    attack_step: f64,
    release_decay: f64,
    bend_decay: f64,
    tone_alpha: f64,
    tone: f64,
    tail_samples: usize,
    sustain: bool,
    clock: u64,
}

#[derive(Clone, Copy)]
struct Voice {
    note: Note,
    started: u64,
    key_down: bool,
    phase: f64,
    phase_step: f64,
    pitch_bend: f64,
    levels: [f64; HARMONICS],
    attack: f64,
    release: f64,
}

struct Tail {
    voice: Voice,
    remaining: usize,
}

impl MotownBass {
    /// Constructs a fresh bass at the given sample rate.
    ///
    /// Panics if the sample rate is not finite and positive.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            sample_rate.is_finite() && sample_rate > 0.0,
            "sample rate must be finite and positive"
        );
        let rate = f64::from(sample_rate);
        Self {
            voices: [None; VOICES],
            tails: std::array::from_fn(|_| None),
            decay: DECAY_SECONDS.map(|seconds| (-1.0 / (rate * seconds)).exp()),
            sample_rate: rate,
            attack_step: 1.0 / (rate * 0.002),
            release_decay: (-1.0 / (rate * 0.085)).exp(),
            bend_decay: (-1.0 / (rate * 0.035)).exp(),
            tone_alpha: 1.0 - (-std::f64::consts::TAU * 1100.0 / rate).exp(),
            tone: 0.0,
            tail_samples: (rate * 0.004).ceil().max(1.0) as usize,
            sustain: false,
            clock: 0,
        }
    }

    fn note_on(&mut self, note: Note, velocity: u8) {
        let frequency = 440.0 * 2.0_f64.powf((f64::from(u8::from(note)) - 69.0) / 12.0);
        // Add only harmonics below Nyquist, leaving room for the slight attack bend.
        let harmonic_limit = (self.sample_rate * 0.45 / frequency).floor() as usize;
        if harmonic_limit == 0 {
            return;
        }
        let index = self
            .voices
            .iter()
            .position(|voice| voice.as_ref().is_some_and(|voice| voice.note == note))
            .or_else(|| self.voices.iter().position(Option::is_none))
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let a = a.as_ref().unwrap();
                        let b = b.as_ref().unwrap();
                        a.key_down
                            .cmp(&b.key_down)
                            .then_with(|| {
                                (a.levels[0] * a.release).total_cmp(&(b.levels[0] * b.release))
                            })
                            .then_with(|| a.started.cmp(&b.started))
                    })
                    .unwrap()
                    .0
            });
        if let Some(voice) = self.voices[index].take() {
            self.tails[index] = Some(Tail {
                voice,
                remaining: self.tail_samples,
            });
        }
        let velocity = (f64::from(velocity) / 127.0).powf(1.4);
        self.clock = self.clock.saturating_add(1);
        self.voices[index] = Some(Voice {
            note,
            started: self.clock,
            key_down: true,
            phase: 0.0,
            phase_step: std::f64::consts::TAU * frequency / self.sample_rate,
            pitch_bend: 0.003,
            levels: std::array::from_fn(|harmonic| {
                if harmonic < harmonic_limit {
                    SPECTRUM[harmonic] * velocity
                } else {
                    0.0
                }
            }),
            attack: 0.0,
            release: 1.0,
        });
    }

    fn event(&mut self, message: &MidiMessage<'_>) {
        match *message {
            MidiMessage::NoteOn(_, note, velocity) if u8::from(velocity) > 0 => {
                self.note_on(note, u8::from(velocity));
            }
            MidiMessage::NoteOff(_, note, _) | MidiMessage::NoteOn(_, note, _) => {
                for voice in self.voices.iter_mut().flatten() {
                    if voice.note == note {
                        voice.key_down = false;
                    }
                }
            }
            MidiMessage::ControlChange(_, controller, value) => match u8::from(controller) {
                64 => self.sustain = u8::from(value) >= 64,
                120 => {
                    self.voices.fill(None);
                    self.tails.iter_mut().for_each(|tail| *tail = None);
                    self.tone = 0.0;
                }
                121 => self.sustain = false,
                123 => {
                    for voice in self.voices.iter_mut().flatten() {
                        voice.key_down = false;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn sample(&mut self) -> f32 {
        let mut sum = 0.0;
        for voice in &mut self.voices {
            if let Some(active) = voice {
                sum += active.sample(
                    &self.decay,
                    self.attack_step,
                    self.release_decay,
                    self.bend_decay,
                    self.sustain,
                );
                if active.levels[0] * active.release < 1e-5 {
                    *voice = None;
                }
            }
        }
        for tail in &mut self.tails {
            if let Some(fade) = tail {
                sum += fade.voice.sample(
                    &self.decay,
                    self.attack_step,
                    self.release_decay,
                    self.bend_decay,
                    false,
                ) * fade.remaining as f64
                    / self.tail_samples as f64;
                fade.remaining -= 1;
                if fade.remaining == 0 {
                    *tail = None;
                }
            }
        }
        self.tone += self.tone_alpha * (sum - self.tone);
        if self.tone.abs() < 1e-15 {
            self.tone = 0.0;
        }
        // Soft DI-style saturation gives chords headroom without hard clipping.
        (self.tone * 0.32).tanh() as f32
    }
}

impl Voice {
    fn sample(
        &mut self,
        decay: &[f64; HARMONICS],
        attack_step: f64,
        release_decay: f64,
        bend_decay: f64,
        sustain: bool,
    ) -> f64 {
        self.phase += self.phase_step * (1.0 + self.pitch_bend);
        if self.phase >= std::f64::consts::TAU {
            self.phase -= std::f64::consts::TAU;
        }
        self.pitch_bend *= bend_decay;
        self.attack = (self.attack + attack_step).min(1.0);
        if !self.key_down && !sustain {
            self.release *= release_decay;
        }

        let (sin, cos) = self.phase.sin_cos();
        let (mut harmonic_sin, mut harmonic_cos) = (0.0, 1.0);
        let mut sum = 0.0;
        for (level, coefficient) in self.levels.iter_mut().zip(decay) {
            let next_sin = harmonic_sin * cos + harmonic_cos * sin;
            harmonic_cos = harmonic_cos * cos - harmonic_sin * sin;
            harmonic_sin = next_sin;
            sum += harmonic_sin * *level;
            *level *= coefficient;
        }
        sum * self.attack * self.release
    }
}

impl Plugin for MotownBass {
    fn process(&mut self, midi: &[(usize, MidiMessage)], _: [&[f32]; 2], outputs: [&mut [f32]; 2]) {
        let [left, right] = outputs;
        let frames = left.len().max(right.len());
        let mut events = midi.iter().peekable();
        for frame in 0..=frames {
            while let Some((offset, message)) = events.peek() {
                if *offset != frame {
                    break;
                }
                self.event(message);
                events.next();
            }
            if frame == frames {
                break;
            }
            let sample = self.sample();
            if let Some(output) = left.get_mut(frame) {
                *output = sample;
            }
            if let Some(output) = right.get_mut(frame) {
                *output = sample;
            }
        }
    }
}

#[cfg(test)]
mod tests;
