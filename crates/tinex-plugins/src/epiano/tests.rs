use super::*;
use wmidi::{Channel, ControlFunction, U7};

fn on(channel: Channel, note: u8, velocity: u8) -> MidiMessage<'static> {
    MidiMessage::NoteOn(
        channel,
        Note::try_from(note).unwrap(),
        U7::try_from(velocity).unwrap(),
    )
}
fn cc(channel: Channel, controller: u8, value: u8) -> MidiMessage<'static> {
    MidiMessage::ControlChange(
        channel,
        ControlFunction::from(U7::try_from(controller).unwrap()),
        U7::try_from(value).unwrap(),
    )
}
fn render(piano: &mut EPiano, events: &[(usize, MidiMessage)], frames: usize) -> Vec<f32> {
    let mut left = vec![f32::NAN; frames];
    let mut right = vec![f32::NAN; frames];
    piano.process(events, [&[], &[]], [&mut left, &mut right]);
    assert_eq!(left, right);
    assert!(left.iter().all(|s| s.is_finite()));
    assert_eq!(piano.solver_failures(), 0);
    left
}
fn energy(audio: &[f32]) -> f64 {
    audio.iter().map(|&s| f64::from(s).powi(2)).sum()
}

#[test]
fn silence_velocity_release_and_restrike() {
    let mut quiet = EPiano::new(48000.0);
    // Clone before rendering so both instruments start with independent, silent state.
    let mut loud = quiet.clone();
    assert_eq!(render(&mut quiet, &[], 64), [0.0; 64]);
    let soft = render(&mut quiet, &[(0, on(Channel::Ch1, 60, 24))], 4800);
    let hard = render(&mut loud, &[(0, on(Channel::Ch1, 60, 127))], 4800);
    assert!(energy(&soft) > 0.0 && energy(&hard) > energy(&soft) * 4.0);
    let state = loud.voices.iter().flatten().next().unwrap().assembly.q;
    render(&mut loud, &[(0, on(Channel::Ch1, 60, 64))], 0);
    assert_eq!(loud.voices.iter().flatten().count(), 1);
    assert_eq!(
        loud.voices.iter().flatten().next().unwrap().assembly.q,
        state
    );
    let tail = render(&mut loud, &[(0, on(Channel::Ch1, 60, 0))], 24000);
    assert!(energy(&tail[..480]) > energy(&tail[20000..20480]) * 1000.0);
    render(&mut loud, &[], 24000);
    assert!(loud.voices.iter().all(Option::is_none));
}

#[test]
fn midi_timing_block_sizes_and_unequal_outputs() {
    for rate in [44100.0, 48000.0, 96000.0] {
        let mut whole = EPiano::new(rate);
        let mut split = whole.clone();
        let events = [
            (127, on(Channel::Ch1, 60, 127)),
            (480, cc(Channel::Ch1, 64, 127)),
            (500, on(Channel::Ch1, 60, 0)),
            (800, on(Channel::Ch1, 60, 48)),
            (900, cc(Channel::Ch1, 64, 0)),
        ];
        let expected = render(&mut whole, &events, 1200);
        let mut actual = render(&mut split, &[], 127);
        actual.extend(render(&mut split, &[(0, events[0].1.clone())], 353));
        actual.extend(render(
            &mut split,
            &[(0, events[1].1.clone()), (20, events[2].1.clone())],
            320,
        ));
        actual.extend(render(
            &mut split,
            &[(0, events[3].1.clone()), (100, events[4].1.clone())],
            400,
        ));
        assert_eq!(expected, actual);
        assert_eq!(expected[..127], [0.0; 127]);
        let expected = render(&mut whole, &[], 256);
        let mut left = [f32::NAN; 64];
        let mut right = [f32::NAN; 256];
        split.process(&[], [&[1.0; 64], &[1.0; 256]], [&mut left, &mut right]);
        assert_eq!(left.as_slice(), &expected[..64]);
        assert_eq!(right.as_slice(), expected);
        assert_eq!(render(&mut whole, &[], 64), render(&mut split, &[], 64));
        // An event at the block end changes state without rendering an extra frame.
        render(&mut split, &[(0, on(Channel::Ch2, 69, 100))], 0);
        assert!(split.voices.iter().flatten().any(|v| v.note == Note::A4));
    }
}

