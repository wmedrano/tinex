//! A reduced physical Rhodes: beam mechanics, hammer, damper, and magnetic pickup.
mod mechanics;
mod pickup;

use mechanics::{Assembly, NoteModel};
use pickup::Pickup;
use tinex_core::plugin::Plugin;
use wmidi::{MidiMessage, Note};

const VOICES: usize = 32;
const OVERSAMPLING: usize = 4;
const FILTER_TAPS: usize = 65;

/// Dry physical electric piano with 32 mechanical assemblies.
///
/// The preset is physically informed, rather than calibrated to a specific Rhodes.
/// Construction precomputes all modes, coefficients, and pickup tables. Processing
/// is allocation-free. Mechanics run at 4x; the output FIR has eight frames of delay.
/// MIDI channels and input audio are ignored. CC64 controls sustain; CC120,
/// CC121 and CC123 silence/reset/release the whole instrument.
/// The mono pickup sum is written to both output channels.
/// Cloning copies the calibrated models and current playback state without recalibration.
#[derive(Clone)]
pub struct EPiano {
    voices: [Option<Voice>; VOICES],
    fades: [Option<Fade>; VOICES],
    models: Box<[NoteModel; 128]>,
    pickup: Pickup,
    sustain: bool,
    clock: u64,
    internal_rate: f64,
    fade_samples: usize,
    filter: OutputFilter,
    solver_failures: u64,
}

#[derive(Clone)]
struct Voice {
    note: Note,
    started: u64,
    assembly: Assembly,
    flux: f64,
    last_sample: f64,
    samples: usize,
}

#[derive(Clone)]
struct Fade {
    voice: Voice,
    remaining: usize,
    failed: bool,
}

impl EPiano {
    /// Panics if the sample rate is not finite and positive.
    pub fn new(sample_rate: f32) -> Self {
        assert!(
            sample_rate.is_finite() && sample_rate > 0.0,
            "sample rate must be finite and positive"
        );
        let internal_rate = f64::from(sample_rate) * OVERSAMPLING as f64;
        let pickup = Pickup::new();
        let models = Box::new(std::array::from_fn(|note| {
            let note = note.try_into().unwrap();
            let mut model = NoteModel::new(note, internal_rate);
            calibrate_output(&mut model, &pickup, f64::from(sample_rate), note);
            model
        }));
        Self {
            voices: std::array::from_fn(|_| None),
            fades: std::array::from_fn(|_| None),
            models,
            pickup,
            sustain: false,
            clock: 0,
            internal_rate,
            fade_samples: (0.005 * internal_rate).ceil().max(1.0) as usize,
            filter: OutputFilter::new(f64::from(sample_rate)),
            solver_failures: 0,
        }
    }

    /// Number of bounded nonlinear solves that failed since construction.
    /// A failed voice is retired through a short fade, without logging in the callback.
    pub fn solver_failures(&self) -> u64 {
        self.solver_failures
    }

    fn note_on(&mut self, note: Note, velocity: u8) {
        let model = &self.models[usize::from(u8::from(note))];
        if model.count == 0 {
            return;
        }
        if let Some(voice) = self.voices.iter_mut().flatten().find(|v| v.note == note) {
            voice.assembly.strike(model, velocity);
            return;
        }
        let index = self
            .voices
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let a = a.as_ref().unwrap();
                        let b = b.as_ref().unwrap();
                        let group = |v: &Voice| {
                            if v.assembly.held {
                                2
                            } else if self.sustain {
                                1
                            } else {
                                0
                            }
                        };
                        group(a).cmp(&group(b)).then_with(|| {
                            if group(a) == 2 {
                                a.started.cmp(&b.started)
                            } else {
                                a.assembly
                                    .energy(&self.models[usize::from(u8::from(a.note))])
                                    .total_cmp(
                                        &b.assembly
                                            .energy(&self.models[usize::from(u8::from(b.note))]),
                                    )
                            }
                        })
                    })
                    .unwrap()
                    .0
            });
        if let Some(voice) = self.voices[index].take() {
            // One preallocated outgoing slot per voice. At most 32 simultaneous tails.
            self.fades[index] = Some(Fade {
                voice,
                remaining: self.fade_samples,
                failed: false,
            });
        }
        let mut assembly = Assembly::new();
        assembly.strike(model, velocity);
        self.clock = self.clock.saturating_add(1);
        self.voices[index] = Some(Voice {
            note,
            started: self.clock,
            assembly,
            flux: self.pickup.flux(0.0, 0.0, model.inverse_pickup_gap),
            last_sample: 0.0,
            samples: 0,
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
                        voice.assembly.held = false;
                    }
                }
            }
            MidiMessage::ControlChange(_, controller, value) => match u8::from(controller) {
                64 => self.sustain = u8::from(value) >= 64,
                120 => {
                    self.voices.fill(None);
                    self.fades.iter_mut().for_each(|fade| *fade = None);
                    self.filter.clear();
                }
                121 => self.sustain = false,
                123 => {
                    for voice in self.voices.iter_mut().flatten() {
                        voice.assembly.held = false;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn sample(&mut self) -> f32 {
        let dt = 1.0 / self.internal_rate;
        for _ in 0..OVERSAMPLING {
            let mut sample = 0.0;
            for index in 0..VOICES {
                let Some(voice) = &mut self.voices[index] else {
                    continue;
                };
                let model = &self.models[usize::from(u8::from(voice.note))];
                if !advance_voice(voice, model, &self.pickup, dt, self.sustain) {
                    self.solver_failures = self.solver_failures.saturating_add(1);
                    self.fades[index] = self.voices[index].take().map(|voice| Fade {
                        voice,
                        remaining: self.fade_samples,
                        failed: true,
                    });
                } else {
                    sample += voice.last_sample;
                    if voice.samples % 256 == 0
                        && voice.assembly.hammer.is_none()
                        && voice.assembly.energy(model) < voice.assembly.initial_energy * 1e-12
                    {
                        self.voices[index] = None;
                    }
                }
            }
            for fade in &mut self.fades {
                let Some(tail) = fade else {
                    continue;
                };
                let model = &self.models[usize::from(u8::from(tail.voice.note))];
                // Freeze a failed solve at its last valid voltage, then fade it.
                if !tail.failed && !advance_voice(&mut tail.voice, model, &self.pickup, dt, false) {
                    tail.failed = true;
                    self.solver_failures = self.solver_failures.saturating_add(1);
                }
                sample += tail.voice.last_sample * tail.remaining as f64 / self.fade_samples as f64;
                tail.remaining -= 1;
                if tail.remaining == 0 {
                    *fade = None;
                }
            }
            self.filter.push(sample);
        }
        self.filter.output() as f32
    }
}

// Per-key electrical voicing, measured once during construction. The calibration
// uses the complete hammer/pickup/filter path, so inaudible high modes cannot
// force an otherwise playable note down to an unnecessarily quiet output level.
fn calibrate_output(model: &mut NoteModel, pickup: &Pickup, rate: f64, note: u8) {
    if model.count == 0 {
        return;
    }
    let frequency = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
    let frames = (rate * (2.0 / frequency).clamp(0.02, 0.25))
        .ceil()
        .min(48000.0) as usize;
    let mut assembly = Assembly::new();
    assembly.strike(model, 127);
    let mut flux = pickup.flux(0.0, 0.0, model.inverse_pickup_gap);
    let mut filter = OutputFilter::new(rate);
    let mut peak = 0.0_f64;
    for _ in 0..frames {
        for _ in 0..OVERSAMPLING {
            if !assembly.advance(model, 1.0 / model.rate, false) {
                return; // Retain the conservative analytical gain on a failed calibration.
            }
            let [x, y] = assembly.tip;
            let next_flux = pickup.flux(x, y, model.inverse_pickup_gap);
            filter.push(-(next_flux - flux) * model.output_gain);
            flux = next_flux;
        }
        peak = peak.max(filter.output().abs());
    }
    if peak.is_finite() && peak > 1e-12 {
        model.output_gain *= 0.03 / peak;
    }
}

fn advance_voice(
    voice: &mut Voice,
    model: &NoteModel,
    pickup: &Pickup,
    dt: f64,
    pedal: bool,
) -> bool {
    if !voice.assembly.advance(model, dt, pedal) {
        return false;
    }
    let [x, y] = voice.assembly.tip;
    let flux = pickup.flux(x, y, model.inverse_pickup_gap);
    let voltage = -(flux - voice.flux) * model.output_gain;
    if !voltage.is_finite() {
        return false;
    }
    voice.flux = flux;
    voice.last_sample = voltage;
    voice.samples = voice.samples.saturating_add(1);
    true
}

impl Plugin for EPiano {
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

#[derive(Clone)]
struct OutputFilter {
    coefficients: [f64; FILTER_TAPS],
    history: [f64; FILTER_TAPS],
    cursor: usize,
    last_input: f64,
    last_output: f64,
    dc_pole: f64,
}

impl OutputFilter {
    fn new(rate: f64) -> Self {
        let cutoff = 0.1125; // 90% of output Nyquist at the 4x internal rate.
        let mut coefficients = std::array::from_fn(|i| {
            let x = i as f64 - (FILTER_TAPS / 2) as f64;
            let sinc = if x == 0.0 {
                2.0 * cutoff
            } else {
                (std::f64::consts::TAU * cutoff * x).sin() / (std::f64::consts::PI * x)
            };
            let angle = std::f64::consts::TAU * i as f64 / (FILTER_TAPS - 1) as f64;
            sinc * (0.42 - 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos())
        });
        let gain: f64 = coefficients.iter().sum();
        for c in &mut coefficients {
            *c /= gain;
        }
        Self {
            coefficients,
            history: [0.0; FILTER_TAPS],
            cursor: 0,
            last_input: 0.0,
            last_output: 0.0,
            dc_pole: (-std::f64::consts::TAU * 8.0 / rate).exp(),
        }
    }

    fn push(&mut self, sample: f64) {
        self.history[self.cursor] = sample;
        self.cursor = (self.cursor + 1) % FILTER_TAPS;
    }

    fn output(&mut self) -> f64 {
        let input: f64 = self
            .coefficients
            .iter()
            .enumerate()
            .map(|(i, c)| c * self.history[(self.cursor + FILTER_TAPS - 1 - i) % FILTER_TAPS])
            .sum();
        let output = input - self.last_input + self.dc_pole * self.last_output;
        self.last_input = input;
        self.last_output = if output.abs() < 1e-15 { 0.0 } else { output };
        self.last_output
    }

    fn clear(&mut self) {
        self.history.fill(0.0);
        self.last_input = 0.0;
        self.last_output = 0.0;
    }
}

#[cfg(test)]
mod tests;