#[test]
fn channels_are_ignored_for_notes_pedal_and_controllers() {
    let mut piano = EPiano::new(48000.0);
    let mut reference = piano.clone();
    let mixed = [
        (0, cc(Channel::Ch2, 64, 127)),
        (0, on(Channel::Ch1, 60, 100)),
        (0, on(Channel::Ch3, 64, 100)),
        (100, on(Channel::Ch16, 60, 0)),
        (100, on(Channel::Ch8, 64, 0)),
        (24000, cc(Channel::Ch4, 121, 0)),
    ];
    let same = [
        (0, cc(Channel::Ch1, 64, 127)),
        (0, on(Channel::Ch1, 60, 100)),
        (0, on(Channel::Ch1, 64, 100)),
        (100, on(Channel::Ch1, 60, 0)),
        (100, on(Channel::Ch1, 64, 0)),
        (24000, cc(Channel::Ch1, 121, 0)),
    ];
    assert_eq!(
        render(&mut piano, &mixed, 72000),
        render(&mut reference, &same, 72000)
    );
    assert!(piano.voices.iter().all(Option::is_none));
    assert!(!piano.sustain);
    render(
        &mut piano,
        &[
            (0, on(Channel::Ch1, 60, 100)),
            (0, cc(Channel::Ch2, 64, 127)),
            (100, cc(Channel::Ch3, 123, 0)),
        ],
        500,
    );
    assert!(!piano.voices[0].as_ref().unwrap().assembly.held);
    assert_eq!(piano.voices[0].as_ref().unwrap().assembly.damper, 0.0);
    render(&mut piano, &[(0, on(Channel::Ch2, 60, 80))], 0);
    assert_eq!(piano.voices.iter().flatten().count(), 1);
    assert!(piano.voices[0].as_ref().unwrap().assembly.held);
    render(&mut piano, &[(0, cc(Channel::Ch16, 120, 0))], 0);
    assert!(piano.voices.iter().all(Option::is_none));
    assert!(piano.fades.iter().all(Option::is_none));
    assert_eq!(render(&mut piano, &[], 100), [0.0; 100]);
}

#[test]
fn stealing_preserves_tail_and_prefers_released_then_oldest() {
    let mut piano = EPiano::new(48000.0);
    let events: Vec<_> = (40..72)
        .map(|note| (0, on(Channel::Ch1, note, 100)))
        .collect();
    render(&mut piano, &events, 1000);
    render(
        &mut piano,
        &[(0, on(Channel::Ch1, 50, 0)), (0, on(Channel::Ch1, 72, 127))],
        0,
    );
    assert_eq!(piano.voices.iter().flatten().count(), VOICES);
    assert!(
        !piano
            .voices
            .iter()
            .flatten()
            .any(|v| u8::from(v.note) == 50)
    );
    assert!(
        piano
            .fades
            .iter()
            .flatten()
            .any(|f| u8::from(f.voice.note) == 50)
    );
    render(&mut piano, &[(0, on(Channel::Ch1, 73, 127))], 0);
    assert!(
        !piano
            .voices
            .iter()
            .flatten()
            .any(|v| u8::from(v.note) == 40)
    );
    render(&mut piano, &[], 300);
    assert!(piano.fades.iter().all(Option::is_none));
}

#[test]
fn every_note_extreme_velocity_and_release_during_contact_stays_bounded() {
    for rate in [44100.0, 48000.0, 96000.0] {
        let mut piano = EPiano::new(rate);
        let mut peak = 0.0_f32;
        for note in 0..=127 {
            for velocity in [1, 127] {
                let audio = render(
                    &mut piano,
                    &[
                        (0, cc(Channel::Ch1, 120, 0)),
                        (0, on(Channel::Ch1, note, velocity)),
                        (1, on(Channel::Ch1, note, 0)),
                    ],
                    1024,
                );
                peak = peak.max(audio.iter().map(|s| s.abs()).fold(0.0, f32::max));
                for voice in piano.voices.iter().flatten() {
                    let model = &piano.models[usize::from(note)];
                    assert!(voice.assembly.energy(model) < voice.assembly.initial_energy * 1.001);
                }
            }
        }
        assert!(peak < 1.0, "rate {rate}, peak {peak}");
    }
}

#[test]
fn held_notes_eventually_retire() {
    let mut piano = EPiano::new(8000.0);
    render(&mut piano, &[(0, on(Channel::Ch1, 84, 100))], 8000 * 40);
    assert!(piano.voices.iter().all(Option::is_none));
}

#[test]
fn invalid_sample_rates_panic() {
    for rate in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(std::panic::catch_unwind(|| EPiano::new(rate)).is_err());
    }
    // A note above the retained mechanical band at a low rate stays silent.
    let mut piano = EPiano::new(1000.0);
    assert_eq!(
        render(&mut piano, &[(0, on(Channel::Ch1, 127, 127))], 128),
        [0.0; 128]
    );
}

#[test]
fn downsampler_rejects_out_of_band_and_dc() {
    let band_energy = |frequency: f64| {
        let mut filter = OutputFilter::new(48000.0);
        let mut energy = 0.0;
        for frame in 0..4096 {
            for step in 0..OVERSAMPLING {
                let i = frame * OVERSAMPLING + step;
                filter.push((std::f64::consts::TAU * frequency * i as f64 / 192000.0).sin());
            }
            let s = filter.output();
            if frame >= FILTER_TAPS {
                energy += s * s;
            }
        }
        energy
    };
    let pass = band_energy(3000.0);
    assert!(pass > 1900.0);
    assert!(band_energy(36000.0) < pass * 1e-6);
    assert!(band_energy(0.0) == 0.0);
    let mut filter = OutputFilter::new(48000.0);
    for _ in 0..48000 {
        for _ in 0..OVERSAMPLING {
            filter.push(1.0);
        }
        filter.output();
    }
    assert_eq!(filter.last_output, 0.0);
}

#[test]
#[ignore = "manual profiling aid"]
fn profile_components() {
    use std::time::Instant;
    let model = NoteModel::new(60, 192000.0);
    let mut assembly = Assembly::new();
    assembly.strike(&model, 127);
    let start = Instant::now();
    for _ in 0..192000 {
        assert!(assembly.advance(&model, 1.0 / 192000.0, false));
        std::hint::black_box(&assembly);
    }
    eprintln!(
        "Mechanics: {:.1} ns/internal step",
        start.elapsed().as_secs_f64() * 1e9 / 192000.0
    );
    let pickup = Pickup::new();
    let start = Instant::now();
    for i in 0..192000 {
        std::hint::black_box(pickup.flux((i as f64 % 100.0) * 1e-5, 0.0001, 125.0));
    }
    eprintln!(
        "Pickup: {:.1} ns/internal step",
        start.elapsed().as_secs_f64() * 1e9 / 192000.0
    );
    let mut filter = OutputFilter::new(48000.0);
    let start = Instant::now();
    for i in 0..48000 {
        for _ in 0..4 {
            filter.push(i as f64);
        }
        std::hint::black_box(filter.output());
    }
    eprintln!(
        "Output: {:.1} ns/output frame",
        start.elapsed().as_secs_f64() * 1e9 / 48000.0
    );
}

#[test]
fn failed_solve_retires_once_through_a_finite_fade() {
    let mut piano = EPiano::new(48000.0);
    render(&mut piano, &[(0, on(Channel::Ch1, 60, 127))], 480);
    piano.voices.iter_mut().flatten().next().unwrap().assembly.q[0] = f64::NAN;
    let mut left = [0.0; 1024];
    let mut right = [0.0; 1024];
    piano.process(&[], [&[], &[]], [&mut left, &mut right]);
    assert_eq!(piano.solver_failures(), 1);
    assert_eq!(left, right);
    assert!(left.iter().all(|s| s.is_finite()));
    assert!(piano.voices.iter().all(Option::is_none));
    assert!(piano.fades.iter().all(Option::is_none));
    piano.process(&[], [&[], &[]], [&mut left, &mut right]);
    assert_eq!(piano.solver_failures(), 1);
}

#[test]
fn playable_registers_have_consistent_gain_and_polyphonic_headroom() {
    let mut piano = EPiano::new(48000.0);
    for note in [21, 36, 48, 60, 72, 84, 96, 108] {
        let audio = render(&mut piano, &[(0, on(Channel::Ch1, note, 127))], 12000);
        let peak = audio.iter().map(|s| s.abs()).fold(0.0, f32::max);
        assert!((0.0275..0.045).contains(&peak), "note {note}, peak {peak}");
        render(&mut piano, &[(0, cc(Channel::Ch2, 120, 0))], 0);
    }
    let mut events: Vec<_> = (40..72)
        .map(|note| (0, on(Channel::Ch1, note, 127)))
        .collect();
    // Repeated strikes retain the vibrating tine; then replace every held note,
    // exercising 32 outgoing fades alongside 32 incoming assemblies.
    events.extend((40..72).map(|note| (128, on(Channel::Ch3, note, 127))));
    events.extend((72..104).map(|note| (480, on(Channel::Ch16, note, 127))));
    let audio = render(&mut piano, &events, 12000);
    let peak = audio.iter().map(|s| s.abs()).fold(0.0, f32::max);
    assert!(peak < 1.0, "32 voices and steal tails, peak {peak}");
}

#[test]
fn rendered_fundamental_stays_in_tune_at_soft_and_hard_velocities() {
    let mut worst = 0.0_f64;
    for rate in [44100.0, 48000.0, 96000.0] {
        let mut piano = EPiano::new(rate);
        for note in [36, 48, 60, 72, 84, 96] {
            for velocity in [24, 127] {
                render(&mut piano, &[(0, cc(Channel::Ch16, 120, 0))], 0);
                let audio = render(
                    &mut piano,
                    &[(0, on(Channel::Ch2, note, velocity))],
                    (rate * 0.35) as usize,
                );
                let audio = &audio[(rate * 0.1) as usize..];
                let expected = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
                let power = |cents: f64| {
                    let angle = std::f64::consts::TAU * expected * 2.0_f64.powf(cents / 1200.0)
                        / f64::from(rate);
                    let (s, c) = angle.sin_cos();
                    let (mut re, mut im, mut osc_re, mut osc_im) = (0.0, 0.0, 1.0, 0.0);
                    for (i, &sample) in audio.iter().enumerate() {
                        let window = 0.5
                            - 0.5
                                * (std::f64::consts::TAU * i as f64 / (audio.len() - 1) as f64)
                                    .cos();
                        re += f64::from(sample) * window * osc_re;
                        im += f64::from(sample) * window * osc_im;
                        (osc_re, osc_im) = (osc_re * c - osc_im * s, osc_im * c + osc_re * s);
                    }
                    re * re + im * im
                };
                let cents = (-10_i32..=10)
                    .max_by(|&a, &b| power(f64::from(a)).total_cmp(&power(f64::from(b))))
                    .unwrap();
                worst = worst.max(f64::from(cents).abs());
                assert!(
                    cents.abs() <= 5,
                    "note {note}, velocity {velocity}, rate {rate}: {cents} cents"
                );
            }
        }
    }
    eprintln!("Worst rendered fundamental error on a 1-cent search grid: {worst:.0} cents");
}

#[test]
fn nonlinear_treble_pickup_rejects_folded_harmonics() {
    let pickup = Pickup::new();
    let mut filter = OutputFilter::new(48000.0);
    let mut previous = pickup.flux(0.0, 0.0, 125.0);
    let mut audio = Vec::new();
    // Strong pickup nonlinearity at a representative high tine partial. Its
    // second and third harmonics would fold to 18 kHz and 3 kHz without the FIR.
    for frame in 0..10080 {
        for substep in 0..OVERSAMPLING {
            let time = (frame * OVERSAMPLING + substep + 1) as f64 / 192000.0;
            let x = 0.004 * (std::f64::consts::TAU * 15000.0 * time).sin();
            let flux = pickup.flux(x, 0.0, 125.0);
            filter.push((flux - previous) * 192000.0);
            previous = flux;
        }
        let sample = filter.output();
        if frame >= 480 {
            audio.push(sample);
        }
    }
    // The measurement covers integer periods of all three frequencies.
    let power = |frequency: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &sample) in audio.iter().enumerate() {
            let angle = std::f64::consts::TAU * frequency * i as f64 / 48000.0;
            re += sample * angle.cos();
            im += sample * angle.sin();
        }
        re * re + im * im
    };
    let reference = power(15000.0);
    for frequency in [3000.0, 18000.0] {
        let db = 10.0 * (power(frequency) / reference).log10();
        eprintln!(
            "Folded nonlinear pickup harmonic at {frequency:.0} Hz: {db:.1} dB relative to partial"
        );
        assert!(db < -75.0);
    }
}
